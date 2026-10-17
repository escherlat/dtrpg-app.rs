//! One-way migration from the JSON catalog/collections caches into the
//! library index, run on first launch after the index is introduced.
//!
//! Per `libri-library-index`'s "existing caches migrate on first launch"
//! requirement: the JSON files are read, not deleted -- they remain on disk
//! as a downgrade path and a rebuild source, even though nothing reads them
//! once the index exists.

use std::path::Path;

use rusqlite::Connection;

use crate::data::catalog_cache::legacy_load_catalog_cache_json;
use crate::data::collections_cache::legacy_load_collections_cache_json;
use crate::data::index::{IndexError, collections, items};

/// Populates `conn` from the JSON caches under `root`, if present.
///
/// A no-op (not an error) if neither cache file exists -- the fresh-install
/// case, where the index starts empty and the live API fetch populates it
/// the normal way.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if writing the migrated data fails.
pub(super) fn migrate_json_caches(conn: &mut Connection, root: &Path) -> Result<(), IndexError> {
    if let Some(catalog) = legacy_load_catalog_cache_json(root) {
        items::replace_all_items(conn, &catalog)?;
    }
    if let Some(collection_entries) = legacy_load_collections_cache_json(root) {
        collections::replace_all_collections(conn, &collection_entries)?;
    }
    Ok(())
}
