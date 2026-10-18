//! Library UI state and interaction controller.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use dtrpg_core::data::catalog_cache::{
    CacheMetadata, load_cache_metadata, load_catalog_cache, save_catalog_cache,
    save_check_batch_timestamp, save_fresh_install_request_timestamp,
};
use dtrpg_core::data::collection::CollectionEntry;
use dtrpg_core::data::collections_cache::{load_collections_cache, save_collections_cache};
use dtrpg_core::data::constants::{
    CATALOG_CHECKPOINT_MIN_INTERVAL_SECS, CATALOG_CHECKPOINT_PAGE_INTERVAL,
    CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS,
};
use dtrpg_core::data::cover_cache::{load_cached_cover, save_cached_cover};
use dtrpg_core::data::enums::*;
use dtrpg_core::data::library::*;
use dtrpg_core::data::paths::{cache_dir, covers_dir};
use dtrpg_core::data::selection::Selection;
use dtrpg_core::data::storage::{StorageConfig, available_space_mb_at};
use dtrpg_core::services::collections::{CollectionsService, CollectionsServiceErrorKind};
use dtrpg_core::services::network_monitor::NetworkMonitor;
use dtrpg_core::services::{LibraryService, LibraryServiceError, LibraryServiceErrorKind, retry};
use dtrpg_core::util::datetime::now_secs;
use dtrpg_core::util::filter::*;
use dtrpg_core::util::matching::*;
use dtrpg_core::util::sort::*;
use gpui::{BorrowAppContext, Bounds, Context, Entity, Pixels, SharedString, Task};
use rust_i18n::t;

use crate::controllers::activity::ActivityController;
use crate::data::events::*;
use crate::data::theme::LibriTheme;
use crate::data::theme::*;
use crate::ui::library::cover::{CoverCache, detail_cache_key};
use crate::util::file_presence::{
    ItemVerification, apply_presence_changes, collection_ids_for_item,
    find_presence_changes_in_collections, resolved_file_path_in_collections, resolved_item_dir,
};
use crate::util::publisher::*;
use crate::view_models::inspector::{InspectorContentsState, InspectorPreviewState, InspectorTab};
use crate::view_models::library::{LibraryPaneState, LibraryViewModel};

mod bulk_selection;
mod catalog;
mod catalog_sync;
mod checks;
mod collections;
mod concurrency;
mod detail;
mod diagnostics;
mod download_dispatch;
mod downloads;
mod filtering;
mod helpers;
mod lifecycle;
mod preferences;
mod selection;
mod thumbnails;

#[cfg(test)]
mod tests_catalog;
#[cfg(test)]
mod tests_checks;
#[cfg(test)]
mod tests_ui;

// Re-export the free helpers so `super::*` sees them in the split impl
// modules and the tests keep addressing them by their original path.
// `app_backtrace` lives in `diagnostics`, but the catalog-sync error path
// reaches it through `super::*`.
pub(crate) use self::diagnostics::app_backtrace;
// `create_collections_symlinks` lives with the download code that uses it, but
// the dispatch path in `download_dispatch` reaches it through `super::*`.
pub(crate) use self::downloads::create_collections_symlinks;
pub(crate) use self::helpers::*;

