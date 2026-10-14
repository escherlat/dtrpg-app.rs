//! Catalog cache: read and write the catalog to/from a JSON file on disk.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

use crate::data::constants::{
    CATALOG_CACHE_FILE, CATALOG_CACHE_METADATA_FILE, CATALOG_CACHE_TMP, STALE_SECS,
};
use crate::data::library::LibraryItem;

/// Bump whenever a change to `LibraryItem`'s cached fields means existing
/// on-disk caches must be treated as stale (forcing a live refetch) rather
/// than silently missing newly populated data.
///
/// Version 2: `cover_url` mapping was fixed after caches already existed in
/// the wild with `cover_url: null` for every item (the field itself has
/// always round-tripped via `#[serde(default)]`, so old caches load without
/// error — they just carry no cover data). Without this version bump, a
/// fresh-looking cache with a matching remote item count silently skips the
/// live fetch for up to 7 days, disabling thumbnail loading with no visible
/// error. Caches written before this field existed deserialize
/// `schema_version` as `0` via `#[serde(default)]`, which never matches the
/// current version and is always treated as stale.
///
/// Version 3: `detail_cover_url` was added for the detail panel's
/// large-context cover. Caches written before this field existed
/// deserialize it as `None` via `#[serde(default)]` and would otherwise never
/// populate it until the 7-day staleness window expired.
const CACHE_SCHEMA_VERSION: u32 = 3;

// ── CacheMetadata
// ─────────────────────────────────────────────────────────────

/// Sidecar metadata written alongside the catalog cache file.
///
/// Used to check whether the cached catalog is stale without reading the full
/// cache file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    /// Unix timestamp (seconds since epoch) when the cache was written.
    pub saved_at_secs:                   u64,
    /// Number of items in the cache at write time.
    pub item_count:                      usize,
    /// Schema version the cache was written with; see [`CACHE_SCHEMA_VERSION`].
    #[serde(default)]
    pub schema_version:                  u32,
    /// Unix timestamp (seconds since epoch) of the last per-item availability
    /// check batch (manual or automatic), gating
    /// `ITEM_CHECK_BATCH_COOLDOWN_SECS`. `#[serde(default)]` so metadata
    /// files written before this field existed deserialize as `None` — the
    /// first post-upgrade batch is never blocked by a cooldown it has no
    /// record of.
    #[serde(default)]
    pub last_item_check_batch_secs:      Option<u64>,
    /// Unix timestamp (seconds since epoch) of the last fresh-install
    /// catalog-initialization request (see
    /// `rust-catalog-fresh-install-initialization`), gating
    /// `CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS`. `#[serde(default)]`
    /// for the same reason as `last_item_check_batch_secs`.
    #[serde(default)]
    pub last_fresh_install_request_secs: Option<u64>,
}

impl CacheMetadata {
    /// Returns true if the cache is older than 7 days or was written by an
    /// older schema version whose data may be missing fields the current
    /// app expects to be populated.
    pub fn is_stale(&self) -> bool {
        if self.schema_version != CACHE_SCHEMA_VERSION {
            return true;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
                                   .unwrap_or(Duration::ZERO)
                                   .as_secs();
        now.saturating_sub(self.saved_at_secs) > STALE_SECS
    }
}

/// Reads the cache metadata from `{root}/catalog_cache_meta.json`.
///
/// Returns `None` on any error (missing file, malformed JSON). Callers that
/// receive `None` should treat the cache as stale.
pub fn load_cache_metadata(root: &Path) -> Option<CacheMetadata> {
    let path = root.join(CATALOG_CACHE_METADATA_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        // A missing file is the expected state on a fresh install (or any time
        // the cache hasn't been written yet) and this is called on every
        // Settings render (`cache_counts`) in addition to catalog-load checks
        // -- warning on it floods the log with a non-error. Any other I/O
        // failure (permissions, a genuinely unreadable disk) is still worth a
        // warning.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "cache metadata not readable");
            return None;
        }
    };
    serde_json::from_str(&text)
        .map_err(|e| warn!(path = %path.display(), error = %e, "cache metadata malformed"))
        .ok()
}

