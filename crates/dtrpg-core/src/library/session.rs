//! The `Library` struct itself: construction and the fields every other file
//! in this module operates on.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use super::jobs::{CheckJobs, DownloadJobs, ThumbnailJobs};
use super::mutations::CollectionsState;
use super::queries::{EphemeralOverlay, QueryCache};
use super::sync::{LibraryEvent, LibraryEvents, SyncState};
use crate::data::index::{IndexError, LibraryIndex};
use crate::data::paths::covers_dir;
use crate::data::storage::StorageConfig;
use crate::data::tag_library::TagLibrary;
use crate::services::covers::{CoverFetcher, HttpCoverTransport};
use crate::services::network_monitor::NetworkMonitor;
use crate::work::{JobEvents, WorkQueue};

/// Long-lived, headless library session.
///
/// All mutable state inside `Library` is internally synchronized (the index
/// and queue use their own locks), so `Library` can be shared via `Arc` and
/// used from the UI controller without making it a `gpui::Entity`.
pub struct Library {
    /// The local SQLite index: the catalog's only storage. Behind a `Mutex`
    /// (not a plain field) since ADR 0002 Slice 2: the catalog-sync run
    /// mutates it from a background thread while `Library` is shared via
    /// `Arc`, which rules out the `&mut self` a plain field would need.
    pub index:                 Mutex<LibraryIndex>,
    /// The shared work queue that runs downloads, checks, and thumbnail
    /// fetches.
    pub work:                  WorkQueue,
    /// Fetches cover images, reading the disk cache before the network.
    pub cover_fetcher:         Arc<CoverFetcher>,
    /// Shared connectivity monitor consulted before network requests.
    pub network_monitor:       Arc<NetworkMonitor>,
    /// Directory the library index database lives in.
    pub cache_root:            PathBuf,
    /// Catalog-sync single-flight state: the current generation, whether a
    /// sync is running, and whether a follow-up was requested while busy.
    pub(super) sync_state:     Mutex<SyncState>,
    /// Sender for this session's [`LibraryEvent`]s. The controller holds the
    /// matching [`LibraryEvents`] receiver returned by [`Library::open`].
    pub(super) events:         tokio::sync::mpsc::UnboundedSender<LibraryEvent>,
    /// The filtered/grouped query cache: ADR 0002 Slice 3.
    pub(super) query_cache:    Mutex<QueryCache>,
    /// Session-only per-item timestamps the index never persists.
    pub(super) ephemeral:      Mutex<EphemeralOverlay>,
    /// The library's tags, as last set by [`Library::set_tags`].
    pub(super) tags:           Mutex<Arc<TagLibrary>>,
    /// Counts the reads and edits of tags that have been started, so a result
    /// that arrives after a newer one is dropped: ADR 0002 Slice 5.
    pub(super) tag_generation: AtomicU64,
    /// Thumbnail-fetch job bookkeeping: ADR 0002 Slice 4.
    pub(super) thumbnail_jobs: Mutex<ThumbnailJobs>,
    /// File-download job bookkeeping: ADR 0002 Slice 4.
    pub(super) download_jobs:  Mutex<DownloadJobs>,
    /// Availability-check job bookkeeping: ADR 0002 Slice 4.
    pub(super) check_jobs:     Mutex<CheckJobs>,
    /// Items with an availability check currently running: ADR 0002 Slice 4.
    pub(super) checking_items: Mutex<HashSet<Arc<str>>>,
    /// Items whose files are being checked against disk: ADR 0002 Slice 5.
    pub(super) verifying:      Mutex<HashSet<Arc<str>>>,
    /// The collections list and whether it has been fetched: ADR 0002 Slice 5.
    pub(super) collections:    Mutex<CollectionsState>,
}

impl Library {
    /// Opens the library at `cache_root` and constructs its queue and
    /// fetchers.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or migrated.
    pub fn open(cache_root: PathBuf, storage: &StorageConfig)
                -> Result<(Self, JobEvents, LibraryEvents), IndexError> {
        let index = LibraryIndex::open_and_migrate(&cache_root)?;
        let network_monitor = Arc::new(NetworkMonitor::new());
        let cover_fetcher = Arc::new(CoverFetcher::new(
            Arc::new(HttpCoverTransport::new(Arc::clone(&network_monitor))),
            covers_dir(),
        ));
        let (work, work_events) = WorkQueue::new(storage.max_concurrent_downloads());
        let (events, sync_events) = tokio::sync::mpsc::unbounded_channel();

        Ok((Self { index: Mutex::new(index),
                   work,
                   cover_fetcher,
                   network_monitor,
                   cache_root,
                   sync_state: Mutex::new(SyncState::default()),
                   events,
                   query_cache: Mutex::new(QueryCache::default()),
                   ephemeral: Mutex::new(EphemeralOverlay::default()),
                   tags: Mutex::new(Arc::default()),
                   tag_generation: AtomicU64::new(0),
                   thumbnail_jobs: Mutex::new(ThumbnailJobs::default()),
                   download_jobs: Mutex::new(DownloadJobs::default()),
                   check_jobs: Mutex::new(CheckJobs::default()),
                   checking_items: Mutex::new(HashSet::new()),
                   verifying: Mutex::new(HashSet::new()),
                   collections: Mutex::new(CollectionsState::default()) },
            work_events,
            sync_events))
    }
}
