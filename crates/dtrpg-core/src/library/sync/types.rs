//! The vocabulary `Library::start_load` and its stages emit and read.

use crate::data::collection::CollectionEntry;
use crate::data::library::LibraryItem;

/// The receiving end of a [`crate::library::Library`]'s catalog-sync event
/// stream. Read it with `recv().await`.
pub type LibraryEvents = tokio::sync::mpsc::UnboundedReceiver<LibraryEvent>;

/// A step in one catalog-sync run.
///
/// Events for one run arrive in order: any number of `Stage`/`Total`/`Page`
/// events, then exactly one of the terminal events (`SkippedFreshCache`,
/// `FullyLoaded`, or `Failed`), then always `Finished`. The controller reacts
/// to each by updating its own view state (thumbnail enqueue, sidebar facet
/// refresh, cache invalidation, `LibraryChanged`) — `Library` itself holds no
/// view state.
#[derive(Clone, Debug)]
pub enum LibraryEvent {
    /// Entered a named stage, for progress-label display.
    Stage(SyncStage),
    /// `total` items are now known. Used to seed or correct a progress bar.
    Total(usize),
    /// Page `page` (1-based) of the live fetch arrived.
    Page(u32),
    /// `items` were loaded from the on-disk cache and appended to the index
    /// ahead of the network stages. `force_reload` is the run's own flag,
    /// echoed back so the controller can decide whether to start tag sync.
    CachePrepopulated {
        items:        Vec<LibraryItem>,
        force_reload: bool,
    },
    /// The collections list changed (the cache hit, then the live refresh
    /// each produce one of these).
    CollectionsChanged(Vec<CollectionEntry>),
    /// `items` were appended to the index during the live fetch's
    /// incremental phase (see `catalog-live-merge`).
    PageAppended { items: Vec<LibraryItem> },
    /// A partial, additive fetch merged `items` into the index.
    PartialMerged { items: Vec<LibraryItem> },
    /// The auto-load policy found the on-disk cache fresh and skipped the
    /// live fetch entirely.
    SkippedFreshCache,
    /// The full live fetch replaced the catalog with `items`.
    FullyLoaded { items: Vec<LibraryItem> },
    /// The run ended without a successful live fetch: the network was
    /// unreachable, or the fetch itself failed. `quiet` is true for an
    /// unauthenticated-session error, which is routine rather than a
    /// user-facing failure.
    Failed { message: String, quiet: bool },
    /// The run is over. Always the last event for one run, whatever else
    /// happened. `reload_requested` is true when a follow-up load was
    /// requested while this one was in flight — the controller should call
    /// `start_load` again.
    Finished { reload_requested: bool },
}

/// Which stage of a catalog-sync run is running, for progress-label display.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyncStage {
    /// Refreshing the collections list.
    Collections,
    /// Checking the remote item count against the cache.
    CountCheck,
    /// Running a cheaper additive partial fetch.
    PartialFetch,
    /// Running the full paginated live fetch.
    LiveFetch,
}

/// Catalog-sync single-flight state, held in `Library` behind a `Mutex`.
///
/// Mirrors the UI controller's former `load_generation`/
/// `catalog_sync_in_flight`/`pending_reload` fields exactly: at most one sync
/// runs at a time; a call that arrives while one is in flight is recorded as
/// a pending follow-up rather than started or dropped.
#[derive(Default)]
pub(crate) struct SyncState {
    pub(super) generation:     u64,
    pub(super) in_flight:      bool,
    pub(super) pending_reload: bool,
}