/// Writes a zeroed default `catalog_cache_meta.json` at `root` if no file
/// exists there yet, so the metadata sidecar exists on disk from first
/// startup rather than only appearing after the first successful catalog
/// fetch. Called once at app boot (see `init_globals`).
///
/// `schema_version: 0` and `saved_at_secs: 0` both independently cause
/// [`CacheMetadata::is_stale`] to report `true`, correctly reflecting that
/// nothing has actually been cached yet. A file that already exists — with
/// real data, or one that fails to parse — is left untouched.
pub fn ensure_cache_metadata_exists(root: &Path) {
    let path = root.join(CATALOG_CACHE_METADATA_FILE);
    if path.exists() {
        return;
    }
    let meta = CacheMetadata { saved_at_secs:                   0,
                               item_count:                      0,
                               schema_version:                  0,
                               last_item_check_batch_secs:      None,
                               last_fresh_install_request_secs: None, };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&meta) {
        let _ = fs::write(&path, json);
    }
}

/// Writes cache metadata for `item_count` items to
/// `{root}/catalog_cache_meta.json`.
///
/// Preserves the existing `last_item_check_batch_secs` from the file being
/// overwritten, if any — a catalog cache write is unrelated to the per-item
/// check-batch cooldown, so it must not reset it.
pub fn save_cache_metadata(root: &Path, item_count: usize) -> Result<(), CatalogCacheError> {
    let saved_at_secs = SystemTime::now().duration_since(UNIX_EPOCH)
                                         .unwrap_or(Duration::ZERO)
                                         .as_secs();
    let existing = load_cache_metadata(root);
    let last_item_check_batch_secs = existing.as_ref().and_then(|m| m.last_item_check_batch_secs);
    let last_fresh_install_request_secs = existing.as_ref()
                                                  .and_then(|m| m.last_fresh_install_request_secs);
    let meta = CacheMetadata { saved_at_secs,
                               item_count,
                               schema_version: CACHE_SCHEMA_VERSION,
                               last_item_check_batch_secs,
                               last_fresh_install_request_secs };
    let json = serde_json::to_string(&meta)?;
    fs::write(root.join(CATALOG_CACHE_METADATA_FILE), &json)?;
    Ok(())
}

/// Persists `now_secs` as the last per-item availability check batch
/// timestamp, preserving the rest of the existing cache metadata (or using
/// zeroed placeholders if no metadata file exists yet — a check batch can
/// run before any catalog has ever been successfully synced).
pub fn save_check_batch_timestamp(root: &Path, now_secs: u64) -> Result<(), CatalogCacheError> {
    let mut meta =
        load_cache_metadata(root).unwrap_or(CacheMetadata { saved_at_secs:                   0,
                                   item_count:                      0,
                                   schema_version:                  0,
                                   last_item_check_batch_secs:      None,
                                   last_fresh_install_request_secs: None, });
    meta.last_item_check_batch_secs = Some(now_secs);
    let json = serde_json::to_string(&meta)?;
    fs::write(root.join(CATALOG_CACHE_METADATA_FILE), &json)?;
    Ok(())
}

/// Persists `now_secs` as the last fresh-install catalog-initialization
/// request timestamp, preserving the rest of the existing cache metadata (or
/// using zeroed placeholders if no metadata file exists yet — the
/// fresh-install request happens before any catalog has ever been
/// successfully synced).
pub fn save_fresh_install_request_timestamp(root: &Path, now_secs: u64)
                                            -> Result<(), CatalogCacheError> {
    let mut meta =
        load_cache_metadata(root).unwrap_or(CacheMetadata { saved_at_secs:                   0,
                                   item_count:                      0,
                                   schema_version:                  0,
                                   last_item_check_batch_secs:      None,
                                   last_fresh_install_request_secs: None, });
    meta.last_fresh_install_request_secs = Some(now_secs);
    let json = serde_json::to_string(&meta)?;
    fs::write(root.join(CATALOG_CACHE_METADATA_FILE), &json)?;
    Ok(())
}

// ── CatalogCacheError
// ─────────────────────────────────────────────────────────