/// Snapshot of all data needed by the root view for a single render pass.
pub struct LibrarySnapshot {
    pub filter:                    SidebarFilter,
    pub counts:                    SectionCounts,
    pub publishers:                Vec<PublisherEntry>,
    pub collections:               Vec<CollectionEntry>,
    /// True once the initial collections fetch for the current session has
    /// completed; the sidebar shows "?" for the collection count until then.
    pub collections_loaded:        bool,
    /// All numeric product IDs in the full catalog; used by the sidebar to
    /// compute per-collection resolved item counts.
    pub catalog_ids:               HashSet<u64>,
    /// Total items in the full catalog (no filter, no search).
    pub total_count:               usize,
    pub total_mb:                  f64,
    /// Items matching the active sidebar filter but ignoring the search query.
    pub filter_count:              usize,
    /// Items matching both the active sidebar filter and the search query.
    pub matched_count:             usize,
    pub search_query:              String,
    pub sort:                      SortMethod,
    pub sort_direction:            SortDirection,
    /// Current sidebar Collections sort method.
    pub collection_sort:           CollectionSortMethod,
    /// Current sidebar Collections sort direction.
    pub collection_sort_direction: SortDirection,
    pub grouped:                   bool,
    pub presentation:              CatalogPresentation,
    pub selected_item:             Option<LibraryItem>,
    pub items:                     Vec<LibraryItem>,
    pub catalog_loading:           bool,
    /// Whether the publishers section's inline search bar is expanded.
    /// Session-only; never persisted.
    pub publisher_search_open:     bool,
    /// Publishers section search filter text. Session-only; never persisted.
    pub publisher_search_query:    String,
    /// Whether the collections section's inline search bar is expanded.
    /// Session-only; never persisted.
    pub collection_search_open:    bool,
    /// Collections section search filter text. Session-only; never persisted.
    pub collection_search_query:   String,
    /// Current width of the detail panel, in pixels.
    pub detail_panel_width:        f32,
    /// Whether bulk-selection mode is active (see `catalog-selection`).
    pub bulk_selection_mode:       bool,
    /// Ids of catalog items currently selected for bulk actions.
    pub bulk_selected_ids:         HashSet<Arc<str>>,
    /// `bulk_selected_ids.len()`, exposed directly so views don't need to
    /// clone the set just to show a count.
    pub bulk_selection_count:      usize,
}

/// Error from a cover/avatar thumbnail fetch, carrying the `Retry-After`
/// duration when the failure was an HTTP 429 response so the fetch's
/// `retry_with_backoff` call can honor it instead of the default backoff.
struct ThumbnailFetchError {
    /// Human-readable description of the failure, logged as-is.
    message:     String,
    /// The server-specified wait duration from a `Retry-After` header, when
    /// the failure was an HTTP 429 response that included one.
    retry_after: Option<std::time::Duration>,
}

