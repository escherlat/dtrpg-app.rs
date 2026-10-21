//! Library UI state and interaction controller.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dtrpg_core::data::catalog_cache::{load_cache_metadata, save_check_batch_timestamp};
use dtrpg_core::data::collection::CollectionEntry;
use dtrpg_core::data::collections_cache::save_collections_cache;
use dtrpg_core::data::cover_cache::load_cached_cover;
use dtrpg_core::data::enums::*;
use dtrpg_core::data::library::*;
use dtrpg_core::data::paths::{cache_dir, covers_dir};
use dtrpg_core::data::selection::Selection;
use dtrpg_core::data::storage::{StorageConfig, available_space_mb_at};
use dtrpg_core::data::tag_library::TagEntry;
use dtrpg_core::library::{
    Library, LibraryEvent, LibraryEvents, SyncStage, ViewState, reload_cooldown_active,
};
use dtrpg_core::services::collections::{CollectionsService, CollectionsServiceErrorKind};
use dtrpg_core::services::{DownloadControl, LibraryService, LibraryServiceErrorKind};
use dtrpg_core::util::datetime::now_secs;
use dtrpg_core::util::filter::*;
use dtrpg_core::util::matching::*;
use dtrpg_core::util::publisher::*;
use dtrpg_core::util::sort::*;
use dtrpg_core::work::{JobEvent, JobEvents, JobId, Outcome, Priority};
use gpui::{AppContext, BorrowAppContext, Bounds, Context, Entity, Pixels, SharedString, Window};
use gpui_kit::component::input::{InputEvent, InputState};
use rust_i18n::t;

use crate::controllers::activity::ActivityController;
use crate::data::events::*;
use crate::data::theme::LibriTheme;
use crate::data::theme::*;
use crate::ui::library::cover::{CoverCache, detail_cache_key};
use crate::util::file_presence::{
    ItemVerification, collection_ids_for_item, find_presence_changes_in_collections,
    resolved_file_path_in_collections, resolved_item_dir,
};
use crate::view_models::inspector::{InspectorContentsState, InspectorPreviewState, InspectorTab};
use crate::view_models::library::{LibraryPaneState, LibraryViewModel};

mod bulk_selection;
mod catalog;
mod catalog_sync;
mod checks;
mod collection_links;
mod collections;
mod concurrency;
mod detail;
mod download_dispatch;
mod downloads;
mod file_tags;
mod filtering;
mod helpers;
mod lifecycle;
mod preferences;
mod selection;
mod tags;
mod theme_files;
mod thumbnails;

#[cfg(test)]
mod tests_availability;
#[cfg(test)]
mod tests_checks;
#[cfg(test)]
mod tests_collection_links;
#[cfg(test)]
mod tests_downloads;
#[cfg(test)]
mod tests_file_tags;
#[cfg(test)]
mod tests_support;
#[cfg(test)]
mod tests_tags;
#[cfg(test)]
mod tests_thumbnails;
#[cfg(test)]
mod tests_ui;
#[cfg(test)]
mod tests_view_cache;

// Re-export the free helpers so `super::*` sees them in the split impl
// modules and the tests keep addressing them by their original path.
// `create_collections_symlinks` lives with the download code that uses it, but
// the dispatch path in `download_dispatch` reaches it through `super::*`.
pub(crate) use self::downloads::create_collections_symlinks;
pub(crate) use self::helpers::*;

/// A collapsible section of the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSection {
    Publishers,
    Collections,
    Tags,
}

/// Which sidebar sections are expanded.
///
/// Read from disk once, when the controller is made, and kept here, because the
/// sidebar is drawn on every change and drawing must not read files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarOpen {
    pub publishers:  bool,
    pub collections: bool,
    pub tags:        bool,
}

impl SidebarOpen {
    /// The sections as they were left, as `ui_state` has them saved.
    #[must_use]
    pub fn saved() -> Self {
        let saved = dtrpg_core::data::ui_state::UiState::load();
        Self { publishers:  saved.publishers_open(),
               collections: saved.collections_open(),
               tags:        saved.tags_open(), }
    }
}

