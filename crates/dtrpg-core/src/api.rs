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

use crate::data::collection::CollectionEntry;
use crate::data::library::LibraryItem;
use crate::services::collections::{CollectionsService, CollectionsServiceError};
use crate::services::{LibraryService, LibraryServiceError};

/// Command/query facade over the library and collections services.
///
/// Owns the concrete service implementations; constructed once per process
/// (per authentication state) and shared by every caller.
pub struct LibraryApi {
    library: Box<dyn LibraryService>,
    collections: Box<dyn CollectionsService>,
}

impl LibraryApi {
    /// Constructs the facade over the given service implementations.
    pub fn new(library: Box<dyn LibraryService>, collections: Box<dyn CollectionsService>) -> Self {
        Self {
            library,
            collections,
        }
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
