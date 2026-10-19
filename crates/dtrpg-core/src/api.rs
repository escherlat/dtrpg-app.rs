//! `LibraryApi`: the only `pub` entry point into the library for callers
//! outside this crate. See
//! `docs/adr/0002-invert-crate-graph-and-add-library-facade.md`.
//!
//! Both the desktop UI (`dtrpg-desktop`) and the MCP server (`dtrpg-mcp`) go
//! through this facade rather than reaching into `services`, `data`, or
//! `credentials` directly, which is what keeps the two from drifting apart.
//! This is an initial, minimal surface -- it grows as the goals that need it
//! (search, collections management, placement, summaries) are built, not
//! ahead of them.

use std::path::{Path, PathBuf};

use crate::data::collection::CollectionEntry;
use crate::data::enums::ItemStatus;
use crate::data::index::{IndexError, LibraryIndex};
use crate::data::library::LibraryItem;
use crate::data::summary::ContentSummary;
use crate::data::tag_color::TagColor;
use crate::data::tag_library::TagLibrary;
use crate::services::collections::{CollectionsService, CollectionsServiceError};
use crate::services::{LibraryService, LibraryServiceError};
use crate::util::datetime::now_secs;

#[path = "api_archive.rs"]
mod api_archive;
#[path = "api_summary.rs"]
mod api_summary;

pub use api_summary::{SummarizeError, provider_for};

/// Command/query facade over the library and collections services, and over
/// the local library index (search, counts, and reads that don't need a
/// network round trip).
///
/// Owns the concrete service implementations; constructed once per process
/// (per authentication state) and shared by every caller. The index itself
/// is opened per call, matching `data::catalog_cache`'s existing
/// stateless-per-call pattern -- see `index_root`'s doc comment for why this
/// facade does not hold a long-lived `LibraryIndex` connection.
pub struct LibraryApi {
    library:     Box<dyn LibraryService>,
    collections: Box<dyn CollectionsService>,
    /// Directory the library index database lives in -- the same directory
    /// `data::catalog_cache`/`data::collections_cache` already use (the
    /// app's own cache directory, not a user-configurable path).
    index_root:  PathBuf,
}

impl LibraryApi {
    /// Constructs the facade over the given service implementations.
    ///
    /// `index_root` is the directory the library index database lives in.
    pub fn new(library: Box<dyn LibraryService>, collections: Box<dyn CollectionsService>,
               index_root: impl Into<PathBuf>)
               -> Self {
        Self { library,
               collections,
               index_root: index_root.into() }
    }

    fn open_index(&self) -> Result<LibraryIndex, IndexError> {
        LibraryIndex::open_and_migrate(&self.index_root)
    }

    /// Returns the directory the library index database lives in.
    #[must_use]
    pub fn index_root(&self) -> &Path {
        &self.index_root
    }

    /// Returns every item in the local library index.
    ///
    /// Unlike [`Self::list_items`] (which calls the SDK service and may
    /// hit the network), this reads the local index only.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or queried.
    pub fn index_list_items(&self) -> Result<Vec<LibraryItem>, IndexError> {
        self.open_index()?.list_items()
    }

    /// Returns the item identified by `id` (the stable catalog id, e.g.
    /// `"b1"` -- not the numeric API id [`Self::get_item`] takes), or
    /// `None` if no such item exists in the local index.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or queried.
    pub fn index_get_item(&self, id: &str) -> Result<Option<LibraryItem>, IndexError> {
        self.open_index()?.get_item(id)
    }

    /// Full-text searches the local index for `query` (title, publisher,
    /// description, and file names).
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or queried
    /// (including a malformed FTS5 query string).
    pub fn search(&self, query: &str) -> Result<Vec<LibraryItem>, IndexError> {
        self.open_index()?.search(query)
    }

    /// Counts every item in the local index.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or queried.
    pub fn index_count_all(&self) -> Result<usize, IndexError> {
        self.open_index()?.count_all()
    }