/// Snapshot of all data needed by the root view for a single render pass.
pub struct LibrarySnapshot {
    pub filter:                    SidebarFilter,
    pub counts:                    SectionCounts,
    pub publishers:                Vec<PublisherEntry>,
    pub collections:               Vec<CollectionEntry>,
    /// Every tag, for the sidebar's Tags section.
    pub tags:                      Vec<TagEntry>,
    /// True once the initial collections fetch for the current session has
    /// completed; the sidebar shows "?" for the collection count until then.
    pub collections_loaded:        bool,
    /// All numeric product IDs in the full catalog; used by the sidebar to
    /// compute per-collection resolved item counts.
    pub catalog_ids:               Arc<HashSet<u64>>,
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
    pub group_by:                  GroupBy,
    /// Which sidebar sections are expanded.
    pub sidebar_open:              SidebarOpen,
    /// The optional list columns that are shown.
    pub list_columns:              Vec<ListColumn>,
    pub presentation:              CatalogPresentation,
    pub selected_item:             Option<LibraryItem>,
    pub items:                     Arc<Vec<LibraryItem>>,
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

/// Owns all mutable state for the library view.
pub struct LibraryController {
    /// View model that owns the service and pane state.
    vm:                            LibraryViewModel,
    /// Keeps the `ActivityController` entity alive so the weak reference in
    /// background task closures remains valid for the lifetime of this
    /// controller.
    #[allow(dead_code)]
    activity:                      Entity<ActivityController>,
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
    /// Which sidebar sections are expanded.
    pub sidebar_open:              SidebarOpen,
    /// What the catalog is grouped by.
    pub group_by:                  GroupBy,
    /// The optional list columns that are shown, in their usual order.
    pub list_columns:              Vec<ListColumn>,
    /// Active catalog presentation mode.
    pub presentation:              CatalogPresentation,
    /// The currently selected item id (for the detail panel).
    pub selection:                 Selection,
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
    /// Headless library session: owns the index, work queue, cover fetcher,
    /// network monitor, and (since Slice 2) the catalog-sync state machine.
    /// `Arc` since catalog-sync runs on a background thread while the
    /// controller renders — see `dtrpg_core::library::Library`'s own doc
    /// comment.
    pub(super) library:            Arc<Library>,
    /// Activity id for the aggregated thumbnail loading entry.
    thumbnail_activity_id:         Option<u64>,
    /// Number of thumbnails processed (successes and failures both count) in
    /// the current batch. Reset to `0` whenever a new aggregated activity
    /// item starts. Together with the number of fetches not yet finished this
    /// gives a real, monotonically advancing progress fraction —
    /// `processed / (processed + unfinished)` — instead of an indeterminate
    /// placeholder.
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
    /// True from startup until the first catalog-sync run completes.
    catalog_loading:               bool,
    /// Activity id for the catalog-sync run currently in flight, if any.
    /// Single-flight tracking itself (generation, in-flight, pending
    /// reload) moved into `Library` in ADR 0002 Slice 2; this is the one
    /// piece of state that stays here, since activity items are a view
    /// concern.
    catalog_activity_id:           Option<u64>,
    /// The most recent total item count a catalog-sync run has reported,
    /// for seeding the activity item's progress bar. Cleared when the run
    /// finishes.
    catalog_progress_total:        Option<usize>,
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
    /// Selected archive file index per inspector entry.
    inspector_selected_archive:    HashMap<Arc<str>, usize>,
    /// Preview state for one archive entry per inspector item.
    inspector_preview:             HashMap<Arc<str>, InspectorPreviewState>,
    inspector_collapsed_dirs:      HashMap<Arc<str>, HashSet<String>>,
    inspector_filter_inputs:       HashMap<Arc<str>, Entity<InputState>>,
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
    /// Per-file Info column metadata (PDF page count, container entry count,
    /// image dimensions), keyed by `(entry_id, row_ix)` and computed once
    /// on first access — see [`Self::file_info`]. Evicted per-entry
    /// whenever a file's `downloaded` flag transitions, since
    /// re-downloading may change the file's content.
    file_info_cache:               HashMap<(Arc<str>, usize), crate::util::file_info::FileInfo>,
    /// Whether files' tags are being compared with the library's right now.
    file_tag_sync_running:         bool,
    /// The storage settings to use instead of the ones on disk. Only tests set
    /// it, so that they do not read or depend on the developer's own settings.
    storage_override:              Option<Arc<StorageConfig>>,
    /// Counts the reads and edits of tags that have been started, so a result
    /// that arrives after a newer one is dropped.
    tag_generation:                u64,
    /// Whether the files' tags have been compared with the library's in this
    /// launch. A reload compares them again.
    file_tags_reconciled:          bool,
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

mod accessors;

#[path = "pending_download.rs"]
mod pending_download;

use pending_download::PendingDownloadRequest;