/// Owns all mutable state for the library view.
pub struct LibraryController {
    /// View model that owns the service and pane state.
    vm:                            LibraryViewModel,
    /// Keeps the `ActivityController` entity alive so the weak reference in
    /// background task closures remains valid for the lifetime of this
    /// controller.
    #[allow(dead_code)]
    activity:                      Entity<ActivityController>,
    /// Full catalog — never filtered.
    catalog:                       Vec<LibraryItem>,
    /// Active sidebar filter.
    pub filter:                    SidebarFilter,
    /// Text search query.
    pub search_query:              String,
    /// Current sort method.
    pub sort:                      SortMethod,
    /// Current sort direction.
    pub sort_direction:            SortDirection,
    /// Current sidebar Collections sort method, persisted via `UiPreferences`.
    pub collection_sort:           CollectionSortMethod,
    /// Current sidebar Collections sort direction, persisted via
    /// `UiPreferences`.
    pub collection_sort_direction: SortDirection,
    /// Whether the catalog is grouped by publisher.
    pub grouped:                   bool,
    /// Active catalog presentation mode.
    pub presentation:              CatalogPresentation,
    /// The currently selected item id (for the detail panel).
    pub selection:                 Selection,
    /// Smart section counts derived from the full catalog.
    pub section_counts:            SectionCounts,
    /// Publisher list derived from the full catalog (count desc, name asc).
    pub publishers:                Vec<PublisherEntry>,
    /// Collection list loaded from the API product-list endpoint.
    pub collections:               Vec<CollectionEntry>,
    /// True once the first `apply_collections` call has completed for the
    /// current session. Used by the sidebar to show a "?" placeholder for
    /// the collection count instead of a misleading `0` while the initial
    /// fetch is still in flight.
    collections_loaded:            bool,
    /// Backing service for collections; stored so it can be replaced on
    /// sign-in.
    collections_service:           Arc<dyn CollectionsService>,
    /// Set of numeric product IDs belonging to the active collection filter.
    /// Populated by `set_filter` when `SidebarFilter::Collection(_)` is set;
    /// cleared when any other filter is active.
    pub collection_members:        HashSet<u64>,
    /// Queue of `(item_id, cover_url, force_network)` triples pending thumbnail
    /// fetches. `force_network` skips the disk cache and always re-fetches —
    /// set for manual "Load Thumbnail"/"Refresh Thumbnails" actions, which
    /// exist specifically to bypass a stale cached image; left `false` for
    /// automatic background loads.
    thumbnail_queue:               VecDeque<(Arc<str>, Arc<str>, bool)>,
    /// Number of thumbnail fetches currently in flight. Bounded by
    /// [`Self::available_slots`] together with `active_downloads`.
    active_thumbnail_fetches:      usize,
    /// Activity id for the aggregated thumbnail loading entry.
    thumbnail_activity_id:         Option<u64>,
    /// Number of thumbnails processed (successes and failures both count) in
    /// the current batch. Reset to `0` whenever a new aggregated activity
    /// item starts. Together with the live queue length this gives a real,
    /// monotonically advancing progress fraction — `processed / (processed
    /// + queue.len())` — instead of an indeterminate placeholder.
    thumbnail_processed:           usize,
    /// True while a [`Self::refresh_all_thumbnails`] batch is in progress.
    /// Drives the distinct "Refreshing…" activity label; cleared once
    /// `thumbnail_refresh_pending` drains to empty.
    thumbnail_refresh_active:      bool,
    /// Item ids belonging to the in-progress refresh batch that have not yet
    /// reported a fetch result. `dispatch_thumbnail_fetch` removes an id here
    /// the first time *any* fetch for it completes — including a fetch that
    /// was already in flight from an unrelated background load before the
    /// refresh started — so a duplicate or unrelated completion for the same
    /// id can never be double-counted toward the refresh batch's totals, and
    /// results from ordinary per-page loading (which shares the same queue
    /// and activity entry) are never folded in either.
    thumbnail_refresh_pending:     HashSet<Arc<str>>,
    /// Number of refresh-batch fetches that completed successfully. Only
    /// items removed from `thumbnail_refresh_pending` count here.
    thumbnail_refresh_succeeded:   usize,
    /// Number of refresh-batch fetches that failed. Only items removed from
    /// `thumbnail_refresh_pending` count here.
    thumbnail_refresh_failed:      usize,
    /// True from startup until the first `set_catalog` call completes.
    catalog_loading:               bool,
    /// Incremented each time [`start_load_inner`](Self::start_load_inner)
    /// starts a new load attempt. Background tasks from a superseded load
    /// compare their captured generation against the current value before
    /// writing catalog state, so a load started before a `clear_and_reload`
    /// cannot clobber it after the fact.
    load_generation:               u64,
    /// True while a catalog-sync load (from [`start_load_inner`]) is in
    /// flight. Unlike `load_generation` (which lets a superseded load's
    /// *results* be discarded after the fact), this prevents a second
    /// catalog-sync task from starting at all while one is running, per
    /// `rust-resource-work-queue-topology`'s serial-catalog-sync requirement.
    ///
    /// [`start_load_inner`]: Self::start_load_inner
    catalog_sync_in_flight:        bool,
    /// True if a load was requested (via [`start_load_inner`]) while
    /// `catalog_sync_in_flight` was already set, so it couldn't start
    /// immediately. Consumed by [`Self::finish_catalog_sync`], which starts
    /// a follow-up load once the in-flight one completes rather than
    /// silently dropping the request — critical for callers like
    /// [`Self::replace_service`] that clear the catalog and depend on their
    /// reload actually happening.
    ///
    /// [`start_load_inner`]: Self::start_load_inner
    pending_reload:                bool,
    /// Shared connectivity monitor consulted before catalog-sync and
    /// cover-thumbnail network requests. Held as an `Arc` so background
    /// tasks spawned on gpui's executor can query it without borrowing
    /// `self`.
    network_monitor:               Arc<NetworkMonitor>,
    /// Cached filtered/sorted result of the current catalog, filter, search
    /// query, and sort settings. `None` means stale; recomputed lazily by
    /// [`cached_visible_items`](Self::cached_visible_items).
    items_cache:                   Option<Vec<LibraryItem>>,
    /// Whether the publishers section's inline search bar is expanded.
    /// Session-only; never persisted.
    publisher_search_open:         bool,
    /// Publishers section search filter text. Session-only; never persisted.
    publisher_search_query:        String,
    /// Whether the collections section's inline search bar is expanded.
    /// Session-only; never persisted.
    collection_search_open:        bool,
    /// Collections section search filter text. Session-only; never persisted.
    collection_search_query:       String,
    /// Current width of the detail panel, in pixels. Session-only; never
    /// persisted to disk.
    detail_panel_width:            f32,
    /// On-screen bounds of every currently visible Grid card / Thumbs row,
    /// keyed by item id, continuously refreshed by each entry's own render
    /// pass (see `catalog_view::render_grid_card` / `render_thumb_row`).
    ///
    /// Used to anchor the single-click item popover beside the entry that
    /// opened it. Kept for *every* visible entry (not just the selected one)
    /// so the bounds for whichever entry gets clicked are already known —
    /// the entry was necessarily visible, and therefore already painted,
    /// before the click could happen — avoiding a one-frame flash at a
    /// fallback position before the popover settles into place. Entries
    /// scrolled out of view keep their last-known bounds; this is harmless
    /// since a stale entry can't be clicked again until it repaints.
    entry_bounds:                  HashMap<Arc<str>, Bounds<Pixels>>,
    /// Selected item file row (its position within the entry's `files`
    /// list) within a multi-item entry's expanded detail tab, keyed by
    /// catalog entry id. Keyed by row position rather than file id because
    /// the API has been observed to reuse the same download id across
    /// genuinely distinct files within a bundle (see
    /// `LibraryItem::dedupe_files`), which would otherwise make two
    /// unrelated rows select and deselect together. Ephemeral — never
    /// persisted, and cleared whenever the entry's detail tab is closed or
    /// reopened (see `catalog-entry-detail-view`).
    selected_item_file:            HashMap<Arc<str>, usize>,
    /// Active v2 inspector tab per detail entry. Session-only state.
    inspector_tabs:                HashMap<Arc<str>, InspectorTab>,
    /// Lazily loaded archive entries for each inspector entry.
    inspector_contents:            HashMap<Arc<str>, InspectorContentsState>,
    /// Selected archive entry paths per inspector entry.
    inspector_selected_entries:    HashMap<Arc<str>, HashSet<String>>,
    /// Whether an extraction task is active for an inspector entry.
    inspector_extractions:         HashMap<Arc<str>, bool>,
    /// Preview state for one archive entry per inspector item.
    inspector_preview:             HashMap<Arc<str>, InspectorPreviewState>,
    /// Whether the "Other details" disclosure section is expanded in a
    /// catalog entry's detail tab, keyed by catalog entry id. Ephemeral —
    /// never persisted, and defaults to collapsed for entries with no entry
    /// here (see `catalog-entry-detail-advanced-disclosure`).
    other_details_open:            HashMap<Arc<str>, bool>,
    /// Whether a per-file "Other details" disclosure is expanded within a
    /// multi-item entry's item tier, keyed by `"{entry_id}:{row_ix}"`. Keyed
    /// by row position rather than file id for the same reason as
    /// `selected_item_file`. Ephemeral — never persisted, and defaults to
    /// collapsed.
    file_other_details_open:       HashMap<Arc<str>, bool>,
    /// Per-file Info column metadata (PDF page count, container entry count, image
    /// dimensions), keyed by `(entry_id, row_ix)` and computed once on first
    /// access — see [`Self::file_info`]. Evicted per-entry whenever a file's
    /// `downloaded` flag transitions, since re-downloading may change the
    /// file's content.
    file_info_cache:               HashMap<(Arc<str>, usize), crate::util::file_info::FileInfo>,
    /// Ids of catalog items with an availability check currently in flight
    /// (on-demand or queued), mirroring `CoverCache::in_flight`'s role for
    /// thumbnails. `LibraryChanged` is emitted on insertion and removal so
    /// catalog card/row rendering can query [`Self::is_checking`] and show a
    /// spinner/overlay.
    checking_items:                HashSet<Arc<str>>,
    /// Ids of catalog items whose file-presence verification (see
    /// `verify-downloaded-status-against-disk`) is currently in flight —
    /// either as part of a catalog-wide load-time pass or an on-demand
    /// single-item check. Distinct from `checking_items` (a network-bound
    /// availability check) so catalog/popover rendering can show a visually
    /// distinct "checking download status" indicator via
    /// [`Self::is_verifying_downloads`] without conflating the two kinds of
    /// check. `LibraryChanged` is emitted on insertion and removal.
    verifying_downloads:           HashSet<Arc<str>>,
    /// Queue of item ids awaiting a periodic per-item availability check
    /// (see `catalog-item-level-reconciliation`), mirroring `thumbnail_queue`.
    check_queue:                   VecDeque<Arc<str>>,
    /// Queue of `(item_id, file_index, title)` triples pending a file
    /// download. Keyed per file (not per entry) so a multi-item entry can
    /// have more than one of its files queued or active independently.
    download_queue:                VecDeque<(Arc<str>, u32, String)>,
    /// Number of file downloads currently in flight. Bounded by
    /// [`Self::available_slots`] together with `active_thumbnail_fetches`.
    active_downloads:              usize,
    /// Per-file cancellation flag for an in-flight download, keyed by
    /// `(item_id, file_index)`, set by [`Self::cancel_download`] and polled by
    /// the download task at its next checkpoint. Removed once the task
    /// observes it (success, error, or cancellation) or by `cancel_download`
    /// itself for a still-queued item.
    download_cancel_flags:         HashMap<(Arc<str>, u32), Arc<std::sync::atomic::AtomicBool>>,
    /// Activity panel id for each in-flight (slot-holding) download, keyed by
    /// `(item_id, file_index)`. Not populated for files still waiting in
    /// `download_queue`.
    download_activity_ids:         HashMap<(Arc<str>, u32), u64>,
    /// Shared thumbnail/download concurrency limit. Initialized from
    /// [`dtrpg_core::data::storage::StorageConfig`] and kept in sync with the
    /// Storage settings page via [`Self::set_max_concurrent_downloads`].
    max_concurrent_downloads:      usize,
    /// User-configured "Recently Updated" sidebar window, in days.
    /// Initialized from [`dtrpg_core::data::storage::StorageConfig`] and kept
    /// in sync with the Storage settings page via
    /// [`Self::set_recently_updated_window_days`].
    recently_updated_window_days:  u32,
    /// A queuing action stashed by a `request_*` gating wrapper after it
    /// found free disk space insufficient, awaiting the user's response to
    /// the resulting [`LowDiskSpaceWarning`]. Consumed by
    /// [`Self::confirm_pending_download`] or cleared by
    /// [`Self::cancel_pending_download`].
    pending_download:              Option<PendingDownloadRequest>,
    /// Whether bulk-selection mode is active, toggled from the toolbar (see
    /// `catalog-selection`). When off, `bulk_selected_ids` is always empty.
    bulk_selection_mode:           bool,
    /// Ids of catalog items currently selected for a bulk action. Cleared
    /// whenever selection mode is turned off or a bulk action dispatches
    /// (see [`Self::clear_bulk_selection_and_exit_mode`]).
    bulk_selected_ids:             HashSet<Arc<str>>,
}