    /// Counts items in the local index whose status matches `status`.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or queried.
    pub fn index_count_by_status(&self, status: ItemStatus) -> Result<usize, IndexError> {
        self.open_index()?.count_by_status(status)
    }

    /// Counts items in the local index whose most recent timestamp falls
    /// within `window_days` of `now_secs`.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or queried.
    pub fn index_count_recently_updated(&self, now_secs: i64, window_days: u32)
                                        -> Result<usize, IndexError> {
        self.open_index()?
            .count_recently_updated(now_secs, window_days)
    }

    /// Returns the persisted summary for a stable catalog item id.
    pub fn index_get_summary(&self, item_id: &str) -> Result<Option<ContentSummary>, IndexError> {
        self.open_index()?.get_summary(item_id)
    }

    /// Stores a generated summary for a stable catalog item id.
    pub fn index_save_summary(&self, summary: &ContentSummary) -> Result<(), IndexError> {
        self.open_index()?.save_summary(summary)
    }

    /// Returns every tag that some item in the local index has, ordered
    /// ignoring case.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or queried.
    pub fn index_list_tags(&self) -> Result<Vec<String>, IndexError> {
        self.open_index()?.list_tags()
    }

    /// Returns the tags of the item `item_id` (the stable catalog id), ordered
    /// ignoring case.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or queried.
    pub fn index_item_tags(&self, item_id: &str) -> Result<Vec<String>, IndexError> {
        self.open_index()?.tags_for_item(item_id)
    }

    /// Returns the tags of every item that has any, keyed by the stable catalog
    /// id.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or queried.
    pub fn index_all_item_tags(
        &self)
        -> Result<std::collections::HashMap<String, Vec<String>>, IndexError> {
        self.open_index()?.all_item_tags()
    }

    /// Returns every tag with its color and how many items have it, and which
    /// items have which.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or queried.
    pub fn index_tag_library(&self) -> Result<TagLibrary, IndexError> {
        TagLibrary::load(&self.open_index()?)
    }

    /// Sets the color of the tag `tag`, or clears it with `None`; returns
    /// `false` if no item has such a tag.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or written.
    pub fn index_set_tag_color(&self, tag: &str, color: Option<TagColor>)
                               -> Result<bool, IndexError> {
        self.open_index()?.set_tag_color(tag, color)
    }

    /// Makes `tags` exactly the tags of the item `item_id`. A tag the index
    /// already has keeps its spelling and its color; a new one has no color.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::InvalidTag`] if a tag cannot be stored,
    /// [`IndexError::NoSuchItem`] if the item is not in the library index, and
    /// [`IndexError`] if the index cannot be opened or written. Nothing is
    /// changed on error.
    pub fn index_set_item_tags(&self, item_id: &str, tags: &[String]) -> Result<(), IndexError> {
        let wanted: Vec<(String, Option<TagColor>)> =
            tags.iter().map(|tag| (tag.clone(), None)).collect();
        self.open_index()?
            .set_item_tags(item_id, &wanted, now_secs())
    }

    /// Tags the item `item_id` with `tag`; returns `false` if it already had
    /// the tag. Tags are free text, unique ignoring case: adding "Solo" when
    /// "solo" exists uses the existing spelling.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::InvalidTag`] if `tag` is empty, longer than 64
    /// characters, or contains a comma or control character,
    /// [`IndexError::NoSuchItem`] if the item is not in the index, and another
    /// [`IndexError`] if the index cannot be opened or written.
    pub fn index_add_tag(&self, item_id: &str, tag: &str) -> Result<bool, IndexError> {
        self.open_index()?.add_tag(item_id, tag)
    }

    /// Removes `tag` from the item `item_id`; returns `false` if it did not
    /// have the tag.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or written.
    pub fn index_remove_tag(&self, item_id: &str, tag: &str) -> Result<bool, IndexError> {
        self.open_index()?.remove_tag(item_id, tag)
    }

