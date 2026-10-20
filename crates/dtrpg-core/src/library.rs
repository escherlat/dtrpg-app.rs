//! Headless library session.
//!
//! `Library` owns the long-lived state that the desktop UI's
//! `LibraryController` used to hold directly: the SQLite index, the work
//! queue, the cover fetcher, and the network monitor. It is the core side of
//! the ADR 0002 controller decomposition; later slices will move more
//! catalog/queue logic here and thin the UI controller to a view-model.

use std::path::PathBuf;
use std::sync::Arc;

use crate::data::index::{IndexError, LibraryIndex};
use crate::data::paths::covers_dir;
use crate::data::storage::StorageConfig;
use crate::services::covers::{CoverFetcher, HttpCoverTransport};
use crate::services::network_monitor::NetworkMonitor;
use crate::work::{JobEvents, WorkQueue};

/// Long-lived, headless library session.
///
/// All mutable state inside `Library` is internally synchronized (the index
/// and queue use their own locks), so `Library` can be shared via `Arc` and
/// used from the UI controller without making it a `gpui::Entity`.
pub struct Library {
    /// The local SQLite index: the catalog's only storage.
    pub index:           LibraryIndex,
    /// The shared work queue that runs downloads, checks, and thumbnail
    /// fetches.
    pub work:            WorkQueue,
    /// Fetches cover images, reading the disk cache before the network.
    pub cover_fetcher:   Arc<CoverFetcher>,
    /// Shared connectivity monitor consulted before network requests.
    pub network_monitor: Arc<NetworkMonitor>,
    /// Directory the library index database lives in.
    pub cache_root:      PathBuf,
}

impl Library {
    /// Opens the library at `cache_root` and constructs its queue and
    /// fetchers.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or migrated.
    pub fn open(cache_root: PathBuf, storage: &StorageConfig)
                -> Result<(Self, JobEvents), IndexError> {
        let index = LibraryIndex::open_and_migrate(&cache_root)?;
        let network_monitor = Arc::new(NetworkMonitor::new());
        let cover_fetcher = Arc::new(CoverFetcher::new(
            Arc::new(HttpCoverTransport::new(Arc::clone(&network_monitor))),
            covers_dir(),
        ));
        let (work, events) = WorkQueue::new(storage.max_concurrent_downloads());

        Ok((Self { index,
                   work,
                   cover_fetcher,
                   network_monitor,
                   cache_root },
            events))
    }
}