/// Errors that can occur when writing the catalog cache.
#[derive(Debug, Error)]
pub enum CatalogCacheError {
    /// An I/O error occurred while reading or writing the cache file.
    #[error("catalog cache I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The catalog data could not be serialized to JSON.
    #[error("catalog cache JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ── load_catalog_cache
// ────────────────────────────────────────────────────────

/// Reads the catalog cache from `{root}/catalog_cache.json`.
///
/// Returns `None` on any error (missing file, malformed JSON) so callers
/// can fall through to the live API fetch without surfacing errors to the user.
pub fn load_catalog_cache(root: &Path) -> Option<Vec<LibraryItem>> {
    let path = root.join(CATALOG_CACHE_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        // A missing file is the expected state on a fresh install; only a
        // genuine I/O failure (permissions, an unreadable disk) is worth a
        // warning.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "catalog cache not readable");
            return None;
        }
    };
    let mut items: Vec<LibraryItem> = serde_json::from_str(&text)
        .map_err(|e| warn!(path = %path.display(), error = %e, "catalog cache malformed"))
        .ok()?;
    // Cache written before file records were deduplicated by id (see
    // `LibraryItem::dedupe_files`) may still have entries repeated verbatim —
    // clean them up on load rather than requiring a full cache wipe.
    for item in &mut items {
        item.dedupe_files();
    }
    items = dedupe_items(items);
    Some(items)
}

/// Collapses whole-item duplicates from `items`, keyed first by `id` (an
/// identical entry repeated verbatim — the symptom of a stale catalog-sync
/// bug that re-appended the same cached page into an already-populated
/// catalog on every load) and secondarily by `product_id` (the catalog SKU,
/// which stays constant across a republish even though `id` doesn't — see
/// the comment on `reconcile_catalog` in `controllers::library`). Items with
/// `product_id == 0` (SKU unknown) are only deduped by `id`. The first
/// occurrence of each key wins.
fn dedupe_items(items: Vec<LibraryItem>) -> Vec<LibraryItem> {
    let mut seen_ids = std::collections::HashSet::new();
    let mut seen_product_ids = std::collections::HashSet::new();
    items.into_iter()
         .filter(|item| {
             if !seen_ids.insert(Arc::clone(&item.id)) {
                 return false;
             }
             item.product_id == 0 || seen_product_ids.insert(item.product_id)
         })
         .collect()
}

// ── save_catalog_cache
// ────────────────────────────────────────────────────────

