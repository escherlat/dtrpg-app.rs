//! Catalog persistence: the library index (`data::index`) is the source of
//! truth for catalog items; this module is the staleness-tracking sidecar
//! that gates the live API refetch, plus the legacy JSON reader used only
//! to migrate an existing install into the index on first launch.
//!
//! Before `libri-library-index`, this file *was* the catalog's on-disk
//! format (a JSON file written and read whole). `load_catalog_cache` and
//! `save_catalog_cache` keep their names and signatures -- every caller in
//! `controllers/library/` goes through them unchanged -- but now delegate
//! to [`crate::data::index::LibraryIndex`] instead of reading or writing
//! `catalog_cache.json` directly.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

use crate::data::constants::{CATALOG_CACHE_FILE, CATALOG_CACHE_METADATA_FILE, STALE_SECS};
use crate::data::index::{IndexError, LibraryIndex};
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
    /// An I/O error occurred while reading or writing the cache metadata
    /// sidecar.
    #[error("catalog cache I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The cache metadata sidecar could not be serialized to JSON.
    #[error("catalog cache JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// The library index could not be opened or written.
    #[error("catalog cache index error: {0}")]
    Index(#[from] IndexError),
}

// ── load_catalog_cache
// ────────────────────────────────────────────────────────

/// Reads the catalog from the library index at `root`, migrating it from
/// `{root}/catalog_cache.json` first if the index doesn't exist yet.
///
/// Returns `None` on any error (including a missing cache and no index) so
/// callers can fall through to the live API fetch without surfacing errors
/// to the user.
pub fn load_catalog_cache(root: &Path) -> Option<Vec<LibraryItem>> {
    let index = LibraryIndex::open_and_migrate(root)
        .map_err(|e| warn!(root = %root.display(), error = %e, "library index not openable"))
        .ok()?;
    index.list_items()
         .map_err(|e| warn!(root = %root.display(), error = %e, "library index query failed"))
         .ok()
         .filter(|items| !items.is_empty())
}

/// Reads the legacy JSON catalog cache directly from
/// `{root}/catalog_cache.json`, without touching the index.
///
/// Used only by [`crate::data::index::LibraryIndex::open_and_migrate`] to
/// seed a newly created index from an existing install's cache; no other
/// caller should read the JSON file directly once the index exists.
///
/// Returns `None` on any error (missing file, malformed JSON).
pub(crate) fn legacy_load_catalog_cache_json(root: &Path) -> Option<Vec<LibraryItem>> {
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

/// Writes `items` to the library index at `root`, replacing the entire
/// catalog, and updates the staleness-tracking metadata sidecar.
///
/// Applies the same defensive id/product-id deduplication the legacy JSON
/// reader used to apply on load (see [`dedupe_items`] and
/// [`LibraryItem::dedupe_files`]) before writing, since the index's `id`
/// primary key rejects a duplicate outright rather than silently collapsing
/// it the way overwriting a JSON file did.
///
/// # Errors
///
/// Returns [`CatalogCacheError::Index`] if the index cannot be opened or
/// written. Returns [`CatalogCacheError::Io`] or
/// [`CatalogCacheError::Json`] if the metadata sidecar cannot be updated.
pub fn save_catalog_cache(root: &Path, items: &[LibraryItem]) -> Result<(), CatalogCacheError> {
    let mut deduped: Vec<LibraryItem> = items.to_vec();
    for item in &mut deduped {
        item.dedupe_files();
    }
    let deduped = dedupe_items(deduped);

    let mut index = LibraryIndex::open_and_migrate(root)?;
    index.replace_all_items(&deduped)?;
    save_cache_metadata(root, deduped.len())?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "catalog_cache_tests.rs"]
mod tests;
