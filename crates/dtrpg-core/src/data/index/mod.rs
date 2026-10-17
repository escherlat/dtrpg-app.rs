//! SQLite-backed library index: the single source of truth for catalog
//! items, files, collections, tags, placement, and summaries, with an FTS5
//! full-text table over the searchable fields.
//!
//! See `docs/adr/0003-sqlite-library-index.md` for why SQLite and why
//! `rusqlite`, and `libri-library-index` for the behavioral requirements
//! this module implements.

mod collections;
mod items;
mod migrate;
mod schema;

use std::path::Path;

use rusqlite::Connection;
use thiserror::Error;

use crate::data::collection::CollectionEntry;
use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;

/// Errors from opening or querying the library index.
#[derive(Debug, Error)]
pub enum IndexError {
    /// The underlying SQLite operation failed.
    #[error("library index error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

/// The library index: one SQLite connection, opened once per process and
/// held for its lifetime (SQLite itself serializes writers; this type adds
/// no additional locking of its own).
pub struct LibraryIndex {
    conn: Connection,
}

impl LibraryIndex {
    /// Opens (creating if necessary) the index database at
    /// `{root}/library_index.sqlite3`, ensures its schema exists, and --
    /// only if the database file did not already exist -- migrates it from
    /// the JSON catalog/collections caches under `root`, per
    /// `libri-library-index`'s migration requirement.
    ///
    /// Colocated with the JSON caches it supersedes (the same `root` every
    /// other cache in this crate uses, which today is the user's
    /// configurable storage root, not a fixed app-data directory) so moving
    /// or copying that root keeps the index with it, exactly as copying it
    /// today keeps the JSON caches with it.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the database cannot be opened, the
    /// schema cannot be created, or migration fails.
    pub fn open_and_migrate(root: &Path) -> Result<Self, IndexError> {
        let _ = std::fs::create_dir_all(root);
        let db_path = root.join("library_index.sqlite3");
        let already_existed = db_path.exists();

        let mut conn = Connection::open(&db_path)?;
        schema::ensure_schema(&conn)?;

        if !already_existed {
            migrate::migrate_json_caches(&mut conn, root)?;
        }

        Ok(Self { conn })
    }

    /// Opens an in-memory index with no migration, for tests.
    #[cfg(test)]
    pub(crate) fn open_in_memory() -> Result<Self, IndexError> {
        let conn = Connection::open_in_memory()?;
        schema::ensure_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Replaces the entire item catalog with `items`. See
    /// [`items::replace_all_items`].
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the write fails.
    pub fn replace_all_items(&mut self, items: &[LibraryItem]) -> Result<(), IndexError> {
        items::replace_all_items(&mut self.conn, items)
    }

    /// Returns every item in the catalog.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn list_items(&self) -> Result<Vec<LibraryItem>, IndexError> {
        items::list_items(&self.conn)
    }

    /// Returns the item identified by `id`, or `None` if no such item exists.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn get_item(&self, id: &str) -> Result<Option<LibraryItem>, IndexError> {
        items::get_item(&self.conn, id)
    }