/// Writes `items` to `{root}/catalog_cache.json` atomically via a `.tmp`
/// rename.
///
/// # Errors
///
/// Returns [`CatalogCacheError::Io`] if the directory cannot be created or the
/// file cannot be written or renamed. Returns [`CatalogCacheError::Json`] if
/// serialization fails.
pub fn save_catalog_cache(root: &Path, items: &[LibraryItem]) -> Result<(), CatalogCacheError> {
    fs::create_dir_all(root)?;
    let tmp = root.join(CATALOG_CACHE_TMP);
    let json = serde_json::to_string(items)?;
    fs::write(&tmp, &json)?;
    fs::rename(&tmp, root.join(CATALOG_CACHE_FILE))?;
    save_cache_metadata(root, items.len())?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::data::enums::ItemStatus;
    use crate::data::library::LibraryItem;

    fn test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dtrpg_cache_test_{name}"))
    }

    fn make_item(id: &str) -> LibraryItem {
        LibraryItem::new(id,
                         "Test Title",
                         "Test Publisher",
                         "",
                         "Core",
                         "PDF",
                         100,
                         10.0,
                         2024,
                         1,
                         ItemStatus::Cloud,
                         "#1C2A44",
                         "Desc.",
                         None)
    }

    #[test]
    fn load_missing_file_returns_none() {
        let dir = std::path::PathBuf::from("/nonexistent/dtrpg_cache_surely_missing");
        assert!(load_catalog_cache(&dir).is_none());
    }

    #[test]
    fn load_malformed_json_returns_none() {
        let dir = test_dir("malformed");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CATALOG_CACHE_FILE), b"not valid json { ").unwrap();
        let result = load_catalog_cache(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert!(result.is_none());
    }

    #[test]
    fn fresh_metadata_is_not_stale() {
        let meta = CacheMetadata { saved_at_secs:
                                       SystemTime::now().duration_since(UNIX_EPOCH)
                                                        .unwrap()
                                                        .as_secs(),
                                   item_count:                      10,
                                   schema_version:                  CACHE_SCHEMA_VERSION,
                                   last_item_check_batch_secs:      None,
                                   last_fresh_install_request_secs: None, };
        assert!(!meta.is_stale());
    }

    #[test]
    fn old_metadata_is_stale() {
        let meta = CacheMetadata { saved_at_secs:                   0, // epoch — very old
                                   item_count:                      10,
                                   schema_version:                  CACHE_SCHEMA_VERSION,
                                   last_item_check_batch_secs:      None,
                                   last_fresh_install_request_secs: None, };
        assert!(meta.is_stale());
    }

    #[test]
    fn fresh_metadata_with_stale_schema_version_is_stale() {
        // Regression: a cache saved before `cover_url` was populated
        // correctly must not be trusted as fresh just because it's recent —
        // it silently disabled thumbnail loading for up to 7 days otherwise.
        let meta = CacheMetadata { saved_at_secs:
                                       SystemTime::now().duration_since(UNIX_EPOCH)
                                                        .unwrap()
                                                        .as_secs(),
                                   item_count:                      10,
                                   schema_version:                  CACHE_SCHEMA_VERSION - 1,
                                   last_item_check_batch_secs:      None,
                                   last_fresh_install_request_secs: None, };
        assert!(meta.is_stale());
    }

    #[test]
    fn metadata_without_schema_version_field_defaults_to_stale() {
        // Regression: metadata JSON written before `schema_version` existed
        // has no such key; `#[serde(default)]` must decode it as `0`, which
        // never matches `CACHE_SCHEMA_VERSION` and is always stale.
        let json = r#"{"saved_at_secs": 9999999999, "item_count": 10}"#;
        let meta: CacheMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.schema_version, 0);
        assert!(meta.is_stale());
    }

    #[test]
    fn missing_metadata_returns_none() {
        let dir = std::path::PathBuf::from("/nonexistent/dtrpg_meta_surely_missing");
        assert!(load_cache_metadata(&dir).is_none());
    }

    #[test]
    fn save_cache_writes_metadata() {
        let dir = test_dir("metadata_roundtrip");
        fs::create_dir_all(&dir).unwrap();
        let items = vec![make_item("b1"), make_item("b2"), make_item("b3")];
        save_catalog_cache(&dir, &items).unwrap();

        let meta = load_cache_metadata(&dir);
        let _ = fs::remove_dir_all(&dir);

        let meta = meta.unwrap();
        assert_eq!(meta.item_count, 3);
        assert!(!meta.is_stale());
    }

    #[test]
    fn ensure_cache_metadata_exists_writes_a_stale_default_when_missing() {
        let dir = test_dir("ensure_metadata_missing");
        let _ = fs::remove_dir_all(&dir);

        ensure_cache_metadata_exists(&dir);
        let meta = load_cache_metadata(&dir);
        let _ = fs::remove_dir_all(&dir);

        let meta = meta.unwrap();
        assert_eq!(meta.item_count, 0);
        assert!(meta.is_stale());
    }

    #[test]
    fn ensure_cache_metadata_exists_leaves_a_real_file_untouched() {
        let dir = test_dir("ensure_metadata_present");
        fs::create_dir_all(&dir).unwrap();
        let items = vec![make_item("b1")];
        save_catalog_cache(&dir, &items).unwrap();

        ensure_cache_metadata_exists(&dir);
        let meta = load_cache_metadata(&dir);
        let _ = fs::remove_dir_all(&dir);

        let meta = meta.unwrap();
        assert_eq!(meta.item_count, 1);
        assert!(!meta.is_stale());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = test_dir("roundtrip");
        fs::create_dir_all(&dir).unwrap();
        let items = vec![make_item("a1"), make_item("a2")];
        save_catalog_cache(&dir, &items).unwrap();

        let loaded = load_catalog_cache(&dir);
        let _ = fs::remove_dir_all(&dir);

        let loaded = loaded.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id.as_ref(), "a1");
        assert_eq!(loaded[1].id.as_ref(), "a2");
    }

    #[test]
    fn load_dedupes_stale_repeated_file_records() {
        // Regression: a cache written before file records were deduplicated
        // by id (see `LibraryItem::dedupe_files`) can have the same file
        // repeated in `files`; loading it must clean this up rather than
        // making the detail tab's item list select every row at once.
        use crate::data::library::LibraryItemFile;

        let dir = test_dir("dedupe_on_load");
        fs::create_dir_all(&dir).unwrap();
        let mut item = make_item("b1");
        item.files = vec![LibraryItemFile { id:         "1234".into(),
                                            index:      0,
                                            name:       "Moria Rulebook".into(),
                                            format:     "PDF".into(),
                                            size_mb:    1.0,
                                            downloaded: false, },
                          LibraryItemFile { id:         "1234".into(),
                                            index:      0,
                                            name:       "Moria Rulebook".into(),
                                            format:     "PDF".into(),
                                            size_mb:    1.0,
                                            downloaded: false, },];
        // Bypass `save_catalog_cache` (which would go through the current,
        // already-deduped write path) to simulate JSON written by an older
        // build that had not yet deduplicated file records.
        fs::write(dir.join(CATALOG_CACHE_FILE),
                  serde_json::to_string(&vec![item]).unwrap()).unwrap();

        let loaded = load_catalog_cache(&dir);
        let _ = fs::remove_dir_all(&dir);

        let loaded = loaded.unwrap();
        assert_eq!(loaded[0].files.len(), 1);
        assert!(!loaded[0].is_multi_item());
    }

    #[test]
    fn load_dedupes_whole_items_repeated_verbatim() {
        // Regression: a catalog-sync bug re-appended the same cached page
        // into an already-populated in-memory catalog on every load, which
        // then got written straight back to disk — ballooning a 2,000-item
        // library into tens of thousands of duplicate entries across
        // repeated launches. Loading an already-bloated cache must collapse
        // it back down rather than requiring a manual cache wipe.
        let dir = test_dir("dedupe_whole_items_on_load");
        fs::create_dir_all(&dir).unwrap();
        let item = make_item("c1");
        fs::write(dir.join(CATALOG_CACHE_FILE),
                  serde_json::to_string(&vec![item.clone(), item.clone(), item]).unwrap()).unwrap();

        let loaded = load_catalog_cache(&dir);
        let _ = fs::remove_dir_all(&dir);

        let loaded = loaded.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id.as_ref(), "c1");
    }

    #[test]
    fn load_dedupes_republished_items_by_product_id() {
        // A republish mints a new `id` but keeps the same `product_id` (the
        // catalog SKU) — see the matching comment on `reconcile_catalog` in
        // `controllers::library`. An already-corrupted cache holding both
        // the stale and the new id for the same product must collapse to
        // one entry, keeping the first occurrence.
        let dir = test_dir("dedupe_by_product_id_on_load");
        fs::create_dir_all(&dir).unwrap();
        let mut old_item = make_item("old-id");
        old_item.product_id = 42;
        let mut new_item = make_item("new-id");
        new_item.product_id = 42;
        fs::write(dir.join(CATALOG_CACHE_FILE),
                  serde_json::to_string(&vec![old_item, new_item]).unwrap()).unwrap();

        let loaded = load_catalog_cache(&dir);
        let _ = fs::remove_dir_all(&dir);

        let loaded = loaded.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id.as_ref(), "old-id");
    }

    #[test]
    fn load_does_not_dedupe_distinct_items_with_unknown_product_id() {
        // `product_id == 0` is a "SKU unknown" sentinel, not a real shared
        // identity — two distinct items that both happen to have it must
        // not collapse into one just because their product ids match.
        let dir = test_dir("no_dedupe_unknown_product_id");
        fs::create_dir_all(&dir).unwrap();
        let item_a = make_item("a1");
        let item_b = make_item("a2");
        assert_eq!(item_a.product_id, 0);
        assert_eq!(item_b.product_id, 0);
        fs::write(dir.join(CATALOG_CACHE_FILE),
                  serde_json::to_string(&vec![item_a, item_b]).unwrap()).unwrap();

        let loaded = load_catalog_cache(&dir);
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(loaded.unwrap().len(), 2);
    }
}
