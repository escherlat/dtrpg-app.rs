//! Library view model for Rust frontend pane state management.

use std::sync::Arc;

use crate::data::library::LibraryItem;
use crate::services::{LibraryService, LibraryServiceError, LibraryServiceErrorKind};

/// Returns `true` when `error` indicates the session has expired.
///
/// # TODO
///
/// Full re-authentication handling (token refresh → login window) is deferred
/// until `connect-sdk-to-rust-app` lands. For now, callers log a warning and
/// show a generic error state.
pub fn is_needs_reauth(error: &LibraryServiceError) -> bool {
    matches!(error.kind, LibraryServiceErrorKind::NeedsReauth)
}

/// High-level pane state for the library view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryPaneState {
    /// List or detail data is being loaded.
    Loading,
    /// List and optional detail content are ready.
    Loaded,
    /// No results are available for the current request.
    Empty,
    /// A recoverable error occurred.
    Error,
}

/// UI-facing library view model that mediates between the service and the
/// controller.
pub struct LibraryViewModel {
    service:    Arc<dyn LibraryService>,
    items:      Vec<LibraryItem>,
    selected:   Option<LibraryItem>,
    pane:       LibraryPaneState,
    last_error: Option<LibraryServiceError>,
}

impl LibraryViewModel {
    /// Creates a new view model from a backend-agnostic service implementation.
    pub fn new(service: Box<dyn LibraryService>) -> Self {
        Self { service:    Arc::from(service),
               items:      Vec::new(),
               selected:   None,
               pane:       LibraryPaneState::Loading,
               last_error: None, }
    }

    /// Returns a cloneable reference to the service for use in background
    /// tasks.
    pub fn service_arc(&self) -> Arc<dyn LibraryService> {
        Arc::clone(&self.service)
    }

    /// Replaces the backing service and resets pane state for a fresh load.
    pub fn replace_service(&mut self, service: Box<dyn LibraryService>) {
        self.service = Arc::from(service);
        self.items.clear();
        self.selected = None;
        self.pane = LibraryPaneState::Loading;
        self.last_error = None;
    }

    /// Returns the current high-level pane state.
    pub fn pane_state(&self) -> &LibraryPaneState {
        &self.pane
    }

    /// Returns the currently loaded list items.
    pub fn items(&self) -> &[LibraryItem] {
        &self.items
    }

    /// Returns the selected detail item if one is loaded.
    pub fn selected(&self) -> Option<&LibraryItem> {
        self.selected.as_ref()
    }

    /// Returns the most recent recoverable error.
    pub fn last_error(&self) -> Option<&LibraryServiceError> {
        self.last_error.as_ref()
    }

    /// Applies a pre-fetched list result, updating pane state accordingly.
    ///
    /// Called either from [`load_list`] (synchronous path) or directly from a
    /// background task that fetched items off the main thread.
    pub fn apply_list_result(&mut self, result: Result<Vec<LibraryItem>, LibraryServiceError>) {
        match result {
            Ok(items) if items.is_empty() => {
                self.items = Vec::new();
                self.selected = None;
                self.pane = LibraryPaneState::Empty;
            }
            Ok(items) => {
                self.items = items;
                self.selected = None;
                self.pane = LibraryPaneState::Loaded;
            }
            Err(error) => {
                if is_needs_reauth(&error) {
                    // TODO: trigger full re-auth (token refresh → login window)
                    // once connect-sdk-to-rust-app lands.
                    // For now, log and show error state.
                    tracing::warn!("session expired, returning to login");
                }
                self.items = Vec::new();
                self.selected = None;
                self.last_error = Some(error);
                self.pane = LibraryPaneState::Error;
            }
        }
    }

    /// Loads the library list and updates pane state based on the outcome.
    ///
    /// This call blocks the calling thread while the service fetches data.
    /// Prefer spawning this on a background executor when called from the UI
    /// thread.
    pub fn load_list(&mut self) {
        self.pane = LibraryPaneState::Loading;
        self.last_error = None;
        let result = self.service.list_items();
        self.apply_list_result(result);
    }

    /// Reloads list data using the same baseline behavior as the initial load.
    pub fn refresh(&mut self) {
        self.load_list();
    }

    /// Loads detail for the item with the given numeric API id.
    ///
    /// On `NotFound` with items already loaded, pane state remains `Loaded`.
    pub fn select_item(&mut self, id: u64) {
        self.last_error = None;

        match self.service.get_item(id) {
            Ok(item) => {
                self.selected = Some(item);
                if self.pane == LibraryPaneState::Loading {
                    self.pane = LibraryPaneState::Loaded;
                }
            }
            Err(error) => {
                self.selected = None;
                let kind = error.kind;
                self.last_error = Some(error);
                self.pane = match kind {
                    LibraryServiceErrorKind::NotFound if !self.items.is_empty() => {
                        LibraryPaneState::Loaded
                    }
                    _ => LibraryPaneState::Error,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::stub::{StubLibraryService, StubMode};

    #[test]
    fn load_list_enters_loaded_with_seeded_stub_data() {
        let mut vm = LibraryViewModel::new(Box::new(StubLibraryService::new(StubMode::Seeded)));

        vm.load_list();

        assert_eq!(vm.pane_state(), &LibraryPaneState::Loaded);
        assert!(!vm.items().is_empty());
        assert!(vm.last_error().is_none());
    }

    #[test]
    fn load_list_enters_empty_with_empty_stub_mode() {
        let mut vm = LibraryViewModel::new(Box::new(StubLibraryService::new(StubMode::Empty)));

        vm.load_list();

        assert_eq!(vm.pane_state(), &LibraryPaneState::Empty);
        assert!(vm.items().is_empty());
        assert!(vm.last_error().is_none());
    }

    #[test]
    fn load_list_enters_error_with_network_stub_mode() {
        let mut vm =
            LibraryViewModel::new(Box::new(StubLibraryService::new(StubMode::NetworkError)));

        vm.load_list();

        assert_eq!(vm.pane_state(), &LibraryPaneState::Error);
        assert!(vm.items().is_empty());
        assert!(vm.last_error().is_some());
        assert_eq!(vm.last_error().map(|e| e.kind),
                   Some(LibraryServiceErrorKind::Network));
    }

    #[test]
    fn select_item_loads_detail_after_list_load() {
        let mut vm = LibraryViewModel::new(Box::new(StubLibraryService::new(StubMode::Seeded)));
        vm.load_list();

        let numeric_id = vm.items()[0].numeric_id;
        vm.select_item(numeric_id);

        assert_eq!(vm.pane_state(), &LibraryPaneState::Loaded);
        assert_eq!(vm.selected().map(|i| i.numeric_id), Some(numeric_id));
        assert!(vm.last_error().is_none());
    }
}
