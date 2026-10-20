//! SQLite-backed library index: the single source of truth for catalog
//! items, files, collections, tags, placement, and summaries, with an FTS5
//! full-text table over the searchable fields.
//!
//! See `docs/adr/0003-sqlite-library-index.md` for why SQLite and why
//! `rusqlite`, and `libri-library-index` for the behavioral requirements
//! this module implements.

mod collections;
mod facets;
mod filtered_items;
mod functions;
mod items;
mod migrate;
mod schema;
mod summaries;
mod tag_methods;
mod tag_sync;
mod tags;

#[cfg(test)]
// Index fixtures use unwrap for known-good in-memory database operations.
#[allow(clippy::unwrap_used)]
#[path = "summary_tests.rs"]
mod summary_tests;

use std::path::Path;
use std::sync::Arc;

pub use filtered_items::ItemFilter;
use rusqlite::Connection;
use thiserror::Error;

use crate::data::collection::CollectionEntry;
use crate::data::enums::ItemStatus;
use crate::data::library::{LibraryItem, SectionCounts};
use crate::data::summary::ContentSummary;
use crate::util::sort::{SortDirection, SortMethod};

/// Errors from opening or querying the library index.
#[derive(Debug, Error)]
pub enum IndexError {
    /// The underlying SQLite operation failed.
    #[error("library index error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A tag could not be stored as given.
    #[error("invalid tag {tag:?}: {reason}")]
    InvalidTag {
        /// The tag as it was passed in.
        tag:    String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// The item to tag is not in the catalog.
    #[error("no item with id {0:?} in the library index")]
    NoSuchItem(String),
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

        let mut index = Self::initialized(Connection::open(&db_path)?)?;

        if !already_existed {
            migrate::migrate_json_caches(&mut index.conn, root)?;
        }

        Ok(index)
    }

    /// Opens an in-memory index with no migration, for tests.
    #[cfg(test)]
    pub(crate) fn open_in_memory() -> Result<Self, IndexError> {
        Self::initialized(Connection::open_in_memory()?)
    }

    /// Single initialization sequence every `LibraryIndex` connection goes
    /// through: custom SQL functions first (so a future schema object that
    /// references `ulower` can be created), then the schema. One path, not
    /// two, so no future open site can half-initialize a connection and let
    /// an `ulower` query fail into `unwrap_or_default()`'s silent empty
    /// catalog.
    fn initialized(conn: Connection) -> Result<Self, IndexError> {
        functions::register_functions(&conn)?;
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

    /// Replaces one already-cataloged item's row, files, and FTS5 entry with
    /// `item`'s current state (status, availability, file flags, …).
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the write fails.
    pub fn replace_item(&mut self, item: &LibraryItem) -> Result<(), IndexError> {
        items::replace_item(&mut self.conn, item)
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

    /// Each publisher with its item count, ordered by lowercased name.
    ///
    /// The index-backed equivalent of `dtrpg_ui::util::publisher::
    /// publisher_entries` over a full-catalog fetch — see issue #355.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn publisher_counts(&self) -> Result<Vec<(Arc<str>, usize)>, IndexError> {
        facets::publisher_counts(&self.conn)
    }

    /// The sidebar section counts for the whole catalog, composing the
    /// facet queries — the index-backed equivalent of
    /// [`crate::data::library::section_counts`] over a full-catalog fetch.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if any constituent query fails.
    pub fn section_counts(&self, now_secs: i64, window_days: u32)
                          -> Result<SectionCounts, IndexError> {
        facets::section_counts(&self.conn, now_secs, window_days)
    }

    /// Returns every item matching `filter`, sorted by `sort`/`direction`.
    ///
    /// Replaces `LibraryController::invalidate_cache()`'s in-memory `Vec`
    /// scan — see `openspec/changes/catalog-reads-from-index`.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn list_items_filtered(&self, filter: &ItemFilter<'_>, sort: SortMethod,
                               direction: SortDirection)
                               -> Result<Vec<LibraryItem>, IndexError> {
        filtered_items::list_items_filtered(&self.conn, filter, sort, direction)
    }

    /// Counts the items matching `filter` without materializing them. Pass
    /// `search: None` for the toolbar's "section size ignoring search"
    /// number (`openspec/changes/search-box-semantics-sql`).
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn count_items_filtered(&self, filter: &ItemFilter<'_>) -> Result<usize, IndexError> {
        filtered_items::count_items_filtered(&self.conn, filter)
    }

    /// Returns the items matching `filter` ordered for publisher grouping.
    ///
    /// Rows are sorted by Unicode-lowercase publisher ascending, then by the
    /// user's chosen `sort`/`direction` within each publisher. The caller can
    /// split the result into `ItemGroup`s with a single linear pass.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn list_items_for_grouping_by_publisher(&self, filter: &ItemFilter<'_>,
                                                sort: SortMethod, direction: SortDirection)
                                                -> Result<Vec<LibraryItem>, IndexError> {
        filtered_items::list_items_for_grouping_by_publisher(&self.conn, filter, sort, direction)
    }

    /// Returns the items matching `filter` expanded to one row per tag,
    /// ordered for tag grouping.
    ///
    /// Each tuple is `(group_name, item)`; `group_name` is `None` only for the
    /// untagged rows, which are appended at the end. Rows are sorted by
    /// Unicode-lowercase tag ascending, then by the user's chosen
    /// `sort`/`direction` within each tag.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if either query fails.
    pub fn list_items_for_grouping_by_tag(
        &self, filter: &ItemFilter<'_>, sort: SortMethod, direction: SortDirection)
        -> Result<Vec<(Option<String>, LibraryItem)>, IndexError> {
        filtered_items::list_items_for_grouping_by_tag(&self.conn, filter, sort, direction)
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

    /// Stores or replaces an item's content summary and refreshes its FTS row.
    pub fn save_summary(&mut self, summary: &ContentSummary) -> Result<(), IndexError> {
        summaries::save(&mut self.conn, summary)?;
        Ok(())
    }

    /// Returns an item's persisted content summary, if one exists.
    pub fn get_summary(&self, item_id: &str) -> Result<Option<ContentSummary>, IndexError> {
        Ok(summaries::get(&self.conn, item_id)?)
    }

    /// Sums `token_count` across every summary created at or after
    /// `since_unix_secs`, for enforcing a rolling monthly token ceiling. See
    /// [`summaries::tokens_used_since`].
    pub fn tokens_used_since(&self, since_unix_secs: i64) -> Result<u64, IndexError> {
        Ok(summaries::tokens_used_since(&self.conn, since_unix_secs)?)
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
                                             offloaded:  false, }];
        index.replace_all_items(&[moria]).unwrap();

        let loaded = index.get_item("a1").unwrap().unwrap();

        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].name.as_ref(), "Moria.pdf");
        assert!(loaded.files[0].downloaded);
        assert!(!loaded.files[0].offloaded);
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