    /// Renames the tag `from` to `to` on every item and returns how many items
    /// that was. If `to` is already a tag, the two are merged.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::InvalidTag`] if `to` cannot be stored, and another
    /// [`IndexError`] if the index cannot be opened or written.
    pub fn index_rename_tag(&self, from: &str, to: &str) -> Result<usize, IndexError> {
        self.open_index()?.rename_tag(from, to)
    }

    /// Removes the tag `tag` from every item and returns how many items had it.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or written.
    pub fn index_delete_tag(&self, tag: &str) -> Result<usize, IndexError> {
        self.open_index()?.delete_tag(tag)
    }

    /// Lists every item in the user's library.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryServiceError`] if the underlying service call
    /// fails.
    pub fn list_items(&self) -> Result<Vec<LibraryItem>, LibraryServiceError> {
        self.library.list_items()
    }

    /// Loads a single item by its numeric API identifier.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryServiceError`] if the underlying service call
    /// fails or the id does not match any item.
    pub fn get_item(&self, id: u64) -> Result<LibraryItem, LibraryServiceError> {
        self.library.get_item(id)
    }

    /// Returns the total item count, if the underlying service supports a
    /// cheap count.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryServiceError`] if the underlying service call
    /// fails.
    pub fn count_items(&self) -> Option<Result<usize, LibraryServiceError>> {
        self.library.count_items()
    }

    /// Lists every collection (named product list) the user owns.
    ///
    /// # Errors
    ///
    /// Returns a [`CollectionsServiceError`] if the underlying service call
    /// fails.
    pub fn list_collections(&self) -> Result<Vec<CollectionEntry>, CollectionsServiceError> {
        self.collections.list_collections()
    }

    /// Creates a new collection with the given name.
    ///
    /// # Errors
    ///
    /// Returns a [`CollectionsServiceError`] if the underlying service call
    /// fails.
    pub fn create_collection(&self, name: &str)
                             -> Result<CollectionEntry, CollectionsServiceError> {
        self.collections.create_collection(name)
    }

    /// Deletes the collection with the given id.
    ///
    /// # Errors
    ///
    /// Returns a [`CollectionsServiceError`] if the underlying service call
    /// fails.
    pub fn delete_collection(&self, id: u64) -> Result<(), CollectionsServiceError> {
        self.collections.delete_collection(id)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::data::enums::ItemStatus;
    use crate::services::collections::stub::{CollectionsStubMode, CollectionsStubService};
    use crate::services::stub::{StubLibraryService, StubMode};
    use crate::services::stub_catalog::stub_catalog;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dtrpg_library_api_test_{name}"))
    }

    fn api_at(dir: &Path) -> LibraryApi {
        LibraryApi::new(Box::new(StubLibraryService::new(StubMode::Seeded)),
                        Box::new(CollectionsStubService::new(CollectionsStubMode::Empty)),
                        dir)
    }

