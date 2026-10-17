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
use crate::services::collections::{CollectionsService, CollectionsServiceError};
use crate::services::{LibraryService, LibraryServiceError};

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
    library: Box<dyn LibraryService>,
    collections: Box<dyn CollectionsService>,
    /// Directory the library index database lives in -- the same directory
    /// `data::catalog_cache`/`data::collections_cache` already use (the
    /// app's own cache directory, not a user-configurable path).
    index_root: PathBuf,
}

impl LibraryApi {
    /// Constructs the facade over the given service implementations.
    ///
    /// `index_root` is the directory the library index database lives in.
    pub fn new(
        library: Box<dyn LibraryService>, collections: Box<dyn CollectionsService>,
        index_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            library,
            collections,
            index_root: index_root.into(),
        }
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
    pub fn index_count_recently_updated(
        &self, now_secs: i64, window_days: u32,
    ) -> Result<usize, IndexError> {
        self.open_index()?
            .count_recently_updated(now_secs, window_days)
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
    pub fn create_collection(
        &self, name: &str,
    ) -> Result<CollectionEntry, CollectionsServiceError> {
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
    use crate::util::stubs::stub_catalog;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dtrpg_library_api_test_{name}"))
    }

    fn api_at(dir: &Path) -> LibraryApi {
        LibraryApi::new(
            Box::new(StubLibraryService::new(StubMode::Seeded)),
            Box::new(CollectionsStubService::new(CollectionsStubMode::Empty)),
            dir,
        )
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
        let downloaded = catalog
            .iter()
            .filter(|i| i.status == ItemStatus::Downloaded)
            .count();
        api.open_index()
            .unwrap()
            .replace_all_items(&catalog)
            .unwrap();

        assert_eq!(
            api.index_count_by_status(ItemStatus::Downloaded).unwrap(),
            downloaded
        );
        assert_eq!(api.index_count_all().unwrap(), catalog.len());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
