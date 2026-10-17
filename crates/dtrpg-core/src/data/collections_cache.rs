//! Collections persistence: the library index (`data::index`) is the
//! source of truth for collections; this module is now only the legacy
//! JSON reader used to migrate an existing install into the index on first
//! launch. `load_collections_cache` and `save_collections_cache` keep their
//! names and signatures -- every caller in `controllers/library/` goes
//! through them unchanged -- but delegate to
//! [`crate::data::index::LibraryIndex`] instead of reading or writing
//! `collections_cache.json` directly.

use std::fs;
use std::path::Path;

use thiserror::Error;
use tracing::warn;

use crate::data::collection::CollectionEntry;
use crate::data::constants::COLLECTIONS_CACHE_FILE;
use crate::data::index::{IndexError, LibraryIndex};

// ── CollectionsCacheError
// ─────────────────────────────────────────────────────

/// Errors that can occur when writing the collections cache.
#[derive(Debug, Error)]
pub enum CollectionsCacheError {
    /// An I/O error occurred while reading or writing the cache file.
    #[error("collections cache I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The collection data could not be serialized to JSON.
    #[error("collections cache JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// The library index could not be opened or written.
    #[error("collections cache index error: {0}")]
    Index(#[from] IndexError),
}

// ── load_collections_cache
// ────────────────────────────────────────────────────

/// Reads the collection list from the library index at `root`, migrating it
/// from `{root}/collections_cache.json` first if the index doesn't exist
/// yet.
///
/// Returns `None` on any error (including nothing cached and no index) so
/// callers can fall through to the live API fetch without surfacing errors
/// to the user.
pub fn load_collections_cache(root: &Path) -> Option<Vec<CollectionEntry>> {
    let index = LibraryIndex::open_and_migrate(root)
        .map_err(|e| warn!(root = %root.display(), error = %e, "library index not openable"))
        .ok()?;
    index
        .list_collections()
        .map_err(|e| warn!(root = %root.display(), error = %e, "library index query failed"))
        .ok()
        .filter(|entries| !entries.is_empty())
}

/// Reads the legacy JSON collections cache directly from
/// `{root}/collections_cache.json`, without touching the index.
///
/// Used only by [`crate::data::index::LibraryIndex::open_and_migrate`] to
/// seed a newly created index from an existing install's cache; no other
/// caller should read the JSON file directly once the index exists.
///
/// Returns `None` on any error (missing file, malformed JSON).
pub(crate) fn legacy_load_collections_cache_json(root: &Path) -> Option<Vec<CollectionEntry>> {
    let path = root.join(COLLECTIONS_CACHE_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        // A missing file is the expected state on a fresh install; only a
        // genuine I/O failure (permissions, an unreadable disk) is worth a
        // warning.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            warn!(path = %path.display(), error = %e, "collections cache not readable");
            return None;
        }
    };
    serde_json::from_str(&text)
        .map_err(|e| warn!(path = %path.display(), error = %e, "collections cache malformed"))
        .ok()
}

// ── save_collections_cache
// ────────────────────────────────────────────────────

/// Writes `entries` to the library index at `root`, replacing the entire
/// collection list.
///
/// # Errors
///
/// Returns [`CollectionsCacheError::Index`] if the index cannot be opened or
/// written.
pub fn save_collections_cache(
    root: &Path, entries: &[CollectionEntry],
) -> Result<(), CollectionsCacheError> {
    let mut index = LibraryIndex::open_and_migrate(root)?;
    index.replace_all_collections(entries)?;
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dtrpg_collections_cache_test_{name}"))
    }

    fn make_entry(id: u64, name: &str) -> CollectionEntry {
        CollectionEntry {
            id,
            name: Arc::from(name),
            member_ids: Arc::from(vec![1u64, 2u64].as_slice()),
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn load_missing_file_returns_none() {
        let dir = std::path::PathBuf::from("/nonexistent/dtrpg_collections_cache_surely_missing");
        assert!(load_collections_cache(&dir).is_none());
    }

    #[test]
    fn load_malformed_json_returns_none() {
        let dir = test_dir("malformed");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(COLLECTIONS_CACHE_FILE), b"not valid json { ").unwrap();
        let result = load_collections_cache(&dir);
        let _ = fs::remove_dir_all(&dir);
        assert!(result.is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = test_dir("roundtrip");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let entries = vec![make_entry(1, "Favorites"), make_entry(2, "Wishlist")];
        save_collections_cache(&dir, &entries).unwrap();

        let loaded = load_collections_cache(&dir);
        let _ = fs::remove_dir_all(&dir);

        let loaded = loaded.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, 1);
        assert_eq!(loaded[0].name.as_ref(), "Favorites");
        assert_eq!(loaded[1].id, 2);
        assert_eq!(loaded[1].name.as_ref(), "Wishlist");
        assert_eq!(&*loaded[0].member_ids, &[1u64, 2u64]);
    }
}