    #[test]
    fn archive_queries_use_the_shared_container_api() {
        let dir = test_dir("archive");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive_path = dir.join("fixture.tar");
        let file = std::fs::File::create(&archive_path).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_path("manual.txt").unwrap();
        header.set_size(5);
        header.set_cksum();
        builder.append(&header, &b"hello"[..]).unwrap();
        builder.finish().unwrap();

        let api = api_at(&dir);
        let entries = api.list_archive_entries(&archive_path).unwrap();

        assert_eq!(entries[0].name, "manual.txt");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_queries_are_empty_before_anything_is_written() {
        let dir = test_dir("empty");
        let _ = std::fs::remove_dir_all(&dir);
        let api = api_at(&dir);

        assert!(api.index_list_items().unwrap().is_empty());
        assert_eq!(api.index_count_all().unwrap(), 0);
        assert!(api.index_get_item("nope").unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_list_and_search_reflect_a_written_catalog() {
        let dir = test_dir("populated");
        let _ = std::fs::remove_dir_all(&dir);
        let api = api_at(&dir);
        let catalog = stub_catalog();
        api.open_index()
           .unwrap()
           .replace_all_items(&catalog)
           .unwrap();

        let listed = api.index_list_items().unwrap();
        assert_eq!(listed.len(), catalog.len());

        let first_title = catalog[0].title.clone();
        let results = api.search(&first_title).unwrap();
        assert!(results.iter().any(|i| i.id == catalog[0].id));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_count_by_status_matches_the_written_catalog() {
        let dir = test_dir("counts");
        let _ = std::fs::remove_dir_all(&dir);
        let api = api_at(&dir);
        let catalog = stub_catalog();
        let downloaded = catalog.iter()
                                .filter(|i| i.status == ItemStatus::Downloaded)
                                .count();
        api.open_index()
           .unwrap()
           .replace_all_items(&catalog)
           .unwrap();

        assert_eq!(api.index_count_by_status(ItemStatus::Downloaded).unwrap(),
                   downloaded);
        assert_eq!(api.index_count_all().unwrap(), catalog.len());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tags_are_stored_in_the_index_and_found_by_search() {
        let dir = test_dir("tags");
        let _ = std::fs::remove_dir_all(&dir);
        let api = api_at(&dir);
        let catalog = stub_catalog();
        api.open_index()
           .unwrap()
           .replace_all_items(&catalog)
           .unwrap();
        let id = catalog[0].id.to_string();

        assert!(api.index_add_tag(&id, "solo").unwrap());
        assert!(!api.index_add_tag(&id, "Solo").unwrap());

        assert_eq!(api.index_item_tags(&id).unwrap(), ["solo"]);
        assert_eq!(api.index_list_tags().unwrap(), ["solo"]);
        let found = api.search("solo").unwrap();
        assert!(found.iter().any(|item| item.id == catalog[0].id));

        assert_eq!(api.index_rename_tag("solo", "duet").unwrap(), 1);
        assert_eq!(api.index_delete_tag("duet").unwrap(), 1);
        assert!(api.index_list_tags().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_tag_library_and_tag_colors_are_read_and_set_through_the_api() {
        let dir = test_dir("tag_library");
        let _ = std::fs::remove_dir_all(&dir);
        let api = api_at(&dir);
        let catalog = stub_catalog();
        api.open_index()
           .unwrap()
           .replace_all_items(&catalog)
           .unwrap();
        let id = catalog[0].id.to_string();
        api.index_add_tag(&id, "solo").unwrap();

        assert!(api.index_set_tag_color("Solo", Some(TagColor::Red))
                   .unwrap());
        assert!(!api.index_set_tag_color("missing", Some(TagColor::Red))
                    .unwrap());

        let library = api.index_tag_library().unwrap();
        assert_eq!(library.entry("solo").unwrap().count, 1);
        assert_eq!(library.color_of("solo"), Some(TagColor::Red));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_items_tags_are_replaced_keeping_the_color_of_a_tag_that_stays() {
        let dir = test_dir("set_item_tags");
        let _ = std::fs::remove_dir_all(&dir);
        let api = api_at(&dir);
        let catalog = stub_catalog();
        api.open_index()
           .unwrap()
           .replace_all_items(&catalog)
           .unwrap();
        let id = catalog[0].id.to_string();
        api.index_add_tag(&id, "solo").unwrap();
        api.index_add_tag(&id, "old").unwrap();
        api.index_set_tag_color("solo", Some(TagColor::Blue))
           .unwrap();

        api.index_set_item_tags(&id, &["Solo".to_owned(), "new".to_owned()])
           .unwrap();

        assert_eq!(api.index_item_tags(&id).unwrap(), ["new", "solo"]);
        assert_eq!(api.index_tag_library().unwrap().color_of("solo"),
                   Some(TagColor::Blue));
        assert!(api.index_set_item_tags("missing", &[]).is_err());
        assert!(api.index_set_item_tags(&id, &["no, comma".to_owned()])
                   .is_err());
        assert_eq!(api.index_item_tags(&id).unwrap(), ["new", "solo"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
