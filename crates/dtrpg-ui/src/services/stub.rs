//! Test stubs for the library service.

use crate::data::library::LibraryItem;
use crate::services::{LibraryService, LibraryServiceError, LibraryServiceErrorKind};
use crate::util::stubs::stub_catalog;

/// Controls which canned behavior a [`StubLibraryService`] exhibits.
#[derive(Clone, Copy, Debug)]
pub enum StubMode {
    /// Returns a seeded set of fake library items.
    Seeded,
    /// Returns an empty item list.
    Empty,
    /// Returns a network error.
    NetworkError,
    /// Returns a session error.
    SessionError,
}

/// A deterministic in-memory [`LibraryService`] for unit tests.
pub struct StubLibraryService {
    mode: StubMode,
}

impl StubLibraryService {
    /// Creates a stub that behaves according to `mode`.
    pub fn new(mode: StubMode) -> Self {
        Self { mode }
    }
}

impl LibraryService for StubLibraryService {
    fn list_items(&self) -> Result<Vec<LibraryItem>, LibraryServiceError> {
        match self.mode {
            StubMode::Seeded => Ok(stub_catalog()),
            StubMode::Empty => Ok(vec![]),
            StubMode::NetworkError => {
                Err(LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                             "stub: simulated network error"))
            }
            StubMode::SessionError => {
                Err(LibraryServiceError::new(LibraryServiceErrorKind::Session,
                                             "stub: simulated session error"))
            }
        }
    }

    fn get_item(&self, _id: u64) -> Result<LibraryItem, LibraryServiceError> {
        match self.mode {
            StubMode::Seeded => stub_catalog().into_iter().next().ok_or_else(|| {
                LibraryServiceError::new(LibraryServiceErrorKind::NotFound, "stub: no items")
            }),
            StubMode::Empty => Err(LibraryServiceError::new(
                LibraryServiceErrorKind::NotFound,
                "stub: no items in empty mode",
            )),
            StubMode::NetworkError => Err(LibraryServiceError::new(
                LibraryServiceErrorKind::Network,
                "stub: simulated network error",
            )),
            StubMode::SessionError => Err(LibraryServiceError::new(
                LibraryServiceErrorKind::Session,
                "stub: simulated session error",
            )),
        }
    }

    fn download_item(&self, _order_product_id: u64, _index: u32, dest: &std::path::Path,
                     _cancel: &std::sync::atomic::AtomicBool,
                     _on_retry: Option<&mut dyn FnMut(u32, std::time::Duration)>)
                     -> Result<(), LibraryServiceError> {
        match self.mode {
            StubMode::Seeded | StubMode::Empty => {
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(dest, b"stub download content").map_err(|e| {
                                                                  LibraryServiceError::new(
                        LibraryServiceErrorKind::Network,
                        format!("stub: failed to write {}: {e}", dest.display()),
                    )
                                                              })
            }
            StubMode::NetworkError => {
                Err(LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                             "stub: simulated network error"))
            }
            StubMode::SessionError => {
                Err(LibraryServiceError::new(LibraryServiceErrorKind::Session,
                                             "stub: simulated session error"))
            }
        }
    }
}