    /// Full-text searches title, publisher, description, and file names for
    /// `query`.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn search(&self, query: &str) -> Result<Vec<LibraryItem>, IndexError> {
        items::search(&self.conn, query)
    }

    /// Counts every item in the catalog.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn count_all(&self) -> Result<usize, IndexError> {
        items::count_all(&self.conn)
    }

    /// Counts items whose `status` matches `status`.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn count_by_status(&self, status: ItemStatus) -> Result<usize, IndexError> {
        items::count_by_status(&self.conn, status)
    }

    /// Counts items whose most recent timestamp falls within `window_days`
    /// of `now_secs`. See
    /// [`crate::util::matching::item_recently_updated`] for the equivalent
    /// per-item predicate.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn count_recently_updated(&self, now_secs: i64, window_days: u32)
                                  -> Result<usize, IndexError> {
        items::count_recently_updated(&self.conn, now_secs, window_days)
    }

    /// Replaces the entire collection list with `entries`. See
    /// [`collections::replace_all_collections`].
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the write fails.
    pub fn replace_all_collections(&mut self, entries: &[CollectionEntry])
                                   -> Result<(), IndexError> {
        collections::replace_all_collections(&mut self.conn, entries)
    }

    /// Returns every collection.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn list_collections(&self) -> Result<Vec<CollectionEntry>, IndexError> {
        collections::list_collections(&self.conn)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::data::library::LibraryItemFile;

    fn item(id: &str, title: &str, publisher: &str, desc: &str) -> LibraryItem {
        LibraryItem::new(id,
                         title,
                         publisher,
                         "",
                         "Core",
                         "PDF",
                         100,
                         10.0,
                         2024,
                         0,
                         ItemStatus::Cloud,
                         "#1C2A44",
                         desc,
                         Some(1_700_000_000))
    }

    #[test]
    fn replace_then_list_round_trips() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        let items = vec![item("a1", "Moria", "Free League", "A classic."),
                         item("a2", "Vaesen", "Free League", "Nordic horror."),];

        index.replace_all_items(&items).unwrap();
        let loaded = index.list_items().unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id.as_ref(), "a1");
        assert_eq!(loaded[1].id.as_ref(), "a2");
    }

    #[test]
    fn replace_all_items_overwrites_the_previous_catalog() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        index.replace_all_items(&[item("a1", "Moria", "Free League", "")])
             .unwrap();

        index.replace_all_items(&[item("b1", "Vaesen", "Free League", "")])
             .unwrap();
        let loaded = index.list_items().unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id.as_ref(), "b1");
    }

    #[test]
    fn get_item_finds_an_existing_item_and_its_files() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        let mut moria = item("a1", "Moria", "Free League", "");
        moria.files = vec![LibraryItemFile { id:         Arc::from("f1"),
                                             index:      0,
                                             name:       Arc::from("Moria.pdf"),
                                             format:     Arc::from("PDF"),
                                             size_mb:    12.0,
                                             downloaded: true,
                                             offloaded: false, }];
        index.replace_all_items(&[moria]).unwrap();

        let loaded = index.get_item("a1").unwrap().unwrap();

        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].name.as_ref(), "Moria.pdf");
        assert!(loaded.files[0].downloaded);
    }

    #[test]
    fn get_item_returns_none_for_an_unknown_id() {
        let index = LibraryIndex::open_in_memory().unwrap();
        assert!(index.get_item("nope").unwrap().is_none());
    }

    #[test]
    fn search_matches_title() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        index.replace_all_items(&[item("a1", "Moria", "Free League", ""),
                                  item("a2", "Vaesen", "Free League", "")])
             .unwrap();

        let results = index.search("Moria").unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_ref(), "a1");
    }

    #[test]
    fn search_matches_description() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        index.replace_all_items(&[item("a1", "Moria", "Free League", "A classic dungeon crawl."),
                                  item("a2", "Vaesen", "Free League", "Nordic horror.")])
             .unwrap();

        let results = index.search("dungeon").unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id.as_ref(), "a1");
    }

    #[test]
    fn search_finds_nothing_for_an_unmatched_term() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        index.replace_all_items(&[item("a1", "Moria", "Free League", "")])
             .unwrap();

        assert!(index.search("nonexistentterm").unwrap().is_empty());
    }

    #[test]
    fn count_all_reflects_the_full_catalog() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        index.replace_all_items(&[item("a1", "Moria", "Free League", ""),
                                  item("a2", "Vaesen", "Free League", "")])
             .unwrap();

        assert_eq!(index.count_all().unwrap(), 2);
    }

    #[test]
    fn count_by_status_matches_the_filtered_subset() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        let mut downloaded = item("a1", "Moria", "Free League", "");
        downloaded.status = ItemStatus::Downloaded;
        let cloud = item("a2", "Vaesen", "Free League", "");
        index.replace_all_items(&[downloaded, cloud]).unwrap();

        assert_eq!(index.count_by_status(ItemStatus::Downloaded).unwrap(), 1);
        assert_eq!(index.count_by_status(ItemStatus::Cloud).unwrap(), 1);
    }

    #[test]
    fn count_recently_updated_respects_the_window() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        let now = 1_800_000_000;
        let mut recent = item("a1", "Moria", "Free League", "");
        recent.date_updated = Some(now - 10 * 86_400);
        let mut old = item("a2", "Vaesen", "Free League", "");
        old.date_updated = Some(now - 40 * 86_400);
        index.replace_all_items(&[recent, old]).unwrap();

        assert_eq!(index.count_recently_updated(now, 30).unwrap(), 1);
        assert_eq!(index.count_recently_updated(now, 7).unwrap(), 0);
    }

    #[test]
    fn collections_round_trip_with_members() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        let entries = vec![CollectionEntry { id:         1,
                                             name:       Arc::from("Favorites"),
                                             member_ids: Arc::from([10u64, 20u64]),
                                             created_at: 1_700_000_000, }];

        index.replace_all_collections(&entries).unwrap();
        let loaded = index.list_collections().unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name.as_ref(), "Favorites");
        assert_eq!(&*loaded[0].member_ids, &[10u64, 20u64]);
    }
}