/// A user-initiated download-queuing action stashed while a
/// [`LowDiskSpaceWarning`] confirmation is pending.
///
/// One variant per `request_*` gating wrapper — see `LibraryController`'s
/// `request_download`, `request_item_download`,
/// `request_download_all_for_collection`, `request_download_all_for_publisher`,
/// and `request_download_all_for_filter`.
enum PendingDownloadRequest {
    /// Stashed by [`LibraryController::request_download`].
    Item {
        /// Entry id to pass to [`LibraryController::enqueue_download`].
        id:    Arc<str>,
        /// Display title to pass to [`LibraryController::enqueue_download`].
        title: String,
    },
    /// Stashed by [`LibraryController::request_item_download`].
    ItemFile {
        /// Entry id to pass to [`LibraryController::enqueue_item_download`].
        id:    Arc<str>,
        /// File index to pass to [`LibraryController::enqueue_item_download`].
        index: u32,
        /// Display title to pass to
        /// [`LibraryController::enqueue_item_download`].
        title: String,
    },
    /// Stashed by [`LibraryController::request_download_all_for_collection`].
    Collection {
        /// Collection id to pass to
        /// [`LibraryController::download_all_for_collection`].
        collection_id: u64,
    },
    /// Stashed by [`LibraryController::request_download_all_for_publisher`].
    Publisher {
        /// Publisher name to pass to
        /// [`LibraryController::download_all_for_publisher`].
        publisher: Arc<str>,
    },
    /// Stashed by [`LibraryController::request_download_all_for_filter`].
    Filter {
        /// Sidebar filter to pass to
        /// [`LibraryController::download_all_for_filter`].
        filter: SidebarFilter,
    },
}
