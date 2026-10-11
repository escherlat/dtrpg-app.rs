//! Library UI state and interaction controller.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use gpui::{BorrowAppContext, Bounds, Context, Entity, Pixels, Point, SharedString, Task};
use rust_i18n::t;

use crate::controllers::activity::ActivityController;
use crate::data::catalog_cache::{
    CacheMetadata, load_cache_metadata, load_catalog_cache, save_catalog_cache,
    save_check_batch_timestamp, save_fresh_install_request_timestamp,
};
use crate::data::collection::CollectionEntry;
use crate::data::collections_cache::{load_collections_cache, save_collections_cache};
use crate::data::constants::{
    CATALOG_CHECKPOINT_MIN_INTERVAL_SECS, CATALOG_CHECKPOINT_PAGE_INTERVAL,
    CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS,
};
use crate::data::cover_cache::{load_cached_cover, save_cached_cover};
use crate::data::enums::*;
use crate::data::events::*;
use crate::data::library::*;
use crate::data::paths::{cache_dir, covers_dir};
use crate::data::selection::Selection;
use crate::data::storage::StorageConfig;
use crate::data::theme::LibriTheme;
use crate::data::theme::*;
use crate::services::collections::{CollectionsService, CollectionsServiceErrorKind};
use crate::services::network_monitor::NetworkMonitor;
use crate::services::{LibraryService, LibraryServiceError, LibraryServiceErrorKind, retry};
use crate::ui::library::cover::{CoverCache, detail_cache_key};
use crate::util::datetime::now_secs;
use crate::util::file_presence::{resolved_file_path, verify_item_downloads};
use crate::util::filter::*;
use crate::util::matching::*;
use crate::util::publisher::*;
use crate::util::sort::*;
use crate::view_models::library::{LibraryPaneState, LibraryViewModel};

/// Flips a per-key boolean flag in `map`, treating a missing entry as `false`.
///
/// Used by per-entry ephemeral UI toggles (e.g. the "Other details"
/// disclosure) that default to a collapsed/off state until first toggled.
fn toggle_bool_flag<K: std::hash::Hash + Eq>(map: &mut HashMap<K, bool>, key: K) -> bool {
    let flag = map.entry(key).or_insert(false);
    *flag = !*flag;
    *flag
}

/// Returns `entry_id`/`row_ix`'s cached Info value if present; otherwise, if
/// `downloaded` is true, computes it via `compute_file_info(path, format)`
/// and caches it. Returns `FileInfo::None` for undownloaded files without
/// touching the cache.
///
/// Extracted as a pure function (independent of `LibraryController`'s
/// service/entity fields) so the caching behavior can be unit tested
/// directly.
fn cached_file_info(cache: &mut HashMap<(Arc<str>, usize), crate::util::file_info::FileInfo>,
                    entry_id: &Arc<str>, row_ix: usize, downloaded: bool, path: &Path,
                    format: &str)
                    -> crate::util::file_info::FileInfo {
    use crate::util::file_info::{FileInfo, compute_file_info};

    let key = (Arc::clone(entry_id), row_ix);
    if let Some(info) = cache.get(&key) {
        return *info;
    }
    if !downloaded {
        return FileInfo::None;
    }
    let info = compute_file_info(path, format);
    cache.insert(key, info);
    info
}

/// Selects `row_ix` for `key` in a per-entry row-selection map, toggling it
/// off if `row_ix` is already selected for that key.
///
/// Used by the multi-item detail tab's file list so clicking the selected
/// row deselects it, and clicking any other row switches the selection to
/// it — regardless of whether rows share a duplicate id (see
/// `LibraryItem::dedupe_files`).
fn toggle_row_selection<K: std::hash::Hash + Eq>(map: &mut HashMap<K, usize>, key: K,
                                                 row_ix: usize) {
    if map.get(&key) == Some(&row_ix) {
        map.remove(&key);
    }
    else {
        map.insert(key, row_ix);
    }
}

/// Reconciles `existing` (the current catalog) against `live` (a fresh live
/// fetch) by item id, per `catalog-live-data-swap`'s reconcile-by-id
/// requirement.
///
/// - An id present in both: kept at its existing position, replaced with the
///   live item's fields, with `is_available` set to `true`. Live items always
///   carry `downloaded: false` on every file (the API has no concept of local
///   downloads), so each live file's `downloaded` flag is restored from the
///   existing item's file with the same id (if any) before recomputing the
///   entry's aggregate `status`, per `catalog-live-data-swap`'s per-file
///   downloaded-state requirement.
/// - An id present only in `existing`: kept unchanged except `is_available` is
///   set to `false` — the server no longer lists it, but it isn't removed.
/// - An id present only in `live`: appended at the end, `is_available` stays
///   `true` (the default for a freshly-fetched item).
///
/// Existing catalog ordering is preserved for known items since
/// `added_order`/list position is otherwise meaningful; only genuinely new
/// items are appended.
fn reconcile_catalog(existing: Vec<LibraryItem>, live: Vec<LibraryItem>) -> Vec<LibraryItem> {
    let mut live_by_id: HashMap<Arc<str>, LibraryItem> =
        live.into_iter()
            .map(|item| (Arc::clone(&item.id), item))
            .collect();

    // The API mints a new `orderProductId` (and thus a new `id`) when a
    // title's files are republished, so `id` alone isn't stable across a
    // republish. `product_id` is the catalog SKU and stays constant across
    // republishes, so it's used as a fallback match for an existing item
    // whose `id` no longer appears live — without this, a republished title
    // reconciles as "old id gone" (flagged unavailable) plus "new id new"
    // (appended), duplicating the entry instead of recognizing it as the
    // same book.
    let mut live_id_by_product_id: HashMap<u64, Arc<str>> = HashMap::new();
    for item in live_by_id.values() {
        if item.product_id != 0 {
            live_id_by_product_id.entry(item.product_id)
                                 .or_insert_with(|| Arc::clone(&item.id));
        }
    }

    let mut reconciled: Vec<LibraryItem> =
        existing.into_iter()
                .map(|mut item| {
                    let matched_id = if live_by_id.contains_key(&item.id) {
                        Some(Arc::clone(&item.id))
                    }
                    else if item.product_id != 0 {
                        live_id_by_product_id.get(&item.product_id).cloned()
                    }
                    else {
                        None
                    };

                    if let Some(mut live_item) = matched_id.and_then(|id| live_by_id.remove(&id)) {
                        let downloaded_by_id: HashMap<Arc<str>, bool> =
                            item.files
                                .iter()
                                .map(|f| (Arc::clone(&f.id), f.downloaded))
                                .collect();
                        for file in &mut live_item.files {
                            if let Some(&downloaded) = downloaded_by_id.get(&file.id) {
                                file.downloaded = downloaded;
                            }
                        }
                        item = live_item;
                        item.is_available = true;
                        item.recompute_status();
                    }
                    else {
                        item.is_available = false;
                    }
                    item
                })
                .collect();

    reconciled.extend(live_by_id.into_values());
    reconciled
}

/// Collapses catalog entries that share a `product_id` (the catalog SKU)
/// down to one, keeping the available entry if exactly one of the pair is
/// available, otherwise the entry with the greater `numeric_id`.
///
/// Needed for on-disk caches saved before `reconcile_catalog` learned the
/// `product_id` fallback match (see its comment): those caches can already
/// contain a stale-unavailable/new-duplicate pair for a republished title,
/// which this collapses on load rather than requiring a manual cache clear.
/// Items with `product_id == 0` (no known SKU) are never deduped against one
/// another — `0` is used elsewhere in this file as a "SKU unknown" sentinel,
/// not a real shared identity.
fn dedupe_by_product_id(items: Vec<LibraryItem>) -> Vec<LibraryItem> {
    let mut index_by_product_id: HashMap<u64, usize> = HashMap::new();
    let mut deduped: Vec<LibraryItem> = Vec::with_capacity(items.len());
    for item in items {
        if item.product_id == 0 {
            deduped.push(item);
            continue;
        }
        match index_by_product_id.get(&item.product_id) {
            Some(&ix) => {
                let current = (deduped[ix].is_available, deduped[ix].numeric_id);
                if (item.is_available, item.numeric_id) > current {
                    deduped[ix] = item;
                }
            }
            None => {
                index_by_product_id.insert(item.product_id, deduped.len());
                deduped.push(item);
            }
        }
    }
    deduped
}

/// Returns the positions of every not-yet-downloaded file in `files`, in
/// order.
///
/// Used by [`LibraryController::enqueue_download`] to determine which files
/// of a (possibly multi-item) entry to queue: an already-downloaded file is
/// never re-enqueued, so a bundle with some items already present only
/// queues the missing ones.
fn missing_file_indices(files: &[LibraryItemFile]) -> Vec<u32> {
    files.iter()
         .enumerate()
         .filter(|(_, f)| !f.downloaded)
         .map(|(idx, _)| idx as u32)
         .collect()
}

/// Sums `size_mb` across every not-yet-downloaded file in `files`.
///
/// Used by the `request_*` gating wrappers to compute the aggregate size a
/// queuing action would enqueue, for comparison against
/// [`crate::data::storage::available_space_mb`].
fn missing_files_size_mb(files: &[LibraryItemFile]) -> f64 {
    files.iter()
         .filter(|f| !f.downloaded)
         .map(|f| f.size_mb)
         .sum()
}

/// Sums [`missing_files_size_mb`] across every item in `catalog` whose id is
/// in `ids`.
///
/// Used by the collection/publisher `request_download_all_for_*` wrappers to
/// compute one aggregate size across every matching item's missing files,
/// rather than checking each item individually.
fn missing_files_size_mb_for_items<'a>(catalog: &[LibraryItem],
                                       ids: impl Iterator<Item = &'a str>)
                                       -> f64 {
    let ids: HashSet<&str> = ids.collect();
    catalog.iter()
           .filter(|item| ids.contains(item.id.as_ref()))
           .map(|item| missing_files_size_mb(&item.files))
           .sum()
}

/// Returns `(id, title)` for every `catalog` item that is a member of
/// `member_ids`, in catalog order.
///
/// Used by [`LibraryController::download_all_for_collection`] to determine
/// which items to pass to [`LibraryController::enqueue_download`]. Membership
/// matching (not download status) is this function's only concern —
/// already-downloaded items are filtered out downstream by
/// [`enqueue_download`](LibraryController::enqueue_download) via
/// [`missing_file_indices`], not here.
fn collection_download_targets(catalog: &[LibraryItem], member_ids: &[u64])
                               -> Vec<(Arc<str>, Arc<str>)> {
    catalog.iter()
           .filter(|item| member_ids_contain(member_ids, item.order_product_id, item.product_id))
           .map(|item| (Arc::clone(&item.id), Arc::clone(&item.title)))
           .collect()
}

/// Returns `(id, title)` for every `catalog` item whose publisher exactly
/// matches `publisher`, in catalog order.
///
/// Used by [`LibraryController::download_all_for_publisher`] to determine
/// which items to pass to [`LibraryController::enqueue_download`]. Publisher
/// matching (not download status) is this function's only concern —
/// already-downloaded items are filtered out downstream by
/// [`enqueue_download`](LibraryController::enqueue_download) via
/// [`missing_file_indices`], not here.
fn publisher_download_targets(catalog: &[LibraryItem], publisher: &str)
                              -> Vec<(Arc<str>, Arc<str>)> {
    catalog.iter()
           .filter(|item| item.publisher.as_ref() == publisher)
           .map(|item| (Arc::clone(&item.id), Arc::clone(&item.title)))
           .collect()
}

/// Removes the queued download for `(id, index)` from `queue`, if present.
///
/// Returns `true` if an entry was removed. Only the matching `(id, index)`
/// pair is removed — other queued files for the same entry (different
/// index) or for other entries are left untouched.
fn dequeue_file(queue: &mut VecDeque<(Arc<str>, u32, String)>, id: &str, index: u32) -> bool {
    let before = queue.len();
    queue.retain(|(qid, qidx, _)| !(qid.as_ref() == id && *qidx == index));
    queue.len() != before
}

/// Returns `true` if a user-requested full reload should be suppressed
/// because the cache was written too recently, per
/// `FORCE_RELOAD_COOLDOWN_SECS`.
///
/// `meta` is `None` when the cache metadata file is missing or unreadable
/// (e.g. before any successful sync, or right after `clear_and_reload`
/// clears the on-disk cache) — treated as "not recently reloaded," so a
/// missing timestamp never blocks a reload.
fn reload_cooldown_active(meta: Option<&CacheMetadata>, now_secs: u64) -> bool {
    let Some(meta) = meta
    else {
        return false;
    };
    now_secs.saturating_sub(meta.saved_at_secs) < crate::data::constants::FORCE_RELOAD_COOLDOWN_SECS
}

/// Returns `true` if a fresh-install catalog-initialization request should be
/// suppressed because the last one happened too recently, per
/// `CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS`.
///
/// `last_request_secs` is `None` when no fresh-install request has ever been
/// recorded (first launch, or metadata missing/unreadable) — treated as "not
/// recently requested," so a missing timestamp never blocks the request.
fn fresh_install_request_gated(last_request_secs: Option<u64>, now_secs: u64) -> bool {
    let Some(last) = last_request_secs
    else {
        return false;
    };
    now_secs.saturating_sub(last) < CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS
}

/// Returns `true` if an item last checked at `last_checked` is due for
/// another availability check, per `ITEM_CHECK_COOLDOWN_SECS`. `None` (never
/// checked) is always due.
fn item_check_due(last_checked: Option<std::time::SystemTime>) -> bool {
    match last_checked {
        None => true,
        Some(last) => {
            last.elapsed().map(|e| e.as_secs()).unwrap_or(0)
            >= crate::data::constants::ITEM_CHECK_COOLDOWN_SECS
        }
    }
}

/// Applies the result of a single-item availability check to `item`, per
/// `catalog-availability-flag`'s single-item check requirement.
///
/// - `Ok`: replaces `item`'s fields with the fresh data, sets `is_available =
///   true`, and records `checked_at`.
/// - `Err` with kind `NotFound`: leaves other fields as-is, sets `is_available
///   = false`, and records `checked_at`.
/// - Any other `Err` (network, session, or otherwise): leaves `item` and its
///   flag entirely unchanged — a transient failure is not evidence of removal.
fn apply_check_result(item: &mut LibraryItem, result: Result<LibraryItem, LibraryServiceError>,
                      checked_at: std::time::SystemTime) {
    match result {
        Ok(fresh) => {
            // Preserve identity/collection-membership fields from the existing
            // item rather than trusting the single-item response
            // for them: a per-item availability re-check should
            // only refresh display data, and blindly overwriting
            // `order_product_id`/`product_id` here has been observed to
            // silently corrupt `collection_member_id` lookups for the checked
            // item (its collection membership stops resolving) when
            // the single-item endpoint's response doesn't carry the
            // exact same id values the list fetch populated.
            let id = Arc::clone(&item.id);
            let numeric_id = item.numeric_id;
            let order_product_id = item.order_product_id;
            let product_id = item.product_id;
            // `downloaded` is a local-only fact the server response knows
            // nothing about - the single-item endpoint always
            // reports it `false`. Preserve each file's existing
            // flag by id so a re-check (or the disk-presence
            // verification racing it, see
            // `verify-downloaded-status-against-disk`) doesn't get its
            // correction wiped the moment this slower network check resolves.
            let downloaded_by_id: HashMap<Arc<str>, bool> =
                item.files
                    .iter()
                    .map(|f| (Arc::clone(&f.id), f.downloaded))
                    .collect();
            *item = fresh;
            item.id = id;
            item.numeric_id = numeric_id;
            item.order_product_id = order_product_id;
            item.product_id = product_id;
            for file in &mut item.files {
                if let Some(&downloaded) = downloaded_by_id.get(&file.id) {
                    file.downloaded = downloaded;
                }
            }
            item.recompute_status();
            item.is_available = true;
            item.availability_last_checked = Some(checked_at);
        }
        Err(e) if e.kind == LibraryServiceErrorKind::NotFound => {
            item.is_available = false;
            item.availability_last_checked = Some(checked_at);
        }
        Err(_) => {}
    }
}

/// Returns `true` if `id` should be pushed onto the check queue: not already
/// queued and not already in flight.
fn should_enqueue_check(check_queue: &VecDeque<Arc<str>>, checking_items: &HashSet<Arc<str>>,
                        id: &Arc<str>)
                        -> bool {
    !checking_items.contains(id) && !check_queue.contains(id)
}

/// Returns how many more thumbnail fetches and/or file downloads can be
/// dispatched without exceeding `max_concurrent_downloads`, given the current
/// count of each already in flight.
///
/// Thumbnail and download slots share one aggregate limit, so this always
/// checks their sum rather than either count individually.
fn remaining_slots(max_concurrent_downloads: usize, active_thumbnail_fetches: usize,
                   active_downloads: usize)
                   -> usize {
    max_concurrent_downloads.saturating_sub(active_thumbnail_fetches + active_downloads)
}

/// Returns `true` if a check-batch request (manual or automatic) should be
/// suppressed because a batch was already enqueued too recently, per
/// `ITEM_CHECK_BATCH_COOLDOWN_SECS`. `None` (no prior batch recorded) never
/// blocks a request.
fn check_batch_cooldown_active(last_batch_secs: Option<u64>, now_secs: u64) -> bool {
    let Some(last) = last_batch_secs
    else {
        return false;
    };
    now_secs.saturating_sub(last) < crate::data::constants::ITEM_CHECK_BATCH_COOLDOWN_SECS
}

/// Selects up to `limit` catalog item ids overdue for an availability check
/// (per [`item_check_due`]), oldest-checked first — never-checked items
/// (`None`) sort first as maximally overdue, per `Option`'s `Ord`
/// implementation (`None < Some(_)`).
fn select_check_batch(catalog: &[LibraryItem], limit: usize) -> Vec<Arc<str>> {
    let mut candidates: Vec<&LibraryItem> =
        catalog.iter()
               .filter(|item| item_check_due(item.availability_last_checked))
               .collect();
    candidates.sort_by_key(|item| item.availability_last_checked);
    candidates.into_iter()
              .take(limit)
              .map(|item| Arc::clone(&item.id))
              .collect()
}

/// Merges a partial date-filtered fetch's `partial` results into `existing`
/// additively, per `catalog-availability-flag`'s partial-fetch requirement:
/// items present in `partial` are appended (if new) or have their fields
/// refreshed and `is_available` set to `true` (if already present); items
/// absent from `partial` are left completely untouched — a partial response
/// is never a complete listing, so absence from it means nothing.
fn merge_partial_fetch(mut existing: Vec<LibraryItem>, partial: Vec<LibraryItem>)
                       -> Vec<LibraryItem> {
    for mut item in partial {
        item.is_available = true;
        // Same `product_id` fallback as `reconcile_catalog`: a republished
        // title's `id` can change between fetches, so matching on `id` alone
        // would append a duplicate instead of refreshing the existing entry.
        let existing_match =
            existing.iter_mut()
                    .find(|e| {
                        e.id == item.id || (item.product_id != 0 && e.product_id == item.product_id)
                    });
        if let Some(existing_item) = existing_match {
            *existing_item = item;
        }
        else {
            existing.push(item);
        }
    }
    existing
}

/// Derives the `since` timestamp for a partial date-filtered fetch: the most
/// recent `date_updated` (falling back to `date_added`) across `catalog`,
/// formatted as RFC 3339. Returns `None` if the catalog is empty or every
/// item has neither timestamp populated.
fn partial_fetch_since(catalog: &[LibraryItem]) -> Option<String> {
    let max_epoch = catalog.iter()
                           .filter_map(|item| item.date_updated.or(item.date_added))
                           .max()?;
    crate::util::datetime::epoch_to_rfc3339(max_epoch)
}

/// Determines whether a startup remote-vs-cached item count mismatch should
/// be resolved with a cheaper additive partial fetch, per
/// `catalog-auto-load-policy`'s startup count-based decision.
///
/// `true` only for a growth-only mismatch (`remote_count > cached_count`) —
/// a decrease, or any mismatch a pure addition can't explain, always
/// requires a full reconciliation fetch, since only that can identify which
/// item is missing.
fn should_attempt_partial_fetch(remote_count: usize, cached_count: usize) -> bool {
    remote_count > cached_count
}

// ── LibraryController
// ─────────────────────────────────────────────────────────

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
}

/// A Zip content preview popover open for one file row within a detail
/// tab's item tier (see `zip-content-preview`).
#[derive(Clone)]
struct ZipPreviewState {
    /// Catalog entry id owning the previewed file row.
    entry_id:   Arc<str>,
    /// The file's position within the entry's `files` list.
    row_ix:     usize,
    /// Screen position the popover is anchored to, captured once when the
    /// preview opens — never updated by subsequent mouse movement, so the
    /// popover stays put while the pointer wanders (mirrors
    /// `CatalogView::popover_anchor_pos`).
    anchor_pos: Point<Pixels>,
    /// Whether a click has pinned this preview open, so it survives
    /// mouse-out until a second click or an explicit close.
    pinned:     bool,
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

impl std::fmt::Display for ThumbnailFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
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
    /// The Zip content preview popover currently open (hovered or pinned) in
    /// a detail tab's item tier, if any (see `zip-content-preview`). At most
    /// one is shown at a time. Ephemeral — never persisted, and cleared
    /// whenever the entry's detail tab is closed or reopened, same
    /// convention as `selected_item_file`.
    zip_preview:                   Option<ZipPreviewState>,
    /// Per-file Info column metadata (PDF page count, Zip file count, image
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
    /// [`crate::data::storage::StorageConfig`] and kept in sync with the
    /// Storage settings page via [`Self::set_max_concurrent_downloads`].
    max_concurrent_downloads:      usize,
    /// User-configured "Recently Updated" sidebar window, in days.
    /// Initialized from [`crate::data::storage::StorageConfig`] and kept in
    /// sync with the Storage settings page via
    /// [`Self::set_recently_updated_window_days`].
    recently_updated_window_days:  u32,
    /// A queuing action stashed by a `request_*` gating wrapper after it
    /// found free disk space insufficient, awaiting the user's response to
    /// the resulting [`LowDiskSpaceWarning`]. Consumed by
    /// [`Self::confirm_pending_download`] or cleared by
    /// [`Self::cancel_pending_download`].
    pending_download:              Option<PendingDownloadRequest>,
}

/// A user-initiated download-queuing action stashed while a
/// [`LowDiskSpaceWarning`] confirmation is pending.
///
/// One variant per `request_*` gating wrapper — see `LibraryController`'s
/// `request_download`, `request_item_download`,
/// `request_download_all_for_collection`, and
/// `request_download_all_for_publisher`.
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
}

impl LibraryController {
    /// Creates a controller and immediately schedules catalog loading on a
    /// background thread.
    ///
    /// The controller starts in the `Loading` pane state with an empty catalog.
    /// When the background fetch completes, [`apply_load_result`] is called
    /// and [`LibraryChanged`] emitted.
    ///
    /// # Panics
    ///
    /// Does not panic; service errors are reflected in [`pane_state`].
    pub fn new(service: Box<dyn LibraryService>,
               collections_service: Box<dyn CollectionsService>,
               activity: Entity<ActivityController>, cx: &mut Context<Self>)
               -> Self {
        let vm = LibraryViewModel::new(service);
        let prefs = crate::data::ui_preferences::UiPreferences::load();
        let view_prefs = crate::data::file_openers::CatalogViewPrefs::load();
        let storage = StorageConfig::load();

        let mut ctrl =
            Self { vm,
                   activity,
                   catalog: Vec::new(),
                   filter: view_prefs.to_filter(),
                   search_query: String::new(),
                   sort: view_prefs.to_sort(),
                   sort_direction: SortDirection::default(),
                   collection_sort: prefs.collection_sort(),
                   collection_sort_direction: prefs.collection_sort_direction(),
                   grouped: view_prefs.grouped.unwrap_or(false),
                   presentation: view_prefs.to_presentation(),
                   selection: Selection::default(),
                   section_counts: SectionCounts::default(),
                   publishers: Vec::new(),
                   collections: Vec::new(),
                   collections_loaded: false,
                   collections_service: Arc::from(collections_service),
                   collection_members: HashSet::new(),
                   thumbnail_queue: VecDeque::new(),
                   active_thumbnail_fetches: 0,
                   thumbnail_activity_id: None,
                   thumbnail_processed: 0,
                   thumbnail_refresh_active: false,
                   thumbnail_refresh_pending: HashSet::new(),
                   thumbnail_refresh_succeeded: 0,
                   thumbnail_refresh_failed: 0,
                   catalog_loading: true,
                   load_generation: 0,
                   catalog_sync_in_flight: false,
                   pending_reload: false,
                   network_monitor: Arc::new(NetworkMonitor::new()),
                   items_cache: None,
                   publisher_search_open: false,
                   publisher_search_query: String::new(),
                   collection_search_open: false,
                   collection_search_query: String::new(),
                   detail_panel_width: crate::data::constants::DETAIL_PANEL_DEFAULT_WIDTH,
                   entry_bounds: HashMap::new(),
                   selected_item_file: HashMap::new(),
                   other_details_open: HashMap::new(),
                   file_other_details_open: HashMap::new(),
                   zip_preview: None,
                   file_info_cache: HashMap::new(),
                   checking_items: HashSet::new(),
                   verifying_downloads: HashSet::new(),
                   check_queue: VecDeque::new(),
                   download_queue: VecDeque::new(),
                   active_downloads: 0,
                   download_cancel_flags: HashMap::new(),
                   download_activity_ids: HashMap::new(),
                   max_concurrent_downloads: storage.max_concurrent_downloads(),
                   recently_updated_window_days: storage.recently_updated_window_days(),
                   pending_download: None };
        ctrl.start_load(cx);
        ctrl.start_periodic_check_batch_timer(cx);
        ctrl.start_periodic_catalog_refresh_timer(cx);
        ctrl
    }

    /// Starts a background loop that calls
    /// [`Self::request_check_batch`] every `ITEM_CHECK_BATCH_TIMER_SECS`,
    /// for the lifetime of the controller.
    ///
    /// The timer interval is independent of `ITEM_CHECK_BATCH_COOLDOWN_SECS`
    /// — this only decides how often the loop *asks*; `request_check_batch`
    /// applies the real cooldown gate, so most wake-ups when nothing is due
    /// are a cheap no-op (a metadata read, no network call). The loop exits
    /// once the controller entity is dropped (`this.update` starts failing).
    fn start_periodic_check_batch_timer(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, async_cx| {
              loop {
                  async_cx
                    .background_executor()
                    .timer(std::time::Duration::from_secs(
                        crate::data::constants::ITEM_CHECK_BATCH_TIMER_SECS,
                    ))
                    .await;
                  let alive = this.update(async_cx, |ctrl, cx| ctrl.request_check_batch(cx))
                                  .is_ok();
                  if !alive {
                      break;
                  }
              }
          })
          .detach();
    }

    /// Starts a background loop that re-runs the catalog staleness check
    /// every `CATALOG_REFRESH_TIMER_INTERVAL_SECS`, independent of the
    /// startup sequence, for the lifetime of the controller — a
    /// long-running session (never restarted) still gets a refresh once its
    /// staleness threshold elapses, per
    /// `rust-resource-refresh-scheduling`'s recurring-timer requirement.
    ///
    /// Calls the same non-forced [`Self::start_load`] the startup sequence
    /// uses: `catalog_sync_in_flight` (see `start_load_inner`) makes an
    /// overlapping wake-up a no-op, and the existing auto-load-policy
    /// staleness check inside `start_load_inner` decides whether this
    /// wake-up actually triggers a fetch or is itself a no-op. The loop
    /// exits once the controller entity is dropped.
    fn start_periodic_catalog_refresh_timer(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, async_cx| {
              loop {
                  async_cx
                    .background_executor()
                    .timer(std::time::Duration::from_secs(
                        crate::data::constants::CATALOG_REFRESH_TIMER_INTERVAL_SECS,
                    ))
                    .await;
                  let alive = this.update(async_cx, |ctrl, cx| ctrl.start_load(cx))
                                  .is_ok();
                  if !alive {
                      break;
                  }
              }
          })
          .detach();
    }

    /// Spawns a background task to load collections and the catalog — from
    /// cache, then optionally from the live API.
    ///
    /// Progress is reported through a single activity item whose label
    /// advances through each stage in turn: collections, the cache-freshness
    /// count check, then one update per catalog page as it arrives.
    ///
    /// When `force_reload` is true the auto-load policy is bypassed and a full
    /// live fetch always runs. When false, the fetch is skipped if the
    /// cache is non-empty and was written within the last 7 days.
    ///
    /// Pages are delivered via an mpsc channel so each page triggers a UI
    /// update before the next page arrives.
    fn start_load(&mut self, cx: &mut Context<Self>) {
        self.start_load_inner(cx, false);
    }

    /// Verifies every catalog item's files against disk and applies any
    /// resulting `downloaded`/`status` changes back to the catalog, per
    /// `verify-downloaded-status-against-disk`.
    ///
    /// Runs as one batched background-executor pass rather than a per-item
    /// spawn, matching `save_catalog_cache`'s single-round-trip shape —
    /// avoids spawning hundreds of tiny tasks for a large library. Marks
    /// every item as verifying for the duration of the pass (see
    /// [`Self::is_verifying_downloads`]) so catalog rendering can show a
    /// pending indicator; catalog/section-count application to the entry
    /// itself is skipped if nothing changed.
    fn verify_catalog_downloads(&mut self, cx: &mut Context<Self>) {
        let catalog = self.catalog.clone();
        let ids: HashSet<Arc<str>> = catalog.iter().map(|i| Arc::clone(&i.id)).collect();
        self.verifying_downloads.extend(ids.iter().cloned());
        cx.emit(LibraryChanged);

        cx.spawn(async move |this, async_cx| {
              let (catalog, changed) = async_cx.background_executor()
                                               .spawn(async move {
                                                   let storage = StorageConfig::load();
                                                   let mut catalog = catalog;
                                                   let mut changed = false;
                                                   for item in &mut catalog {
                                                       changed |=
                                                           verify_item_downloads(item, &storage);
                                                   }
                                                   (catalog, changed)
                                               })
                                               .await;

              this.update(async_cx, |ctrl, cx| {
                      for id in &ids {
                          ctrl.verifying_downloads.remove(id);
                      }
                      if changed {
                          ctrl.catalog = catalog;
                          ctrl.section_counts = section_counts(&ctrl.catalog,
                                                               now_secs(),
                                                               ctrl.recently_updated_window_days);
                          ctrl.invalidate_cache();
                      }
                      cx.emit(LibraryChanged);
                  })
                  .ok();
          })
          .detach();
    }

    /// Clears `catalog_sync_in_flight` and, if a load was requested while
    /// this sync was running (see `start_load_inner`'s single-flight guard),
    /// immediately starts a fresh one instead of leaving it dropped.
    ///
    /// Every exit path of `start_load_inner`'s background task must call
    /// this exactly once instead of setting `catalog_sync_in_flight = false`
    /// directly, so a queued follow-up (e.g. from `replace_service`) is
    /// never silently lost.
    fn finish_catalog_sync(&mut self, cx: &mut Context<Self>) {
        self.catalog_sync_in_flight = false;
        if std::mem::take(&mut self.pending_reload) {
            self.start_load(cx);
        }
    }

    fn start_load_inner(&mut self, cx: &mut Context<Self>, force_reload: bool) {
        // Serial catalog-sync dispatch: never let a second catalog-sync task
        // start while one is already in flight, per
        // `rust-resource-work-queue-topology`. Unlike `load_generation` below
        // (which discards a superseded load's *results*), this stops a
        // redundant load from starting — and therefore from making a
        // redundant remote request — at all.
        //
        // A caller that arrives here while a sync is already running (e.g.
        // `replace_service` clearing the catalog for a newly authenticated
        // session while the prior unauthenticated attempt is still failing
        // out) still needs its load to happen eventually — dropping it
        // silently would leave a just-cleared catalog empty forever. Record
        // it as pending instead; [`Self::finish_catalog_sync`] starts a
        // follow-up load once the in-flight one completes.
        if self.catalog_sync_in_flight {
            tracing::debug!("catalog sync already in flight; queuing a follow-up reload once it completes");
            self.pending_reload = true;
            return;
        }
        self.catalog_sync_in_flight = true;

        self.load_generation += 1;
        let generation = self.load_generation;
        let service_arc = self.vm.service_arc();
        let collections_service = Arc::clone(&self.collections_service);
        let weak_activity = self.activity.downgrade();
        let storage_root = cache_dir();
        let save_root = storage_root.clone();
        let network_monitor = Arc::clone(&self.network_monitor);

        cx.spawn(async move |this, async_cx| {
            // Single activity item for the whole load. Its label is updated in
            // place as the load moves through stages (collections, count
            // check, live fetch, page-by-page progress) rather than starting
            // a new activity per stage — the stages are not independent
            // operations, they are phases of one catalog load.
            let activity_id = weak_activity
                .update(async_cx, |a, cx| a.start(&t!("activity.loading_library"), None, cx))
                .unwrap_or(0);

            // ── Pre-populate catalog from disk cache ────────────────────────────
            // Runs first, ahead of the collections stage below: it's a local disk
            // read with no network dependency, so any catalog just cleared by this
            // load's caller (e.g. `replace_service` swapping in an authenticated
            // service, or `clear_and_reload`) reappears immediately instead of
            // sitting empty for however long the collections network call below
            // takes to resolve — see `catalog-live-merge`.
            let cache_root = storage_root.clone();
            let meta_root = cache_root.clone();

            let cached = async_cx
                .background_executor()
                .spawn(async move { load_catalog_cache(&cache_root).map(dedupe_by_product_id) })
                .await;
            if let Some(items) = cached.as_ref() {
                this.update(async_cx, |ctrl, cx| {
                    if ctrl.load_generation != generation {
                        return; // superseded by a newer load (e.g. cache-clear reload)
                    }
                    if force_reload {
                        ctrl.catalog.clear();
                    }
                    // Only pre-populate when there's nothing in memory yet (fresh
                    // controller, or just cleared above) — `append_catalog_page`
                    // does a plain `extend` with no dedup, so calling it again on an
                    // already-populated catalog (e.g. a periodic refresh tick after
                    // the initial load completed, or any later call to
                    // `start_load`/`start_load_inner`) would duplicate every item.
                    if ctrl.catalog.is_empty() {
                        ctrl.append_catalog_page(items.clone(), cx);
                    }
                })
                .ok();
            }

            // ── Stage: collections ──────────────────────────────────────────────
            weak_activity
                .update(async_cx, |a, cx| {
                    a.update_label(activity_id, t!("activity.loading_library_collections"), cx)
                })
                .ok();

            let col_cache_root = storage_root.clone();
            let cached_collections = async_cx
                .background_executor()
                .spawn(async move { load_collections_cache(&col_cache_root) })
                .await;
            if let Some(entries) = cached_collections {
                this.update(async_cx, |ctrl, cx| {
                    if ctrl.load_generation != generation {
                        return; // superseded by a newer load (e.g. cache-clear reload)
                    }
                    ctrl.apply_collections(entries, cx);
                })
                .ok();
            }

            let col_save_root = storage_root.clone();
            let live_collections = async_cx
                .background_executor()
                .spawn(async move { collections_service.list_collections() })
                .await;
            match live_collections {
                Ok(entries) => {
                    let to_save = entries.clone();
                    async_cx
                        .background_executor()
                        .spawn(async move {
                            if let Err(e) = save_collections_cache(&col_save_root, &to_save) {
                                tracing::warn!(error = %e, "failed to save collections cache");
                            }
                        })
                        .await;
                    this.update(async_cx, |ctrl, cx| {
                        if ctrl.load_generation != generation {
                            return; // superseded by a newer load
                        }
                        ctrl.apply_collections(entries, cx);
                    })
                    .ok();
                }
                Err(e) => {
                    // Non-fatal: collections and catalog are independent
                    // datasets sharing one activity item, so a collections
                    // failure does not abort the catalog stages below.
                    if e.kind == CollectionsServiceErrorKind::Session
                    {
                        // WARN, not debug: the default log filter is `warn` (see
                        // `logging::init`), and "not signed in yet" is exactly the kind of
                        // routine-but-worth-knowing condition a user reading logs should be
                        // able to see without setting RUST_LOG.
                        tracing::warn!(error = %e,
                                      "collections load skipped: no authenticated session");
                    }
                    else {
                        tracing::warn!(error = %e, "collections load failed");
                    }
                }
            }

            // ── Auto-load policy ──────────────────────────────────────────────
            // Skip the live fetch when: the cache is non-empty, was written within
            // the last 7 days, and force_reload is false.
            if !force_reload && let Some(items) = cached.as_ref().filter(|items| !items.is_empty()) {
                        let meta = async_cx
                            .background_executor()
                            .spawn(async move { load_cache_metadata(&meta_root) })
                            .await;
                        let is_fresh = meta.as_ref().is_some_and(|m| !m.is_stale());

                        if is_fresh {
                            // Check remote count if the service supports it cheaply — this
                            // is a stage of the same load, surfaced via a label update on
                            // the existing activity rather than a separate item, so the
                            // user sees one operation moving through phases instead of a
                            // flurry of independent-looking activities.
                            weak_activity
                                .update(async_cx, |a, cx| {
                                    a.update_label(activity_id, t!("activity.loading_library_count"), cx)
                                })
                                .ok();
                            let svc = service_arc.clone();
                            let remote_count = async_cx
                                .background_executor()
                                .spawn(async move { svc.count_items() })
                                .await;
                            let count_matches = match remote_count {
                                Some(Ok(n)) => n == items.len(),
                                Some(Err(_)) => false, // count fetch failed; do live fetch
                                None => true, // count not supported; trust fresh cache
                            };
                            if count_matches {
                                tracing::debug!(
                                    cached = items.len(),
                                    "catalog auto-load: cache is fresh, skipping live fetch"
                                );
                                weak_activity
                                    .update(async_cx, |a, cx| a.complete(activity_id, cx))
                                    .ok();
                                this.update(async_cx, |ctrl, cx| {
                                    ctrl.finish_catalog_sync(cx);
                                    if ctrl.load_generation != generation {
                                        return; // superseded by a newer load
                                    }
                                    ctrl.catalog_loading = false;
                                    // This branch completes the load without ever calling
                                    // `set_catalog` — a restored `Publisher` filter that no
                                    // longer matches must be corrected here too, or it stays
                                    // applied (filtering the catalog to nothing) until a
                                    // forced reload happens to reach `set_catalog` instead.
                                    ctrl.revalidate_publisher_filter();
                                    cx.emit(LibraryChanged);
                                    // The skip-fetch path is exactly the case most likely to be
                                    // showing stale `downloaded` state indefinitely — that's the
                                    // whole reason the live fetch was skipped — so it needs
                                    // disk verification the most.
                                    ctrl.verify_catalog_downloads(cx);
                                })
                                .ok();
                                return;
                            }
                            else if let Some(Ok(remote_n)) = remote_count
                                    && should_attempt_partial_fetch(remote_n, items.len())
                                    && let Some(since) = partial_fetch_since(items)
                            {
                                // Growth-only mismatch: try a cheaper additive partial fetch
                                // before falling back to a full paginated one. A decrease (or
                                // a mismatch a pure addition can't explain) always falls
                                // through to the full fetch below, since only reconciliation
                                // can identify *which* item is missing.
                                weak_activity
                                    .update(async_cx, |a, cx| {
                                        a.update_label(
                                            activity_id,
                                            t!("activity.loading_library_partial"),
                                            cx,
                                        )
                                    })
                                    .ok();
                                let svc = service_arc.clone();
                                let partial = async_cx
                                    .background_executor()
                                    .spawn(async move {
                                        let mut items = Vec::new();
                                        let result = svc.list_items_updated_since(
                                            &since,
                                            &mut |page| items.extend(page),
                                        );
                                        result.map(|r| r.map(|()| items))
                                    })
                                    .await;
                                if let Some(Ok(partial_items)) = partial {
                                    let is_current = this
                                        .update(async_cx, |ctrl, _cx| {
                                            ctrl.load_generation == generation
                                        })
                                        .unwrap_or(false);
                                    if is_current {
                                        this.update(async_cx, |ctrl, cx| {
                                            ctrl.apply_partial_fetch(partial_items, cx);
                                            ctrl.verify_catalog_downloads(cx);
                                        })
                                        .ok();
                                        let items_to_save = this
                                            .update(async_cx, |ctrl, _cx| ctrl.catalog.clone())
                                            .unwrap_or_default();
                                        async_cx
                                            .background_executor()
                                            .spawn(async move {
                                                if let Err(e) = save_catalog_cache(
                                                    &save_root,
                                                    &items_to_save,
                                                ) {
                                                    tracing::warn!(error = %e, "failed to save catalog cache");
                                                }
                                            })
                                            .await;
                                        weak_activity
                                            .update(async_cx, |a, cx| a.complete(activity_id, cx))
                                            .ok();
                                        this.update(async_cx, |ctrl, cx| {
                                            ctrl.finish_catalog_sync(cx);
                                        })
                                        .ok();
                                        return;
                                    }
                                }
                                // `None` (unsupported), an error, or a superseded load: fall
                                // through to the full paginated fetch below.
                            }
                        }
            }

            // ── Fresh-install initialization ─────────────────────────────────────
            // No local catalog data was loaded from disk above: this is a fresh
            // install (or the cache was cleared/relocated), per
            // `rust-catalog-fresh-install-initialization`. Credentials are not
            // pre-checked here — `list_items_paged` below already returns a
            // `Session` error, handled as a quiet completion, when no
            // authenticated session exists yet, which this same code path relies
            // on regardless of fresh-install status.
            //
            // Issues a dedicated totals request (item count only — the API has no
            // aggregate-size endpoint; per-file `size`/`sizeMB` only exist within
            // an already-fetched item's file list) before any page of item data,
            // gated by a minimum interval so repeated fresh-install attempts (e.g.
            // relaunching shortly after a failed first attempt) don't hammer the
            // API with a totals request every time.
            let mut fresh_install_total: Option<usize> = None;
            let is_fresh_install = !force_reload && cached.as_ref().is_none_or(Vec::is_empty);
            if is_fresh_install {
                let meta_root = storage_root.clone();
                let last_request = async_cx
                    .background_executor()
                    .spawn(async move { load_cache_metadata(&meta_root) })
                    .await
                    .and_then(|m| m.last_fresh_install_request_secs);
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(std::time::Duration::ZERO)
                    .as_secs();
                let gated = fresh_install_request_gated(last_request, now_secs);
                let monitor_for_totals = network_monitor.clone();
                let endpoint_reachable = async_cx
                    .background_executor()
                    .spawn(async move {
                        monitor_for_totals.check_endpoint(crate::data::constants::DTRPG_API_HOST)
                    })
                    .await;
                if gated {
                    tracing::debug!(
                        "fresh-install initialization skipped: last request was within the \
                         minimum interval"
                    );
                } else if !endpoint_reachable {
                    tracing::debug!(
                        "fresh-install totals request skipped: DriveThruRPG API unreachable"
                    );
                } else {
                    let svc = service_arc.clone();
                    let cancel = std::sync::atomic::AtomicBool::new(false);
                    let config = retry::RetryConfig {
                        max_attempts: crate::data::constants::CATALOG_SYNC_MAX_ATTEMPTS,
                        base_secs: crate::data::constants::CATALOG_SYNC_RETRY_BASE_DELAY_SECS,
                        max_secs: crate::data::constants::CATALOG_SYNC_RETRY_MAX_DELAY_SECS,
                    };
                    let mut on_retry =
                        |attempt: u32, delay: std::time::Duration, e: &LibraryServiceError| {
                            tracing::debug!(
                                attempt,
                                delay_secs = delay.as_secs(),
                                reason = %e,
                                "fresh-install totals request retrying"
                            );
                        };
                    // `count_items()` returns `Option<Result<usize, _>>` — `None` means
                    // "count not supported by this implementation" (not an error, nothing to
                    // retry); folded into `Ok(None)` here so `retry_with_backoff` only sees a
                    // plain `Result` and only retries a genuine `Network`-kind failure.
                    let totals: Result<Option<usize>, LibraryServiceError> = async_cx
                        .background_executor()
                        .spawn(async move {
                            retry::retry_with_backoff(
                                config,
                                &cancel,
                                || match svc.count_items() {
                                    None => Ok(None),
                                    Some(Ok(n)) => Ok(Some(n)),
                                    Some(Err(e)) => Err(e),
                                },
                                |e: &LibraryServiceError| {
                                    e.kind == LibraryServiceErrorKind::Network
                                        || e.kind == LibraryServiceErrorKind::RateLimited
                                },
                                |e: &LibraryServiceError| e.retry_after,
                                Some(&mut on_retry),
                            )
                        })
                        .await;
                    match totals {
                        Ok(Some(count)) => {
                            tracing::debug!(total_items = count, "fresh-install: totals request");
                            fresh_install_total = Some(count);
                        }
                        Ok(None) => {}
                        Err(e) if e.kind == LibraryServiceErrorKind::Session => {
                            // Routine, not a failure: no authenticated session yet. WARN
                            // (not debug) so it's visible under the default `warn` log
                            // filter without setting RUST_LOG, matching the collections/
                            // catalog-fetch Session-error branches elsewhere in this
                            // function.
                            tracing::warn!(
                                error = %e,
                                "fresh-install initialization waiting for authenticated session"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "fresh-install totals request failed");
                        }
                    }
                    let request_root = storage_root.clone();
                    async_cx
                        .background_executor()
                        .spawn(async move {
                            if let Err(e) =
                                save_fresh_install_request_timestamp(&request_root, now_secs)
                            {
                                tracing::warn!(
                                    error = %e,
                                    "failed to save fresh-install request timestamp"
                                );
                            }
                        })
                        .await;
                }
            }

            // ── Stage: page-by-page fetch from API ──────────────────────────────
            // Query the network monitor before committing to the paginated fetch —
            // per `rust-resource-network-monitor`, a process needing a specific
            // endpoint stops rather than requesting when it's reported unreachable.
            let monitor_for_fetch = network_monitor.clone();
            let endpoint_reachable = async_cx
                .background_executor()
                .spawn(async move {
                    monitor_for_fetch.check_endpoint(crate::data::constants::DTRPG_API_HOST)
                })
                .await;
            if !endpoint_reachable {
                tracing::debug!("catalog fetch skipped: DriveThruRPG API unreachable");
                weak_activity.update(async_cx, |a, cx| a.complete(activity_id, cx)).ok();
                this.update(async_cx, |ctrl, cx| {
                    if ctrl.load_generation == generation {
                        ctrl.catalog_loading = false;
                        cx.emit(LibraryChanged);
                    }
                    ctrl.finish_catalog_sync(cx);
                })
                .ok();
                return;
            }

            // Move the shared activity into its next stage — no new item is
            // created, so a count check that ran above doesn't leave a stale
            // "getting count of items…" label behind. The per-page label set
            // below (`page N…`) replaces this as soon as the first page
            // arrives.
            weak_activity
                .update(async_cx, |a, cx| {
                    a.update_label(activity_id, t!("activity.loading_library"), cx)
                })
                .ok();

            // Two channels: one for page batches, one for the API-reported total, if any.
            let (tx, rx) = std::sync::mpsc::channel::<Vec<LibraryItem>>();
            let (total_tx, total_rx) = std::sync::mpsc::channel::<usize>();

            // Cloned before `service_arc` moves into the fetch spawn below — used to get
            // a live item count for progress seeding when this isn't a fresh install.
            let svc_for_count = service_arc.clone();

            // Run the paginated fetch on the background executor. Each page — and, when
            // the API reports one (via `links.last`), the estimated total — is sent
            // through its channel as it arrives; the result indicates overall success/failure.
            let fetch = async_cx
                .background_executor()
                .spawn(async move {
                    service_arc.list_items_paged(
                        &mut |page| { tx.send(page).ok(); },
                        Some(&mut |total| { total_tx.send(total).ok(); }),
                    )
                });

            // Seed the progress denominator, preferring a fresh-install totals
            // request (see above) since it reflects the remote count directly. For a
            // full fetch triggered for any other reason (stale cache, count mismatch,
            // force reload), a single best-effort `count_items()` call gets a live
            // remote count instead — the local cache's item count is exactly the value
            // already known (or suspected) to be wrong whenever this stage runs at all,
            // so seeding progress from it pins the bar near 100% after only a few pages
            // once the real catalog has grown since the cache was last saved. Only if
            // that live request also comes back empty does the stale cache count get
            // used, so the bar still starts determinate rather than not at all. The
            // real DriveThruRPG API never reports `links.last`, so `total_rx` below
            // essentially never fires in practice — this is the only correction this
            // estimate gets before the fetch completes.
            let live_count = if fresh_install_total.is_none() {
                async_cx.background_executor()
                        .spawn(async move { svc_for_count.count_items() })
                        .await
                        .and_then(Result::ok)
            } else {
                None
            };
            let mut estimated_total: Option<usize> =
                fresh_install_total.or(live_count).or_else(|| {
                    cached.as_ref().map(|items| items.len()).filter(|&n| n > 0)
                });
            if let Some(total) = estimated_total {
                weak_activity
                    .update(async_cx, |a, cx| a.update_progress(activity_id, 0.0, cx))
                    .ok();
                tracing::debug!(estimated_total = total, "catalog load: seeded total");
            }

            // Accumulate all live SDK pages into a local buffer so a *populated* cache
            // remains visible in the UI throughout the fetch instead of flashing down to
            // a single page (see `catalog-live-merge`). When there is nothing cached to
            // protect — first launch, or right after `clear_and_reload` — there is no
            // flash risk, so each page is appended to the visible catalog (and its
            // thumbnails enqueued) as it arrives instead of waiting for the entire fetch
            // to finish. This is what makes thumbnail loading observable/testable without
            // waiting through a full multi-page fetch.
            let catalog_was_empty = this
                .update(async_cx, |ctrl, _cx| ctrl.catalog.is_empty())
                .unwrap_or(false);

            let mut rx = Some(rx);
            let mut total_rx = Some(total_rx);
            let mut live_items: Vec<LibraryItem> = Vec::new();
            let mut page_num: u32 = 0;
            // Checkpointing state for `catalog-cache-checkpointing`: the accumulating
            // `live_items` buffer (not `self.catalog`, which isn't swapped in until the
            // full fetch completes) is periodically saved to disk so an interrupted load
            // still leaves a recent cache for the next startup. `checkpoint_task` is
            // awaited — never dropped while pending — before starting the next checkpoint
            // or the final post-fetch save, so writes never race each other or the final
            // save over the same `.tmp` file.
            let mut checkpoint_task: Option<Task<()>> = None;
            let mut last_checkpoint_page: u32 = 0;
            let mut last_checkpoint_at = std::time::Instant::now();
            loop {
                let Some(receiver) = rx.take() else { break; };

                // Non-blocking check for an API-reported total before processing the
                // next page, so the bar re-bases onto the real count as soon as it's
                // known without ever blocking page delivery.
                if let Some(total_receiver) = total_rx.take() {
                    match total_receiver.try_recv() {
                        Ok(total) => {
                            estimated_total = Some(total);
                            tracing::debug!(
                                estimated_total = total,
                                "catalog load: API-reported item count"
                            );
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            total_rx = Some(total_receiver);
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
                    }
                }

                let (msg, returned_rx) = async_cx
                    .background_executor()
                    .spawn(async move {
                        let msg = receiver.recv();
                        (msg, receiver)
                    })
                    .await;

                match msg {
                    Ok(items) => {
                        if catalog_was_empty {
                            let page = items.clone();
                            this.update(async_cx, |ctrl, cx| {
                                if ctrl.load_generation == generation {
                                    ctrl.append_catalog_page(page, cx);
                                }
                            })
                            .ok();
                        }
                        live_items.extend(items);
                        page_num += 1;

                        // Checkpoint the accumulating buffer every N pages, or after a
                        // minimum interval, whichever fires first — see
                        // `catalog-cache-checkpointing`.
                        let pages_since_checkpoint = page_num - last_checkpoint_page;
                        if pages_since_checkpoint >= CATALOG_CHECKPOINT_PAGE_INTERVAL
                           || last_checkpoint_at.elapsed()
                              >= std::time::Duration::from_secs(
                                  CATALOG_CHECKPOINT_MIN_INTERVAL_SECS,
                              )
                        {
                            if let Some(prev) = checkpoint_task.take() {
                                prev.await;
                            }
                            let snapshot = live_items.clone();
                            let checkpoint_root = save_root.clone();
                            checkpoint_task = Some(async_cx.background_executor().spawn(
                                async move {
                                    if let Err(e) = save_catalog_cache(&checkpoint_root, &snapshot)
                                    {
                                        tracing::warn!(
                                            error = %e,
                                            "failed to checkpoint catalog cache"
                                        );
                                    }
                                },
                            ));
                            last_checkpoint_page = page_num;
                            last_checkpoint_at = std::time::Instant::now();
                        }

                        // Always advance the label to the current page — the
                        // real DriveThruRPG API never reports `links.last`, so a
                        // percentage is usually unavailable; the page number is
                        // otherwise the only thing that shows the load is
                        // actually making progress rather than being frozen (see
                        // `activity_panel_view`'s indeterminate progress bar).
                        let label = t!("activity.loading_library_page", page = page_num).to_string();
                        weak_activity
                            .update(async_cx, |a, cx| a.update_label(activity_id, label, cx))
                            .ok();
                        // When a total estimate is available, also drive the
                        // progress bar's percentage from it.
                        if let Some(total) = estimated_total.filter(|&t| t > 0) {
                            let progress = (live_items.len() as f32 / total as f32).min(1.0);
                            weak_activity
                                .update(async_cx, |a, cx| a.update_progress(activity_id, progress, cx))
                                .ok();
                        }
                        rx = Some(returned_rx);
                    }
                    Err(_) => break, // sender dropped — all pages have been sent
                }
            }

            // Wait for the fetch task to complete and surface any error.
            // On success: swap the catalog atomically with the full live dataset — a no-op
            //   diff when `catalog_was_empty` already appended every page incrementally
            //   above, but still required to guarantee the final state is exactly the live
            //   dataset (e.g. if a page arrived out of order or a duplicate id was
            //   deduped differently) — then save to disk and dismiss the loading indicator.
            // On error: leave the cached catalog unchanged in memory, but still clear
            //   `catalog_loading` — otherwise a failed fetch with nothing cached leaves the
            //   catalog view's spinner (`catalog_loading && item_count == 0`) spinning
            //   forever instead of falling through to the empty state.
            match fetch.await {
                Ok(()) => {
                    let is_current = this
                        .update(async_cx, |ctrl, _cx| ctrl.load_generation == generation)
                        .unwrap_or(false);
                    if !is_current {
                        // A newer load (e.g. triggered by a cache clear) has already
                        // taken over; discard this stale result instead of clobbering it.
                        weak_activity.update(async_cx, |a, cx| a.complete(activity_id, cx)).ok();
                        this.update(async_cx, |ctrl, cx| {
                            ctrl.finish_catalog_sync(cx);
                        })
                        .ok();
                        return;
                    }
                    this.update(async_cx, |ctrl, cx| {
                        ctrl.set_catalog(live_items, !catalog_was_empty, cx);
                        ctrl.verify_catalog_downloads(cx);
                    }).ok();
                    // Wait for any in-flight checkpoint write to finish before writing the
                    // final cache — both write `save_root`'s `.tmp` file, so they must never
                    // run concurrently.
                    if let Some(prev) = checkpoint_task.take() {
                        prev.await;
                    }
                    let items_to_save = this
                        .update(async_cx, |ctrl, _cx| ctrl.catalog.clone())
                        .unwrap_or_default();
                    async_cx
                        .background_executor()
                        .spawn(async move {
                            if let Err(e) = save_catalog_cache(&save_root, &items_to_save) {
                                tracing::warn!(error = %e, "failed to save catalog cache");
                            }
                        })
                        .await;
                    weak_activity.update(async_cx, |a, cx| a.complete(activity_id, cx)).ok();
                }
                Err(e) => {
                    // Session errors are expected when starting before auth completes;
                    // treat them as a quiet completion rather than a user-facing alert.
                    // Network and other errors are genuine failures worth surfacing.
                    if e.kind == LibraryServiceErrorKind::Session {
                        // WARN, not debug: see the matching comment on the collections
                        // load's Session-error branch above.
                        tracing::warn!(error = %e, "catalog load skipped: no authenticated session");
                        weak_activity.update(async_cx, |a, cx| a.complete(activity_id, cx)).ok();
                    } else {
                        tracing::error!(error = %e, backtrace = %app_backtrace(), "catalog load failed");
                        let detail = e.panel_detail();
                        weak_activity.update(async_cx, |a, cx| a.error(activity_id, detail, cx)).ok();
                    }
                    this.update(async_cx, |ctrl, cx| {
                        if ctrl.load_generation != generation {
                            return; // superseded by a newer load
                        }
                        ctrl.catalog_loading = false;
                        cx.emit(LibraryChanged);
                    })
                    .ok();
                }
            }
            this.update(async_cx, |ctrl, cx| {
                ctrl.finish_catalog_sync(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Atomically replaces the catalog with a complete dataset and recomputes
    /// derived state.
    ///
    /// Used after all live SDK pages have been collected so the UI transitions
    /// from cached data to the full live set in one render pass.
    /// `reconcile` selects between two behaviors, per `catalog-live-data-swap`:
    /// `false` replaces `self.catalog` outright with `items` (used when there
    /// was no local baseline to reconcile against — first launch or after
    /// `clear_and_reload`); `true` merges `items` into the existing catalog
    /// by id via `reconcile_catalog` (items missing from `items` are kept,
    /// flagged `is_available = false`, rather than dropped).
    fn set_catalog(&mut self, items: Vec<LibraryItem>, reconcile: bool, cx: &mut Context<Self>) {
        self.enqueue_thumbnails(&items, cx);
        self.catalog = if reconcile {
            reconcile_catalog(std::mem::take(&mut self.catalog), items)
        }
        else {
            items
        };
        self.catalog_loading = false;
        self.section_counts =
            section_counts(&self.catalog, now_secs(), self.recently_updated_window_days);
        self.publishers = publisher_entries(&self.catalog);
        self.revalidate_publisher_filter();
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Resets `self.filter` to [`SidebarFilter::AllTitles`] if it is a
    /// `Publisher` filter whose publisher is not present in
    /// `known_publishers`.
    ///
    /// Called once the catalog-load's item list is available, since a
    /// `Publisher` filter restored from disk at [`Self::new`] (before any
    /// items have loaded) can't be validated synchronously. Does not save
    /// the reset — the persisted preference is left untouched, so a launch
    /// where the publisher reappears restores it.
    pub fn validate_publisher_filter(&mut self, known_publishers: &[Arc<str>]) {
        if let SidebarFilter::Publisher(name) = &self.filter
           && !known_publishers.contains(name)
        {
            self.filter = SidebarFilter::AllTitles;
        }
    }

    /// Convenience wrapper around [`Self::validate_publisher_filter`] using
    /// the just-recomputed `self.publishers` as the known set.
    ///
    /// Every catalog-load completion path that (re)computes `self.publishers`
    /// must call this — not just the full live-fetch path
    /// ([`Self::set_catalog`]) — since the cache-fresh skip-fetch branch in
    /// [`Self::start_load_inner`] and [`Self::apply_partial_fetch`] both also
    /// complete a load without ever calling `set_catalog`. Missing one of
    /// these left a restored `Publisher` filter that no longer matched
    /// permanently un-validated on an ordinary cache-hit cold start.
    fn revalidate_publisher_filter(&mut self) {
        let known_publishers: Vec<Arc<str>> = self.publishers
                                                  .iter()
                                                  .map(|p| Arc::clone(&p.name))
                                                  .collect();
        self.validate_publisher_filter(&known_publishers);
    }

    /// Merges a partial date-filtered fetch's `partial` results into the
    /// catalog additively (see [`merge_partial_fetch`]) and performs the
    /// same finalization as [`Self::set_catalog`] (thumbnail enqueue,
    /// derived state recompute, cache invalidation, `LibraryChanged`).
    ///
    /// Unlike a full reconcile, this never flags any existing item
    /// unavailable — a partial response is never a complete listing.
    fn apply_partial_fetch(&mut self, partial: Vec<LibraryItem>, cx: &mut Context<Self>) {
        self.enqueue_thumbnails(&partial, cx);
        self.catalog = merge_partial_fetch(std::mem::take(&mut self.catalog), partial);
        self.catalog_loading = false;
        self.section_counts =
            section_counts(&self.catalog, now_secs(), self.recently_updated_window_days);
        self.publishers = publisher_entries(&self.catalog);
        self.revalidate_publisher_filter();
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Appends a page of items received incrementally from the background load
    /// task.
    ///
    /// Used only for the initial disk-cache pre-population, not for live SDK
    /// pages.
    fn append_catalog_page(&mut self, items: Vec<LibraryItem>, cx: &mut Context<Self>) {
        self.enqueue_thumbnails(&items, cx);
        self.catalog.extend(items);
        self.section_counts =
            section_counts(&self.catalog, now_secs(), self.recently_updated_window_days);
        self.publishers = publisher_entries(&self.catalog);
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Spawns a background task to fetch product lists from the API and apply
    /// them.
    pub fn load_collections(&mut self, cx: &mut Context<Self>) {
        let collections_service = Arc::clone(&self.collections_service);
        let cache_root = cache_dir();
        let weak_activity = self.activity.downgrade();
        let activity_id = self.activity.update(cx, |a, cx| {
                                           a.start(&t!("activity.loading_collections"), None, cx)
                                       });
        tracing::debug!("load_collections: starting collections fetch");
        cx.spawn(async move |this, async_cx| {
              let result = async_cx.background_executor()
                                   .spawn(async move { collections_service.list_collections() })
                                   .await;
              match result {
                  Ok(entries) => {
                      tracing::debug!(count = entries.len(),
                                      "load_collections: fetched {} entries",
                                      entries.len());
                      let to_save = entries.clone();
                      let save_root = cache_root.clone();
                      async_cx
                        .background_executor()
                        .spawn(async move {
                            if let Err(e) = save_collections_cache(&save_root, &to_save) {
                                tracing::warn!(error = %e, "failed to save collections cache");
                            }
                        })
                        .await;
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.apply_collections(entries, cx);
                          })
                          .ok();
                      weak_activity.update(async_cx, |a, cx| a.complete(activity_id, cx))
                                   .ok();
                  }
                  Err(e) => {
                      // Session errors are expected when starting before auth
                      // completes
                      // (see `start_load_inner`'s matching treatment for the
                      // catalog fetch); quietly complete
                      // rather than surfacing a user-facing error.
                      if e.kind == CollectionsServiceErrorKind::Session {
                          tracing::warn!(error = %e,
                                      "collections load skipped: no authenticated session");
                          weak_activity.update(async_cx, |a, cx| a.complete(activity_id, cx))
                                       .ok();
                      }
                      else {
                          tracing::warn!(error = %e, "collections load failed");
                          weak_activity.update(async_cx, |a, cx| {
                                           a.error(activity_id, e.to_string(), cx)
                                       })
                                       .ok();
                      }
                  }
              }
          })
          .detach();
    }

    /// Stores the fetched collections and emits a change event.
    fn apply_collections(&mut self, mut collections: Vec<CollectionEntry>, cx: &mut Context<Self>) {
        let catalog_ids = self.catalog_ids();
        sort_collections(&mut collections,
                         self.collection_sort,
                         self.collection_sort_direction,
                         &catalog_ids);
        self.collections = collections;
        self.collections_loaded = true;
        // Refresh collection_members if the current filter is a Collection
        // filter, in case the filter was set before the collections
        // loaded.
        if let SidebarFilter::Collection(id, _) = &self.filter {
            let id = *id;
            self.collection_members = self.collections
                                          .iter()
                                          .find(|c| c.id == id)
                                          .map(|c| c.member_ids.iter().copied().collect())
                                          .unwrap_or_default();
        }
        cx.emit(LibraryChanged);
    }

    /// Returns the current pane state from the service layer.
    pub fn pane_state(&self) -> &LibraryPaneState {
        self.vm.pane_state()
    }

    /// Replaces the backing services and triggers a fresh catalog and
    /// collections load.
    ///
    /// Clears the activity panel so stale error messages from the previous
    /// (unauthenticated) service do not persist after sign-in.
    ///
    /// Sets `catalog_loading` before clearing the catalog so the catalog view
    /// shows its loading spinner instead of the "library empty" state for the
    /// gap between this clear and `start_load` repopulating it — otherwise a
    /// catalog already populated from disk cache (e.g. during startup's
    /// silent re-authentication) flashes empty before reappearing.
    pub fn replace_service(&mut self, service: Box<dyn LibraryService>,
                           collections_service: Box<dyn CollectionsService>,
                           cx: &mut Context<Self>) {
        tracing::debug!("replace_service: installing authenticated services");
        self.vm.replace_service(service);
        self.collections_service = Arc::from(collections_service);
        self.catalog_loading = true;
        self.catalog.clear();
        self.section_counts =
            section_counts(&self.catalog, now_secs(), self.recently_updated_window_days);
        self.publishers = publisher_entries(&self.catalog);
        self.collections.clear();
        self.collections_loaded = false;
        self.collection_members.clear();
        self.selection = Selection::default();
        self.invalidate_cache();
        self.activity.update(cx, |a, cx| a.clear(cx));
        cx.emit(LibraryChanged);
        // Collections are loaded as the first stage of `start_load`, sharing
        // its single activity item rather than starting a separate one.
        self.start_load(cx);
    }

    /// Starts a background task to create a new collection with the given name.
    ///
    /// Tracks progress in the activity panel. On success the new entry is
    /// appended and [`LibraryChanged`] is emitted. On failure
    /// [`CollectionCreateFailed`] is emitted so the window can push an
    /// error notification.
    pub fn create_collection(&mut self, name: String, cx: &mut Context<Self>) {
        let label = t!("activity.creating_collection", name = name);
        let activity_id = self.activity.update(cx, |a, cx| a.start(&label, None, cx));

        let collections_service = Arc::clone(&self.collections_service);
        cx.spawn(async move |this, async_cx| {
              let result =
                  async_cx.background_executor()
                          .spawn(async move { collections_service.create_collection(&name) })
                          .await;

              match result {
                  Ok(entry) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.collections.push(entry);
                              ctrl.activity
                                  .update(cx, |a, cx| a.complete(activity_id, cx));
                              cx.emit(LibraryChanged);
                          })
                          .ok();
                  }
                  Err(e) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.activity.update(cx, |a, cx| {
                                               a.error(activity_id, e.to_string(), cx);
                                           });
                              cx.emit(CollectionCreateFailed { message: e.message.clone(), });
                          })
                          .ok();
                  }
              }
          })
          .detach();
    }

    /// Creates a new collection with the given name and, once the create call
    /// succeeds, immediately adds `item_id` as a member of it.
    ///
    /// `item_id` drives local membership tracking/matching (the optimistic
    /// `member_ids` update); `product_id` is the catalog product id sent on
    /// the network add call. See [`Self::add_item_to_collection`].
    ///
    /// Used by the "New collection…" affordance in the Manage Collections
    /// dialog, so a single user action both creates the collection and adds
    /// the current item to it. On create failure, only
    /// [`CollectionCreateFailed`] is emitted — no add is attempted.
    pub fn create_collection_and_add_member(&mut self, name: String, item_id: u64,
                                            product_id: u64, cx: &mut Context<Self>) {
        let label = t!("activity.creating_collection", name = name);
        let activity_id = self.activity.update(cx, |a, cx| a.start(&label, None, cx));

        let collections_service = Arc::clone(&self.collections_service);
        cx.spawn(async move |this, async_cx| {
              let result =
                  async_cx.background_executor()
                          .spawn(async move { collections_service.create_collection(&name) })
                          .await;

              match result {
                  Ok(entry) => {
                      let collection_id = entry.id;
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.collections.push(entry);
                              ctrl.activity
                                  .update(cx, |a, cx| a.complete(activity_id, cx));
                              cx.emit(LibraryChanged);
                              ctrl.add_item_to_collection(collection_id, item_id, product_id, cx);
                          })
                          .ok();
                  }
                  Err(e) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.activity.update(cx, |a, cx| {
                                               a.error(activity_id, e.to_string(), cx);
                                           });
                              cx.emit(CollectionCreateFailed { message: e.message.clone(), });
                          })
                          .ok();
                  }
              }
          })
          .detach();
    }

    /// Invokes the catalog auto-load freshness policy, running a full live
    /// fetch only when the policy determines one is needed.
    ///
    /// Used by the "Catalog > Reload" menu action. Gated by
    /// [`reload_cooldown_active`]: if the on-disk cache was written more
    /// recently than `FORCE_RELOAD_COOLDOWN_SECS`, this is a silent no-op —
    /// no network fetch, no change to `catalog_loading` or the catalog.
    pub fn reload_catalog(&mut self, cx: &mut Context<Self>) {
        let meta = load_cache_metadata(&cache_dir());
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                                              .map(|d| d.as_secs())
                                              .unwrap_or_default();
        if reload_cooldown_active(meta.as_ref(), now) {
            return;
        }
        self.catalog_loading = true;
        cx.emit(LibraryChanged);
        self.start_load_inner(cx, false);
    }

    /// Drops the in-memory catalog and collections, then forces a full live
    /// fetch.
    ///
    /// Used after the on-disk app cache has been cleared, so stale content
    /// disappears from the UI immediately instead of lingering until an
    /// unrelated reload repopulates it from what is now a missing cache file.
    ///
    /// Any catalog load already in flight is superseded: `start_load_inner`
    /// bumps `load_generation`, so the older task's completion is discarded
    /// rather than clobbering the fresh reload triggered here.
    /// Queued-but-not-yet-started thumbnail fetches are dropped too; a
    /// fetch already in flight still completes and populates `CoverCache`,
    /// which is harmless since it only caches an image for a URL that may
    /// no longer be visible.
    pub fn clear_and_reload(&mut self, cx: &mut Context<Self>) {
        self.catalog.clear();
        self.collections.clear();
        self.collection_members.clear();
        self.section_counts = SectionCounts::default();
        self.publishers.clear();
        self.invalidate_cache();
        // Queued availability checks reference items that no longer exist in
        // the now-cleared catalog; an in-flight check (if any) still
        // completes normally and harmlessly finds nothing to update.
        self.check_queue.clear();
        for (id, _url, _force) in self.thumbnail_queue.drain(..) {
            cx.global_mut::<CoverCache>().in_flight.remove(&id);
        }
        // If nothing is currently in flight, no pending fetch completion will
        // ever run `drain_thumbnail_queue`'s empty-queue completion
        // branch — without this, the aggregated activity item would be
        // left showing "in progress" indefinitely.
        if self.active_thumbnail_fetches == 0
           && let Some(id) = self.thumbnail_activity_id.take()
        {
            self.thumbnail_processed = 0;
            self.activity.update(cx, |a, cx| a.complete(id, cx));
        }
        self.reload_catalog(cx);
    }

    /// Emits `LibraryChanged` to trigger a UI re-render without modifying
    /// state.
    ///
    /// Used by sidebar section headers to force a re-render after persisting UI
    /// prefs.
    pub fn notify_ui_change(&mut self, cx: &mut Context<Self>) {
        cx.emit(LibraryChanged);
    }

    /// Deletes the collection with the given id via the service, then removes
    /// it locally.
    ///
    /// Logs failures to the activity panel and leaves the collection in place
    /// on error.
    pub fn delete_collection(&mut self, id: u64, cx: &mut Context<Self>) {
        let label = t!("activity.deleting_collection").to_string();
        let activity_id = self.activity.update(cx, |a, cx| a.start(&label, None, cx));
        let collections_service = Arc::clone(&self.collections_service);
        cx.spawn(async move |this, async_cx| {
              let result = async_cx.background_executor()
                                   .spawn(async move { collections_service.delete_collection(id) })
                                   .await;
              match result {
                  Ok(()) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.collections.retain(|c| c.id != id);
                              if let SidebarFilter::Collection(cid, _) = &ctrl.filter
                                 && *cid == id
                              {
                                  ctrl.filter = SidebarFilter::default();
                                  ctrl.collection_members.clear();
                              }
                              ctrl.activity
                                  .update(cx, |a, cx| a.complete(activity_id, cx));
                              cx.emit(LibraryChanged);
                          })
                          .ok();
                  }
                  Err(e) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.activity.update(cx, |a, cx| {
                                               a.error(activity_id, e.to_string(), cx);
                                           });
                          })
                          .ok();
                  }
              }
          })
          .detach();
    }

    /// Adds `item_id` (an item's `order_product_id`/`product_id`) as a member
    /// of the collection with `collection_id`.
    ///
    /// `item_id` drives the local optimistic `member_ids`/`collection_members`
    /// update and its rollback-on-failure branch, matching the id space
    /// `CollectionEntry::member_ids` already uses. `product_id` is the item's
    /// catalog `product_id`, sent as the network add call's product
    /// identifier — the API's `product_list_items` endpoint rejects
    /// `order_product_id` values with an invalid-product-id error.
    ///
    /// Updates `member_ids` (and `collection_members`, if that collection is
    /// the active filter) immediately, then confirms the change via the
    /// service. On failure the optimistic update is rolled back and
    /// [`CollectionMemberAddFailed`] is emitted so the window can show an
    /// error notification.
    pub fn add_item_to_collection(&mut self, collection_id: u64, item_id: u64, product_id: u64,
                                  cx: &mut Context<Self>) {
        let Some(collection) = self.collections.iter_mut().find(|c| c.id == collection_id)
        else {
            return;
        };
        if member_ids_contain(&collection.member_ids, item_id, product_id) {
            return;
        }
        let collection_name = collection.name.clone();
        let mut member_ids: Vec<u64> = collection.member_ids.iter().copied().collect();
        member_ids.push(item_id);
        collection.member_ids = Arc::from(member_ids.as_slice());

        if matches!(&self.filter, SidebarFilter::Collection(id, _) if *id == collection_id) {
            self.collection_members.insert(item_id);
        }
        cx.emit(LibraryChanged);

        let collections_service = Arc::clone(&self.collections_service);
        cx.spawn(async move |this, async_cx| {
              let result = async_cx.background_executor()
                                   .spawn(async move {
                                       collections_service.add_member(collection_id, product_id)
                                   })
                                   .await;
              if let Err(e) = result {
                  // A conflict means the item is already a member server-side: the
                  // optimistic local update already matches reality, so it is left
                  // in place rather than rolled back, and this is surfaced as a
                  // low-severity notice instead of a hard failure.
                  if e.kind == CollectionsServiceErrorKind::Conflict {
                      this.update(async_cx, |_ctrl, cx| {
                              cx.emit(CollectionMemberAlreadyPresent { message: e.message.clone(), });
                          })
                          .ok();
                      return;
                  }
                  this.update(async_cx, |ctrl, cx| {
                          if let Some(collection) =
                              ctrl.collections.iter_mut().find(|c| c.id == collection_id)
                          {
                              let member_ids: Vec<u64> = collection.member_ids
                                                                   .iter()
                                                                   .copied()
                                                                   .filter(|id| *id != item_id)
                                                                   .collect();
                              collection.member_ids = Arc::from(member_ids.as_slice());
                          }
                          if matches!(&ctrl.filter, SidebarFilter::Collection(id, _) if *id == collection_id)
                          {
                              ctrl.collection_members.remove(&item_id);
                          }
                          cx.emit(LibraryChanged);
                          ctrl.activity.update(cx, |a, cx| {
                                           a.log_alert(format!(
                                               "Add to collection '{collection_name}'"
                                           ),
                                                       e.message.clone(),
                                                       cx);
                                       });
                          cx.emit(CollectionMemberAddFailed { message: e.message.clone(), });
                      })
                      .ok();
              }
          })
          .detach();
    }

    /// Removes `item_id` (an item's `order_product_id`/`product_id`) as a
    /// member of the collection with `collection_id`.
    ///
    /// `item_id` drives matching against the local optimistic `member_ids`/
    /// `collection_members` cache, which may hold either id depending on how
    /// the entry was populated (server data is `product_id`-keyed; a locally
    /// added entry may still carry `item_id`). `product_id` is the item's
    /// catalog `product_id`, sent as the network removal call's product
    /// identifier — the API's server-side lookup that resolves the removal
    /// target matches by `product_id`, never `order_product_id`, so sending
    /// the wrong one causes the item to silently not be found.
    ///
    /// Updates `member_ids` (and `collection_members`, if that collection is
    /// the active filter) immediately, then confirms the change via the
    /// service. On failure the optimistic update is rolled back and
    /// [`CollectionMemberRemoveFailed`] is emitted so the window can show an
    /// error notification.
    pub fn remove_item_from_collection(&mut self, collection_id: u64, item_id: u64,
                                       product_id: u64, cx: &mut Context<Self>) {
        let Some(collection) = self.collections.iter_mut().find(|c| c.id == collection_id)
        else {
            return;
        };
        if !member_ids_contain(&collection.member_ids, item_id, product_id) {
            return;
        }
        let collection_name = collection.name.clone();
        // Remove whichever id(s) are actually present, and remember them so a
        // failed removal restores exactly what was there before rather than
        // introducing an id that was never actually cached.
        let removed_ids: Vec<u64> = collection.member_ids
                                              .iter()
                                              .copied()
                                              .filter(|id| *id == item_id || *id == product_id)
                                              .collect();
        let member_ids: Vec<u64> = collection.member_ids
                                             .iter()
                                             .copied()
                                             .filter(|id| *id != item_id && *id != product_id)
                                             .collect();
        collection.member_ids = Arc::from(member_ids.as_slice());

        if matches!(&self.filter, SidebarFilter::Collection(id, _) if *id == collection_id) {
            self.collection_members.remove(&item_id);
            self.collection_members.remove(&product_id);
        }
        cx.emit(LibraryChanged);

        let collections_service = Arc::clone(&self.collections_service);
        cx.spawn(async move |this, async_cx| {
              let result = async_cx.background_executor()
                                   .spawn(async move {
                                       collections_service.remove_member(collection_id, product_id)
                                   })
                                   .await;
              if let Err(e) = result {
                  this.update(async_cx, |ctrl, cx| {
                          if let Some(collection) =
                              ctrl.collections.iter_mut().find(|c| c.id == collection_id)
                          {
                              let mut member_ids: Vec<u64> = collection.member_ids
                                                                       .iter()
                                                                       .copied()
                                                                       .collect();
                              member_ids.extend(&removed_ids);
                              collection.member_ids = Arc::from(member_ids.as_slice());
                          }
                          if matches!(&ctrl.filter, SidebarFilter::Collection(id, _) if *id == collection_id)
                          {
                              ctrl.collection_members.extend(&removed_ids);
                          }
                          cx.emit(LibraryChanged);
                          ctrl.activity.update(cx, |a, cx| {
                                           a.log_alert(format!(
                                               "Remove from collection '{collection_name}'"
                                           ),
                                                       e.message.clone(),
                                                       cx);
                                       });
                          cx.emit(CollectionMemberRemoveFailed { message: e.message.clone(), });
                      })
                      .ok();
              }
          })
          .detach();
    }

    // ── Shared thumbnail/download concurrency ────────────────────────────────

    /// Returns how many more thumbnail fetches and/or file downloads can be
    /// dispatched right now without exceeding `max_concurrent_downloads`.
    ///
    /// Thumbnail and download slots draw from the same aggregate limit (see
    /// `thumbnail-queue-concurrency`'s "shared limit" requirement).
    fn available_slots(&self) -> usize {
        remaining_slots(self.max_concurrent_downloads,
                        self.active_thumbnail_fetches,
                        self.active_downloads)
    }

    /// Updates the shared thumbnail/download concurrency limit and drains
    /// both queues in case the new limit freed up slots.
    ///
    /// Called when [`crate::controllers::settings::SettingsController`]
    /// reports a `SettingsChanged` event, so a change made on the Storage
    /// settings page takes effect immediately.
    pub fn set_max_concurrent_downloads(&mut self, n: usize, cx: &mut Context<Self>) {
        if self.max_concurrent_downloads == n {
            return;
        }
        self.max_concurrent_downloads = n;
        self.drain_thumbnail_queue(cx);
        self.drain_download_queue(cx);
    }

    /// Updates the "Recently Updated" sidebar window and recomputes the
    /// section counts and cached filtered items so the change is reflected
    /// immediately.
    ///
    /// Called when [`crate::controllers::settings::SettingsController`]
    /// reports a `SettingsChanged` event, so a change made on the Storage
    /// settings page takes effect immediately.
    pub fn set_recently_updated_window_days(&mut self, days: u32, cx: &mut Context<Self>) {
        if self.recently_updated_window_days == days {
            return;
        }
        self.recently_updated_window_days = days;
        self.section_counts = section_counts(&self.catalog, now_secs(), days);
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    // ── Thumbnail loading
    // ──────────────────────────────────────────────────────

    /// Enqueues thumbnail fetches for items that have a `cover_url` not yet
    /// cached or in flight.  Must be called before items are added to `catalog`
    /// so the in-flight marker is set before any render pass can check it.
    ///
    /// Disk-cached covers are loaded synchronously into [`CoverCache`] here,
    /// before this function returns, so the very first render pass after a
    /// catalog load already has the real thumbnail available and never shows
    /// the generative placeholder for items that were already downloaded in a
    /// prior session. Only items with no disk-cached bytes fall through to the
    /// async network queue.
    fn enqueue_thumbnails(&mut self, items: &[LibraryItem], cx: &mut Context<Self>) {
        let covers_root = covers_dir();
        let to_enqueue: Vec<(Arc<str>, Arc<str>)> = {
            let cache = cx.global_mut::<CoverCache>();
            items.iter()
                 .filter_map(|item| {
                     let url = item.cover_url.as_ref()?;
                     let id = Arc::clone(&item.id);
                     if cache.get(&id).is_some() || cache.is_in_flight(&id) {
                         return None;
                     }
                     if let Some(bytes) = load_cached_cover(&covers_root, &id) {
                         cache.insert(Arc::clone(&id), bytes);
                         return None;
                     }
                     Some((id, Arc::clone(url)))
                 })
                 .collect()
        };
        for (id, url) in &to_enqueue {
            cx.global_mut::<CoverCache>().mark_in_flight(Arc::clone(id));
            self.thumbnail_queue
                .push_back((Arc::clone(id), Arc::clone(url), false));
        }
        self.drain_thumbnail_queue(cx);
    }

    /// Dispatches thumbnail fetches for as many queued URLs as there are free
    /// concurrency slots (see [`Self::available_slots`]).
    fn drain_thumbnail_queue(&mut self, cx: &mut Context<Self>) {
        while self.available_slots() > 0 {
            let Some((item_id, url, force_network)) = self.thumbnail_queue.pop_front()
            else {
                break;
            };
            self.active_thumbnail_fetches += 1;
            self.dispatch_thumbnail_fetch(item_id, url, force_network, cx);
        }
    }

    /// Starts a single thumbnail fetch task. Assumes the caller has already
    /// reserved a concurrency slot by incrementing `active_thumbnail_fetches`.
    fn dispatch_thumbnail_fetch(&mut self, item_id: Arc<str>, url: Arc<str>,
                                force_network: bool, cx: &mut Context<Self>) {
        let activity_id = if let Some(id) = self.thumbnail_activity_id {
            id
        }
        else {
            let label = if self.thumbnail_refresh_active {
                t!("activity.refreshing_thumbnails")
            }
            else {
                t!("activity.loading_thumbnails")
            };
            let id = self.activity.update(cx, |a, cx| a.start(&label, None, cx));
            self.thumbnail_activity_id = Some(id);
            self.thumbnail_processed = 0;
            id
        };

        let weak_activity = self.activity.downgrade();
        let url_str = url.to_string();
        let network_monitor = Arc::clone(&self.network_monitor);

        cx.spawn(async move |this, async_cx| {
              // gpui's executors are not a Tokio runtime, and `dtrpg-ui` does not depend on
              // `tokio` directly, so the async `reqwest::get(...).await` used here previously
              // had no reactor to run on and always failed. `reqwest::blocking` manages its
              // own internal runtime per call and works from a plain OS thread, matching the
              // pattern the SDK gateway already uses (`tokio::runtime::Runtime::block_on`) to
              // run network calls from these same background-executor threads.
              //
              // Disk-cache-first: the in-memory `CoverCache` is always empty at startup, so
              // without a persistent disk cache every launch would re-download every cover
              // from the network. Check disk before hitting the network, and persist a fresh
              // network fetch to disk so the next launch is a cache hit.
              let fetch_url = url_str.clone();
              let disk_key = Arc::clone(&item_id);
              let cancel = std::sync::atomic::AtomicBool::new(false);
              let retry_url = fetch_url.clone();
              let result: Result<Vec<u8>, ThumbnailFetchError> =
                  async_cx.background_executor()
                          .spawn(async move {
                              let covers_root = covers_dir();
                              if !force_network
                                 && let Some(bytes) = load_cached_cover(&covers_root, &disk_key)
                              {
                                  return Ok(bytes);
                              }
                              if !network_monitor.check_endpoint(
                                  crate::data::constants::DTRPG_API_HOST,
                              ) {
                                  return Err(ThumbnailFetchError {
                                      message:     "DriveThruRPG API endpoint unreachable"
                                          .to_string(),
                                      retry_after: None,
                                  });
                              }
                              let config = retry::RetryConfig {
                                  max_attempts: crate::data::constants::IMAGE_CACHE_MAX_ATTEMPTS,
                                  base_secs: crate::data::constants::IMAGE_CACHE_RETRY_BASE_DELAY_SECS,
                                  max_secs: crate::data::constants::IMAGE_CACHE_RETRY_MAX_DELAY_SECS,
                              };
                              let mut on_retry = |attempt: u32, delay: std::time::Duration,
                                                   reason: &ThumbnailFetchError| {
                                  tracing::debug!(
                                      url = %retry_url,
                                      attempt,
                                      delay_secs = delay.as_secs(),
                                      reason = %reason,
                                      "cover thumbnail fetch retrying"
                                  );
                              };
                              let bytes = retry::retry_with_backoff(
                                  config,
                                  &cancel,
                                  || -> Result<Vec<u8>, ThumbnailFetchError> {
                                      let resp = reqwest::blocking::get(&fetch_url).map_err(
                                          |e| ThumbnailFetchError {
                                              message:     e.to_string(),
                                              retry_after: None,
                                          },
                                      )?;
                                      if resp.status().as_u16() == 429 {
                                          let retry_after = resp
                                              .headers()
                                              .get(reqwest::header::RETRY_AFTER)
                                              .and_then(|v| v.to_str().ok())
                                              .and_then(|v| v.trim().parse::<u64>().ok())
                                              .map(std::time::Duration::from_secs);
                                          return Err(ThumbnailFetchError {
                                              message: "rate limited (HTTP 429)".to_string(),
                                              retry_after,
                                          });
                                      }
                                      Ok(resp
                                          .bytes()
                                          .map_err(|e| ThumbnailFetchError {
                                              message:     e.to_string(),
                                              retry_after: None,
                                          })?
                                          .to_vec())
                                  },
                                  |_| true,
                                  |e: &ThumbnailFetchError| e.retry_after,
                                  Some(&mut on_retry),
                              )?;
                              save_cached_cover(&covers_root, &disk_key, &bytes);
                              Ok(bytes)
                          })
                          .await;

              match result {
                  Ok(bytes) => {
                      this.update(async_cx, |ctrl, cx| {
                              cx.global_mut::<CoverCache>()
                                .insert(Arc::clone(&item_id), bytes);
                              if let Some(item) = ctrl.catalog.iter_mut().find(|i| i.id == item_id)
                              {
                                  item.thumbnail_last_attempted =
                                      Some(std::time::SystemTime::now());
                              }
                              ctrl.invalidate_cache();
                              ctrl.active_thumbnail_fetches =
                                  ctrl.active_thumbnail_fetches.saturating_sub(1);
                              ctrl.record_refresh_result(&item_id, false, cx);
                              cx.emit(LibraryChanged);
                              ctrl.drain_thumbnail_queue(cx);
                          })
                          .ok();
                  }
                  Err(e) => {
                      tracing::warn!(url = %url_str, error = %e, "thumbnail fetch failed");
                      this.update(async_cx, |ctrl, cx| {
                              cx.global_mut::<CoverCache>().in_flight.remove(&item_id);
                              if let Some(item) = ctrl.catalog.iter_mut().find(|i| i.id == item_id)
                              {
                                  item.thumbnail_last_attempted =
                                      Some(std::time::SystemTime::now());
                              }
                              ctrl.invalidate_cache();
                              ctrl.active_thumbnail_fetches =
                                  ctrl.active_thumbnail_fetches.saturating_sub(1);
                              ctrl.record_refresh_result(&item_id, true, cx);
                              ctrl.drain_thumbnail_queue(cx);
                          })
                          .ok();
                  }
              }

              // Count this attempt (success or failure) toward the batch total so the
              // progress bar reflects real throughput instead of sitting indeterminate.
              let (processed, remaining, refresh_active) = this.update(async_cx, |ctrl, _cx| {
                                                                   ctrl.thumbnail_processed += 1;
                                                                   (ctrl.thumbnail_processed,
                                                                    ctrl.thumbnail_queue.len(),
                                                                    ctrl.thumbnail_refresh_active)
                                                               })
                                                               .unwrap_or((0, 0, false));

              if remaining == 0 {
                  weak_activity.update(async_cx, |a, cx| a.complete(activity_id, cx))
                               .ok();
                  this.update(async_cx, |ctrl, _cx| {
                          ctrl.thumbnail_activity_id = None;
                          ctrl.thumbnail_processed = 0;
                      })
                      .ok();
              }
              else {
                  let label = if refresh_active {
                      t!("activity.refreshing_thumbnails_remaining",
                         remaining = remaining).to_string()
                  }
                  else {
                      t!("activity.loading_thumbnails_remaining",
                         remaining = remaining).to_string()
                  };
                  let total = (processed + remaining) as f32;
                  let progress = if total > 0.0 {
                      processed as f32 / total
                  }
                  else {
                      0.0
                  };
                  weak_activity.update(async_cx, |a, cx| {
                                   a.update_label(activity_id, label, cx);
                                   a.update_progress(activity_id, progress, cx);
                               })
                               .ok();
              }
          })
          .detach();
    }

    /// Enqueues a single cover URL at the front of the thumbnail queue and
    /// starts the drain loop.  Used by the per-item "Load Thumbnail"
    /// context menu action.
    ///
    /// Forces a network re-fetch bypassing both the in-memory and on-disk
    /// caches — this action exists specifically to retry or refresh a
    /// cover, not to redundantly reload whatever is already cached.
    pub fn load_thumbnail(&mut self, cover_url: Arc<str>, cx: &mut Context<Self>) {
        let item_id = self.catalog
                          .iter()
                          .find(|i| i.cover_url.as_deref() == Some(&*cover_url))
                          .map(|i| Arc::clone(&i.id));

        if let Some(id) = item_id {
            self.thumbnail_queue.retain(|(i, _, _)| i != &id);
            cx.global_mut::<CoverCache>()
              .mark_in_flight(Arc::clone(&id));
            self.thumbnail_queue.push_front((id, cover_url, true));
            self.drain_thumbnail_queue(cx);
        }
    }

    /// Enqueues a fetch of `detail_cover_url` for `item_id` if it isn't
    /// already cached (in-memory or on disk) or already in flight.
    ///
    /// Called when a detail tab is opened for an item. Unlike `cover_url`
    /// (fetched eagerly for every catalog item as soon as the catalog loads),
    /// the large-context detail cover is fetched lazily — most items are
    /// never opened in the detail panel, so eagerly downloading a full-size
    /// or WebP image for all of them would waste bandwidth and disk space.
    pub fn ensure_detail_cover(&mut self, item_id: &Arc<str>, cx: &mut Context<Self>) {
        let Some(url) = self.catalog
                            .iter()
                            .find(|i| i.id == *item_id)
                            .and_then(|i| i.detail_cover_url.clone())
        else {
            return;
        };
        let cache_key = detail_cache_key(item_id);
        if cx.global::<CoverCache>().get(&cache_key).is_some()
           || cx.global::<CoverCache>().is_in_flight(&cache_key)
        {
            return;
        }
        if let Some(bytes) = load_cached_cover(&covers_dir(), &cache_key) {
            cx.global_mut::<CoverCache>().insert(cache_key, bytes);
            return;
        }
        cx.global_mut::<CoverCache>()
          .mark_in_flight(Arc::clone(&cache_key));
        self.thumbnail_queue.push_front((cache_key, url, false));
        self.drain_thumbnail_queue(cx);
    }

    /// Force re-fetches the large-context detail cover for `item_id`,
    /// bypassing the cache — used by the detail panel's refresh-thumbnail
    /// action. Falls back to refreshing the small-context `cover_url` when
    /// the item has no `detail_cover_url`.
    pub fn refresh_detail_cover(&mut self, item_id: Arc<str>, cx: &mut Context<Self>) {
        let Some(item) = self.catalog.iter().find(|i| i.id == item_id)
        else {
            return;
        };
        if let Some(url) = item.detail_cover_url.clone() {
            let cache_key = detail_cache_key(&item_id);
            self.thumbnail_queue.retain(|(i, _, _)| *i != cache_key);
            cx.global_mut::<CoverCache>()
              .mark_in_flight(Arc::clone(&cache_key));
            self.thumbnail_queue.push_front((cache_key, url, true));
            self.drain_thumbnail_queue(cx);
        }
        else if let Some(url) = item.cover_url.clone() {
            self.load_thumbnail(url, cx);
        }
    }

    /// Re-fetches thumbnails for every catalog item with a `cover_url`,
    /// overwriting any cached image (in-memory, and on disk). Used by the
    /// "Refresh Thumbnails" catalog menu action.
    ///
    /// Unlike [`enqueue_thumbnails`](Self::enqueue_thumbnails), this does not
    /// skip items already present in the cache — every eligible item is
    /// re-queued with a forced network re-fetch.
    pub fn refresh_all_thumbnails(&mut self, cx: &mut Context<Self>) {
        let to_enqueue: Vec<(Arc<str>, Arc<str>, bool)> =
            self.catalog
                .iter()
                .filter_map(|item| {
                    let url = item.cover_url.as_ref()?;
                    Some((Arc::clone(&item.id), Arc::clone(url), true))
                })
                .collect();

        if to_enqueue.is_empty() {
            cx.emit(ThumbnailRefreshNoOp);
            return;
        }

        self.thumbnail_queue.clear();
        // This forces a fresh full re-fetch batch — reset the processed counter
        // so the progress bar denominator isn't skewed by whatever
        // fraction of a prior batch had already completed before this
        // action was invoked.
        self.thumbnail_processed = 0;
        self.thumbnail_refresh_active = true;
        self.thumbnail_refresh_pending =
            to_enqueue.iter().map(|(id, _, _)| Arc::clone(id)).collect();
        self.thumbnail_refresh_succeeded = 0;
        self.thumbnail_refresh_failed = 0;
        let cache = cx.global_mut::<CoverCache>();
        for (id, _, _) in &to_enqueue {
            cache.mark_in_flight(Arc::clone(id));
        }
        cx.emit(ThumbnailRefreshStarted { count: to_enqueue.len(), });
        self.thumbnail_queue.extend(to_enqueue);
        self.drain_thumbnail_queue(cx);
    }

    /// Records a refresh-batch fetch result for `item_id`, if it belongs to the
    /// batch currently pending in [`Self::thumbnail_refresh_pending`].
    ///
    /// The first completion for a given id claims membership by removing it
    /// from the pending set — whether that completion came from the fetch the
    /// refresh itself dispatched, or from an unrelated fetch that happened to
    /// already be in flight for the same id when the refresh started. Any
    /// later completion for the same id finds it already removed and is
    /// silently ignored. This keeps [`ThumbnailRefreshCompleted`]'s totals
    /// matching exactly what [`ThumbnailRefreshStarted`] announced, even
    /// though refresh and ordinary per-page loads share one queue and one
    /// activity entry and can interleave.
    fn record_refresh_result(&mut self, item_id: &Arc<str>, failed: bool, cx: &mut Context<Self>) {
        if !self.thumbnail_refresh_pending.remove(item_id) {
            return;
        }
        if failed {
            self.thumbnail_refresh_failed += 1;
        }
        else {
            self.thumbnail_refresh_succeeded += 1;
        }
        if self.thumbnail_refresh_active && self.thumbnail_refresh_pending.is_empty() {
            self.thumbnail_refresh_active = false;
            let succeeded = std::mem::take(&mut self.thumbnail_refresh_succeeded);
            let failed = std::mem::take(&mut self.thumbnail_refresh_failed);
            cx.emit(ThumbnailRefreshCompleted { succeeded, failed });
        }
    }

    // ── Snapshot
    // ──────────────────────────────────────────────────────────────

    /// Returns all data needed by the root view for one render pass.
    pub fn snapshot(&self) -> LibrarySnapshot {
        let now_secs = now_secs();
        let window_days = self.recently_updated_window_days;
        let filter_count = self.catalog
                               .iter()
                               .filter(|i| {
                                   item_matches_filter(i,
                                                       &self.filter,
                                                       &self.collection_members,
                                                       now_secs,
                                                       window_days)
                               })
                               .count();
        let items = self.visible_items();
        let matched_count = items.len();
        let selected_item = self.selected_item().cloned();
        let catalog_ids = self.catalog_ids();
        LibrarySnapshot { filter: self.filter.clone(),
                          counts: self.section_counts,
                          publishers: self.publishers.clone(),
                          collections: self.collections.clone(),
                          collections_loaded: self.collections_loaded,
                          catalog_ids,
                          total_count: self.section_counts.all,
                          total_mb: self.total_size_mb(),
                          filter_count,
                          matched_count,
                          search_query: self.search_query.clone(),
                          sort: self.sort,
                          sort_direction: self.sort_direction,
                          collection_sort: self.collection_sort,
                          collection_sort_direction: self.collection_sort_direction,
                          grouped: self.grouped,
                          presentation: self.presentation,
                          selected_item,
                          items,
                          catalog_loading: self.catalog_loading,
                          publisher_search_open: self.publisher_search_open,
                          publisher_search_query: self.publisher_search_query.clone(),
                          collection_search_open: self.collection_search_open,
                          collection_search_query: self.collection_search_query.clone(),
                          detail_panel_width: self.detail_panel_width }
    }

    /// Returns true while the initial catalog fetch is still in flight and no
    /// items have arrived.
    pub fn is_loading(&self) -> bool {
        self.catalog_loading && self.catalog.is_empty()
    }

    /// Builds the set of all product IDs that can match a collection's
    /// `member_ids`.
    ///
    /// Includes both `order_product_id` and `product_id` since the product
    /// list items API returns `productId` (not `orderProductId`) in its
    /// response.
    fn catalog_ids(&self) -> HashSet<u64> {
        self.catalog
            .iter()
            .flat_map(|i| {
                let mut ids = Vec::with_capacity(2);
                if i.order_product_id > 0 {
                    ids.push(i.order_product_id);
                }
                if i.product_id > 0 {
                    ids.push(i.product_id);
                }
                ids
            })
            .collect()
    }

    // ── Filtered result set
    // ───────────────────────────────────────────────────

    /// Recomputes the filtered, sorted item cache from the current catalog,
    /// filter, search query, and sort settings.
    ///
    /// Called eagerly at every mutation site that changes any of those inputs
    /// so render-path accessors never re-scan the catalog.
    fn invalidate_cache(&mut self) {
        let now_secs = now_secs();
        let window_days = self.recently_updated_window_days;
        let mut items: Vec<LibraryItem> = self.catalog
                                              .iter()
                                              .filter(|i| {
                                                  item_matches_filter(i,
                                                                      &self.filter,
                                                                      &self.collection_members,
                                                                      now_secs,
                                                                      window_days)
                                                  && item_matches_query(i, &self.search_query)
                                              })
                                              .cloned()
                                              .collect();
        sort_items(&mut items, self.sort, self.sort_direction);
        self.items_cache = Some(items);
    }

    /// Returns the filtered, sorted result set for the current state.
    #[must_use]
    pub fn visible_items(&self) -> Vec<LibraryItem> {
        self.items_cache.clone().unwrap_or_default()
    }

    /// Returns the number of items in the filtered, sorted result set.
    ///
    /// Cheaper than cloning the full `Vec` when only the count is needed.
    #[must_use]
    pub fn visible_items_count(&self) -> usize {
        self.visible_items().len()
    }

    /// Returns a slice of the filtered, sorted result set.
    ///
    /// `range` is 0-based relative to the start of the full result set. Used
    /// by `uniform_list` render closures.
    #[must_use]
    pub fn visible_items_slice(&self, range: std::ops::Range<usize>) -> Vec<LibraryItem> {
        let items = self.visible_items();
        items.get(range)
             .map(<[LibraryItem]>::to_vec)
             .unwrap_or_default()
    }

    // ── Sidebar filter mutations
    // ──────────────────────────────────────────────

    /// Sets the active sidebar filter.
    ///
    /// When the new filter is `Collection(id, _)`, `collection_members` is
    /// populated from the matching entry's `member_ids`. For all other
    /// filters, `collection_members` is cleared.
    pub fn set_filter(&mut self, filter: SidebarFilter, cx: &mut Context<Self>) {
        if let SidebarFilter::Collection(id, _) = &filter {
            self.collection_members = self.collections
                                          .iter()
                                          .find(|c| c.id == *id)
                                          .map(|c| c.member_ids.iter().copied().collect())
                                          .unwrap_or_default();
        }
        else {
            self.collection_members.clear();
        }
        self.filter = filter;
        self.selection = Selection::None;
        self.invalidate_cache();
        self.save_view_prefs();
        cx.emit(LibraryChanged);
    }

    // ── Search mutations
    // ──────────────────────────────────────────────────────

    /// Updates the text search query.
    pub fn set_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.search_query = query;
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Clears the text search query.
    pub fn clear_search_query(&mut self, cx: &mut Context<Self>) {
        self.search_query.clear();
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    // ── Sidebar section search mutations
    // ──────────────────────────────────────
    //
    // These filter the publishers/collections sidebar lists only. They never
    // touch the catalog cache and are intentionally session-only: closing a
    // section's search bar clears its query so reopening it always starts
    // fresh.

    /// Toggles the publishers section's inline search bar, clearing its query
    /// on close.
    pub fn toggle_publisher_search(&mut self, cx: &mut Context<Self>) {
        self.publisher_search_open = !self.publisher_search_open;
        if !self.publisher_search_open {
            self.publisher_search_query.clear();
        }
        cx.emit(LibraryChanged);
    }

    /// Updates the publishers section search filter text.
    pub fn set_publisher_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.publisher_search_query = query;
        cx.emit(LibraryChanged);
    }

    /// Toggles the collections section's inline search bar, clearing its query
    /// on close.
    pub fn toggle_collection_search(&mut self, cx: &mut Context<Self>) {
        self.collection_search_open = !self.collection_search_open;
        if !self.collection_search_open {
            self.collection_search_query.clear();
        }
        cx.emit(LibraryChanged);
    }

    /// Updates the collections section search filter text.
    pub fn set_collection_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.collection_search_query = query;
        cx.emit(LibraryChanged);
    }

    // ── Sort mutations
    // ────────────────────────────────────────────────────────

    /// Sets the sort method.
    pub fn set_sort(&mut self, sort: SortMethod, cx: &mut Context<Self>) {
        self.sort = sort;
        self.invalidate_cache();
        self.save_view_prefs();
        cx.emit(LibraryChanged);
    }

    /// Sets the sort direction.
    pub fn set_sort_direction(&mut self, direction: SortDirection, cx: &mut Context<Self>) {
        self.sort_direction = direction;
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Sets the sidebar Collections sort method, persists it, and re-sorts
    /// `self.collections` in place.
    pub fn set_collection_sort(&mut self, method: CollectionSortMethod, cx: &mut Context<Self>) {
        self.collection_sort = method;
        let catalog_ids = self.catalog_ids();
        sort_collections(&mut self.collections,
                         method,
                         self.collection_sort_direction,
                         &catalog_ids);
        crate::data::ui_preferences::UiPreferences::load().save_collection_sort(method,
                                                                    self.collection_sort_direction);
        cx.emit(LibraryChanged);
    }

    /// Sets the sidebar Collections sort direction, persists it, and
    /// re-sorts `self.collections` in place.
    pub fn set_collection_sort_direction(&mut self, direction: SortDirection,
                                         cx: &mut Context<Self>) {
        self.collection_sort_direction = direction;
        let catalog_ids = self.catalog_ids();
        sort_collections(&mut self.collections,
                         self.collection_sort,
                         direction,
                         &catalog_ids);
        crate::data::ui_preferences::UiPreferences::load().save_collection_sort(self.collection_sort,
                                                                    direction);
        cx.emit(LibraryChanged);
    }

    // ── Grouping / presentation mutations ────────────────────────────────────

    /// Toggles publisher grouping on or off.
    pub fn set_grouped(&mut self, grouped: bool, cx: &mut Context<Self>) {
        self.grouped = grouped;
        self.save_view_prefs();
        cx.emit(LibraryChanged);
    }

    /// Switches the catalog presentation mode.
    pub fn set_presentation(&mut self, mode: CatalogPresentation, cx: &mut Context<Self>) {
        self.presentation = mode;
        self.save_view_prefs();
        cx.emit(LibraryChanged);
    }

    /// Persists the current sidebar filter, sort, grouped, and presentation
    /// state to disk. Best-effort — I/O failures are logged and ignored (see
    /// [`crate::data::file_openers::CatalogViewPrefs::save`]).
    fn save_view_prefs(&self) {
        crate::data::file_openers::CatalogViewPrefs::from_state(&self.filter,
                                                                self.sort,
                                                                self.grouped,
                                                                self.presentation).save();
    }

    // ── Selection mutations
    // ───────────────────────────────────────────────────

    /// Opens the single-click item popover for `id` (see `main-window-tabs`).
    ///
    /// Distinct from opening an expanded detail tab, which is a separate,
    /// double-click action handled by `TabsController::open_detail_tab`.
    pub fn select_item(&mut self, id: Arc<str>, cx: &mut Context<Self>) {
        self.maybe_check_item(Arc::clone(&id), cx);
        self.verify_selected_item_download(Arc::clone(&id), cx);
        self.selection = Selection::Item(id);
        cx.emit(LibraryChanged);
    }

    /// Verifies `id`'s files against disk in the background and applies any
    /// resulting `downloaded`/`status` change, per
    /// `verify-downloaded-status-against-disk`.
    ///
    /// Independent of [`Self::maybe_check_item`]'s network-bound
    /// availability check (and its cooldown): a missing or reappeared file
    /// on disk is a local fact unrelated to whether the server still lists
    /// the item, and a local `Path::exists()` call per file is cheap enough
    /// that gating it adds complexity without a real cost problem to solve.
    fn verify_selected_item_download(&mut self, id: Arc<str>, cx: &mut Context<Self>) {
        let Some(mut item) = self.item_by_id(&id).cloned()
        else {
            return;
        };
        self.verifying_downloads.insert(Arc::clone(&id));
        cx.emit(LibraryChanged);

        cx.spawn(async move |this, async_cx| {
              let (item, changed) = async_cx.background_executor()
                                            .spawn(async move {
                                                let storage = StorageConfig::load();
                                                let changed =
                                                    verify_item_downloads(&mut item, &storage);
                                                (item, changed)
                                            })
                                            .await;

              this.update(async_cx, |ctrl, cx| {
                      ctrl.verifying_downloads.remove(&id);
                      if changed
                         && let Some(existing) = ctrl.catalog.iter_mut().find(|i| i.id == item.id)
                      {
                          *existing = item;
                          ctrl.section_counts = section_counts(&ctrl.catalog,
                                                               now_secs(),
                                                               ctrl.recently_updated_window_days);
                          ctrl.invalidate_cache();
                      }
                      cx.emit(LibraryChanged);
                  })
                  .ok();
          })
          .detach();
    }

    /// Returns `true` if `id`'s files are currently being verified against
    /// disk (catalog-wide load-time pass or the on-demand single-item
    /// check), per `verify-downloaded-status-against-disk`.
    #[must_use]
    pub fn is_verifying_downloads(&self, id: &str) -> bool {
        self.verifying_downloads.contains(id)
    }

    /// Returns a snapshot of every item id currently undergoing file-presence
    /// verification, mirroring [`Self::checking_items_snapshot`]'s role for
    /// non-virtualized render paths.
    #[must_use]
    pub fn verifying_downloads_snapshot(&self) -> HashSet<Arc<str>> {
        self.verifying_downloads.clone()
    }

    // ── Item-level availability checks
    // ────────────────────────────────────────

    /// Returns `true` if an availability check is currently in flight for
    /// catalog item `id` (on-demand or from the periodic queue).
    #[must_use]
    pub fn is_checking(&self, id: &str) -> bool {
        self.checking_items.contains(id)
    }

    /// Returns a snapshot of every item id with an availability check
    /// currently in flight.
    ///
    /// Used by non-virtualized render paths (e.g. grouped Thumbs) that build
    /// their per-item element list inside a `move` closure without ongoing
    /// access to `cx`, mirroring how `CoverCache`'s image map is snapshotted
    /// up front for the same reason.
    #[must_use]
    pub fn checking_items_snapshot(&self) -> HashSet<Arc<str>> {
        self.checking_items.clone()
    }

    /// Checks `id`'s availability against the server if it hasn't been
    /// checked within `ITEM_CHECK_COOLDOWN_SECS`, or if it has never been
    /// checked. No-ops if the item is unknown, already within its cooldown,
    /// or already has a check in flight.
    ///
    /// Called when the user views an entry's details (the single-click
    /// popover via [`Self::select_item`], or an expanded detail tab — see
    /// each `open_detail_tab` call site).
    pub fn maybe_check_item(&mut self, id: Arc<str>, cx: &mut Context<Self>) {
        let Some(item) = self.item_by_id(&id)
        else {
            return;
        };
        if !item_check_due(item.availability_last_checked) {
            return;
        }
        self.start_item_check(id, cx);
    }

    /// Starts a background availability check for `id`, bypassing the
    /// per-item cooldown (callers are expected to have already filtered by
    /// staleness — see [`Self::maybe_check_item`] and
    /// [`Self::drain_check_queue`]). No-ops if a check for this id is
    /// already in flight or the item is unknown.
    fn start_item_check(&mut self, id: Arc<str>, cx: &mut Context<Self>) {
        if self.checking_items.contains(&id) || self.item_by_id(&id).is_none() {
            return;
        }
        self.checking_items.insert(Arc::clone(&id));
        cx.emit(LibraryChanged);

        let numeric_id = self.item_by_id(&id).map(|i| i.numeric_id).unwrap_or(0);
        let service_arc = self.vm.service_arc();

        cx.spawn(async move |this, async_cx| {
            let result = async_cx.background_executor()
                                 .spawn(async move { service_arc.get_item(numeric_id) })
                                 .await;
            if let Err(e) = &result
               && e.kind != LibraryServiceErrorKind::NotFound
            {
                // Transient failure: not evidence of removal, leave the item
                // and its flag entirely unchanged.
                tracing::warn!(id = %id, error = %e, "item availability check failed transiently");
            }
            this.update(async_cx, |ctrl, cx| {
                    if let Some(existing) = ctrl.catalog.iter_mut().find(|i| i.id == id) {
                        apply_check_result(existing, result, std::time::SystemTime::now());
                        ctrl.section_counts = section_counts(&ctrl.catalog,
                                                             now_secs(),
                                                             ctrl.recently_updated_window_days);
                        ctrl.invalidate_cache();
                    }
                    ctrl.checking_items.remove(&id);
                    cx.emit(LibraryChanged);
                    ctrl.drain_check_queue(cx);
                })
                .ok();
        })
        .detach();
    }

    /// Pushes `ids` not already queued or in flight onto `check_queue`, then
    /// starts the drain loop.
    ///
    /// Used by [`Self::request_check_batch`] to enqueue a batch of items
    /// overdue for a re-check.
    pub fn enqueue_checks(&mut self, ids: impl Iterator<Item = Arc<str>>, cx: &mut Context<Self>) {
        for id in ids {
            if should_enqueue_check(&self.check_queue, &self.checking_items, &id) {
                self.check_queue.push_back(id);
            }
        }
        self.drain_check_queue(cx);
    }

    /// Starts the next queued availability check if none is currently in
    /// flight, mirroring [`Self::drain_thumbnail_queue`]'s single-flight
    /// pattern. No-ops if a check is already in flight or the queue is
    /// empty; re-invoked from [`Self::start_item_check`]'s completion
    /// handler for the next queued id.
    fn drain_check_queue(&mut self, cx: &mut Context<Self>) {
        if !self.checking_items.is_empty() {
            return;
        }
        let Some(id) = self.check_queue.pop_front()
        else {
            return;
        };
        self.start_item_check(id, cx);
    }

    /// Requests a batch of per-item availability checks, gated by
    /// `ITEM_CHECK_BATCH_COOLDOWN_SECS` and shared between the manual
    /// trigger and the automatic periodic timer so neither can flood the API
    /// by stacking on top of the other.
    ///
    /// No-ops (silently) if a batch was already requested too recently.
    /// Otherwise selects up to `ITEM_CHECK_BATCH_SIZE` catalog items overdue
    /// for a check (oldest-checked first), enqueues them, and immediately
    /// persists the batch timestamp — before the queue finishes draining —
    /// so a slow-draining batch doesn't leave the cooldown window open to a
    /// second trigger firing mid-batch.
    pub fn request_check_batch(&mut self, cx: &mut Context<Self>) {
        let root = cache_dir();
        let last_batch_secs = load_cache_metadata(&root).and_then(|m| m.last_item_check_batch_secs);
        let now = std::time::SystemTime::now();
        let now_secs = now.duration_since(std::time::UNIX_EPOCH)
                          .map(|d| d.as_secs())
                          .unwrap_or_default();
        if check_batch_cooldown_active(last_batch_secs, now_secs) {
            return;
        }

        let ids = select_check_batch(&self.catalog, crate::data::constants::ITEM_CHECK_BATCH_SIZE);
        if let Err(e) = save_check_batch_timestamp(&root, now_secs) {
            tracing::warn!(error = %e, "failed to persist check-batch cooldown timestamp");
        }
        self.enqueue_checks(ids.into_iter(), cx);
    }

    /// Closes the item popover, if one is open.
    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selection = Selection::None;
        cx.emit(LibraryChanged);
    }

    /// Precise on-screen bounds of catalog entry `id`, if it has painted at
    /// least once. Used to anchor the single-click item popover beside the
    /// entry that opened it — see [`Self::set_entry_bounds`].
    ///
    /// Every visible Grid card and Thumbs row reports its bounds continuously
    /// (not just the selected one), so by the time an entry can be clicked it
    /// has already painted and its bounds are available immediately — no
    /// one-frame flash at a fallback position before the popover settles
    /// beside the entry.
    pub fn entry_bounds(&self, id: &str) -> Option<Bounds<Pixels>> {
        self.entry_bounds.get(id).copied()
    }

    /// Records the on-screen bounds of catalog entry `id`, called from that
    /// entry's own render pass once painted (see
    /// `catalog_view::render_grid_card` / `render_thumb_row`).
    ///
    /// Only re-emits [`LibraryChanged`] when the bounds actually changed, so
    /// repeated paints of an entry that hasn't moved don't trigger a
    /// re-render feedback loop.
    pub fn set_entry_bounds(&mut self, id: Arc<str>, bounds: Bounds<Pixels>,
                            cx: &mut Context<Self>) {
        if self.entry_bounds.get(&id) != Some(&bounds) {
            self.entry_bounds.insert(id, bounds);
            cx.emit(LibraryChanged);
        }
    }

    /// Looks up a catalog item by id, independent of the current popover
    /// selection.
    ///
    /// Used to resolve the full item for an open expanded detail tab, which
    /// tracks only the item id, not a full `LibraryItem` snapshot.
    #[must_use]
    pub fn item_by_id(&self, id: &str) -> Option<&LibraryItem> {
        self.catalog.iter().find(|i| i.id.as_ref() == id)
    }

    // ── Item selection (multi-item detail tab) ───────────────────────────────

    /// Returns the row index of the currently selected item file for a
    /// multi-item entry's detail tab, if any item has been selected.
    #[must_use]
    pub fn selected_item_file(&self, entry_id: &str) -> Option<usize> {
        self.selected_item_file.get(entry_id).copied()
    }

    /// Selects the file at `row_ix` as the active item within `entry_id`'s
    /// expanded detail tab, updating the item metadata area in place.
    ///
    /// Clicking the already-selected row deselects it (toggle), returning
    /// the item list to its unselected prompt state.
    ///
    /// Emits [`LibraryChanged`].
    pub fn select_item_file(&mut self, entry_id: Arc<str>, row_ix: usize, cx: &mut Context<Self>) {
        toggle_row_selection(&mut self.selected_item_file, entry_id, row_ix);
        cx.emit(LibraryChanged);
    }

    /// Clears any selected item for `entry_id`'s detail tab, so reopening it
    /// shows no pre-selected item (selection is ephemeral, see
    /// `catalog-entry-detail-view`).
    ///
    /// Emits [`LibraryChanged`].
    pub fn clear_item_selection(&mut self, entry_id: &str, cx: &mut Context<Self>) {
        let had_selection = self.selected_item_file.remove(entry_id).is_some();
        let had_zip_preview = self.zip_preview
                                  .as_ref()
                                  .is_some_and(|p| p.entry_id.as_ref() == entry_id);
        if had_zip_preview {
            self.zip_preview = None;
        }
        if had_selection || had_zip_preview {
            cx.emit(LibraryChanged);
        }
    }

    // ── Zip content preview (item tier) ──────────────────────────────────────

    /// Returns the active Zip preview's file row and anchor position for
    /// `entry_id`, if one is open (hovered or pinned).
    #[must_use]
    pub fn zip_preview_for(&self, entry_id: &str) -> Option<(usize, Point<Pixels>, bool)> {
        self.zip_preview
            .as_ref()
            .filter(|p| p.entry_id.as_ref() == entry_id)
            .map(|p| (p.row_ix, p.anchor_pos, p.pinned))
    }

    /// Returns `entry_id`'s file at `row_ix`'s Info column value (PDF page
    /// count, Zip file count, or image dimensions).
    ///
    /// Returns the cached value if present. Otherwise, if the file has been
    /// downloaded to `entry_dir`, extracts and caches its Info value via
    /// [`crate::util::file_info::compute_file_info`]; returns
    /// [`FileInfo::None`] immediately for undownloaded files or an unknown
    /// entry/row without touching the cache.
    pub fn file_info(&mut self, entry_id: &Arc<str>, row_ix: usize, entry_dir: &Path)
                     -> crate::util::file_info::FileInfo {
        let Some((downloaded, path, format)) =
            self.item_by_id(entry_id)
                .and_then(|item| item.files.get(row_ix))
                .map(|file| {
                    (file.downloaded, entry_dir.join(file.name.as_ref()), file.format.clone())
                })
        else {
            return crate::util::file_info::FileInfo::None;
        };
        cached_file_info(&mut self.file_info_cache,
                         entry_id,
                         row_ix,
                         downloaded,
                         &path,
                         &format)
    }

    /// Opens (or moves) the hover preview to `entry_id`'s file row `row_ix`,
    /// anchored at `anchor_pos`. No-op while a *different* row is pinned —
    /// pinning takes priority over hover until explicitly dismissed. Also a
    /// no-op if this row is already the active unpinned hover target, so
    /// continuous `on_mouse_move` ticks over the same row don't repeatedly
    /// re-read the archive or re-notify every frame.
    ///
    /// Emits [`LibraryChanged`].
    pub fn hover_zip_preview(&mut self, entry_id: Arc<str>, row_ix: usize,
                             anchor_pos: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(existing) = &self.zip_preview {
            if existing.pinned && (existing.entry_id != entry_id || existing.row_ix != row_ix) {
                return;
            }
            if !existing.pinned && existing.entry_id == entry_id && existing.row_ix == row_ix {
                return;
            }
        }
        self.zip_preview = Some(ZipPreviewState { entry_id,
                                                  row_ix,
                                                  anchor_pos,
                                                  pinned: false });
        cx.emit(LibraryChanged);
    }

    /// Ends the hover preview for `entry_id`'s file row `row_ix` — a no-op if
    /// that row is pinned (pinning survives mouse-out) or if a different row
    /// is the active preview.
    ///
    /// Emits [`LibraryChanged`].
    pub fn clear_hover_zip_preview(&mut self, entry_id: &str, row_ix: usize,
                                   cx: &mut Context<Self>) {
        let should_clear = self.zip_preview.as_ref().is_some_and(|p| {
                                                        !p.pinned
                                                        && p.entry_id.as_ref() == entry_id
                                                        && p.row_ix == row_ix
                                                    });
        if should_clear {
            self.zip_preview = None;
            cx.emit(LibraryChanged);
        }
    }

    /// Toggles the pinned state of `entry_id`'s file row `row_ix`: pins it
    /// open (opening the preview if it wasn't already active) on the first
    /// click, or clears it entirely on a second click of the same row.
    ///
    /// Emits [`LibraryChanged`].
    pub fn toggle_zip_preview_pin(&mut self, entry_id: Arc<str>, row_ix: usize,
                                  anchor_pos: Point<Pixels>, cx: &mut Context<Self>) {
        let already_pinned =
            self.zip_preview
                .as_ref()
                .is_some_and(|p| p.pinned && p.entry_id == entry_id && p.row_ix == row_ix);
        self.zip_preview = if already_pinned {
            None
        }
        else {
            Some(ZipPreviewState { entry_id,
                                   row_ix,
                                   anchor_pos,
                                   pinned: true })
        };
        cx.emit(LibraryChanged);
    }

    /// Closes the Zip preview popover for `entry_id`, whether hovered or
    /// pinned — used by the popover's own close control.
    ///
    /// Emits [`LibraryChanged`].
    pub fn close_zip_preview(&mut self, entry_id: &str, cx: &mut Context<Self>) {
        if self.zip_preview
               .as_ref()
               .is_some_and(|p| p.entry_id.as_ref() == entry_id)
        {
            self.zip_preview = None;
            cx.emit(LibraryChanged);
        }
    }

    // ── Other details disclosure ─────────────────────────────────────────────

    /// Returns whether `entry_id`'s "Other details" section is expanded.
    ///
    /// Defaults to `false` (collapsed) for any entry id not yet toggled, per
    /// `catalog-entry-detail-advanced-disclosure`.
    #[must_use]
    pub fn is_other_details_open(&self, entry_id: &str) -> bool {
        self.other_details_open
            .get(entry_id)
            .copied()
            .unwrap_or(false)
    }

    /// Flips `entry_id`'s "Other details" open/collapsed state.
    ///
    /// Ephemeral — never persisted, and independent per entry id.
    ///
    /// Emits [`LibraryChanged`].
    pub fn toggle_other_details(&mut self, entry_id: Arc<str>, cx: &mut Context<Self>) {
        toggle_bool_flag(&mut self.other_details_open, entry_id);
        cx.emit(LibraryChanged);
    }

    /// Returns whether a single file's "Other details" section is expanded
    /// within a multi-item entry's item tier.
    ///
    /// `key` is `"{entry_id}:{row_ix}"` (built by `file_other_details_key` in
    /// `ui::views::detail_panel_view`). Defaults to `false` (collapsed) for
    /// any key not yet toggled.
    #[must_use]
    pub fn is_file_other_details_open(&self, key: &str) -> bool {
        self.file_other_details_open
            .get(key)
            .copied()
            .unwrap_or(false)
    }

    /// Flips a single file's "Other details" open/collapsed state within a
    /// multi-item entry's item tier.
    ///
    /// Ephemeral — never persisted, and independent per `"{entry_id}:{row_ix}"`
    /// key.
    ///
    /// Emits [`LibraryChanged`].
    pub fn toggle_file_other_details(&mut self, key: Arc<str>, cx: &mut Context<Self>) {
        toggle_bool_flag(&mut self.file_other_details_open, key);
        cx.emit(LibraryChanged);
    }

    // ── Detail panel width
    // ────────────────────────────────────────────────────

    /// Returns the current detail panel width, in pixels.
    #[must_use]
    pub fn detail_panel_width(&self) -> f32 {
        self.detail_panel_width
    }

    /// Sets the detail panel width, clamped to
    /// `[DETAIL_PANEL_MIN_WIDTH, DETAIL_PANEL_MAX_WIDTH]`.
    ///
    /// Emits [`LibraryChanged`].
    pub fn set_detail_panel_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.detail_panel_width = width.clamp(crate::data::constants::DETAIL_PANEL_MIN_WIDTH,
                                              crate::data::constants::DETAIL_PANEL_MAX_WIDTH);
        cx.emit(LibraryChanged);
    }

    // ── Download queue
    // ────────────────────────────────────────────────────────

    /// Reverts a `Downloaded` entry to `Cloud` status.
    ///
    /// This is "Remove Download" — clearing every already-downloaded item in
    /// the entry — not a cancellation of an in-flight fetch. Use
    /// [`Self::cancel_download`] to cancel a queued or in-progress download.
    /// Entry status is never set directly (see
    /// `LibraryItem::recompute_status`): every file's `downloaded` flag is
    /// cleared first, then the entry status is re-derived.
    pub fn remove_download(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(item) = self.catalog.iter_mut().find(|i| i.id.as_ref() == id)
           && item.status == ItemStatus::Downloaded
        {
            for file in &mut item.files {
                file.downloaded = false;
            }
            item.recompute_status();
            self.section_counts =
                section_counts(&self.catalog, now_secs(), self.recently_updated_window_days);
            self.invalidate_cache();
            self.file_info_cache
                .retain(|(entry_id, _), _| entry_id.as_ref() != id);
            cx.emit(LibraryChanged);
        }
    }

    /// Queues a download for every not-yet-downloaded file in the entry with
    /// the given id, dispatching as many as there are free concurrency
    /// slots. For a single-item entry this enqueues its one file, matching
    /// prior behavior; for a multi-item entry it enqueues each missing item
    /// as an independent download.
    ///
    /// No-op for files that are already `Downloaded` or already
    /// queued/in flight — see [`Self::enqueue_item_download`].
    pub fn enqueue_download(&mut self, id: &str, title: impl Into<String>, cx: &mut Context<Self>) {
        let title = title.into();
        let Some(indices) = self.catalog
                                .iter()
                                .find(|i| i.id.as_ref() == id)
                                .map(|item| missing_file_indices(&item.files))
        else {
            return;
        };
        for index in indices {
            self.enqueue_item_download(id, index, title.clone(), cx);
        }
    }

    /// Queues a download for every not-yet-downloaded item that is a member
    /// of the collection with `collection_id`, reusing
    /// [`Self::enqueue_download`] per matching item.
    ///
    /// No-op if the collection id doesn't exist or has no matching catalog
    /// items. Already-downloaded, already-queued, and already-active
    /// items/files are left untouched — see [`Self::enqueue_download`].
    pub fn download_all_for_collection(&mut self, collection_id: u64, cx: &mut Context<Self>) {
        let Some(collection) = self.collections.iter().find(|c| c.id == collection_id)
        else {
            return;
        };
        let member_ids = Arc::clone(&collection.member_ids);
        let targets = collection_download_targets(&self.catalog, &member_ids);
        for (id, title) in targets {
            self.enqueue_download(&id, title.to_string(), cx);
        }
    }

    /// Queues a download for every not-yet-downloaded item under the given
    /// publisher, reusing [`Self::enqueue_download`] per matching item.
    ///
    /// No-op if the publisher has no matching catalog items.
    /// Already-downloaded, already-queued, and already-active items/files
    /// are left untouched — see [`Self::enqueue_download`].
    pub fn download_all_for_publisher(&mut self, publisher: &str, cx: &mut Context<Self>) {
        let targets = publisher_download_targets(&self.catalog, publisher);
        for (id, title) in targets {
            self.enqueue_download(&id, title.to_string(), cx);
        }
    }

    /// Queues a single file's download, dispatching it immediately if a
    /// concurrency slot is free.
    ///
    /// No-op if the item/file does not exist, the file is already
    /// downloaded, or that specific file is already queued/in flight.
    pub fn enqueue_item_download(&mut self, id: &str, index: u32, title: impl Into<String>,
                                 cx: &mut Context<Self>) {
        let Some(item_id) = self.catalog
                                .iter()
                                .find(|i| {
                                    i.id.as_ref() == id
                                    && i.files.get(index as usize).is_some_and(|f| !f.downloaded)
                                })
                                .map(|i| Arc::clone(&i.id))
        else {
            return;
        };
        let key = (Arc::clone(&item_id), index);
        let already_pending = self.download_queue
                                  .iter()
                                  .any(|(qid, qidx, _)| qid.as_ref() == id && *qidx == index)
                              || self.download_activity_ids.contains_key(&key);
        if already_pending {
            return;
        }
        self.download_queue
            .push_back((item_id, index, title.into()));
        self.drain_download_queue(cx);
    }

    /// Gates [`Self::enqueue_download`] behind a free-disk-space check.
    ///
    /// Computes the entry's aggregate missing-file size and compares it to
    /// [`crate::data::storage::available_space_mb`]. If space is sufficient
    /// or can't be determined, queues immediately; otherwise stashes the
    /// request and emits [`LowDiskSpaceWarning`] instead of queuing anything.
    pub fn request_download(&mut self, id: &str, title: impl Into<String>, cx: &mut Context<Self>) {
        let title = title.into();
        let Some(needed_mb) = self.catalog
                                  .iter()
                                  .find(|i| i.id.as_ref() == id)
                                  .map(|item| missing_files_size_mb(&item.files))
        else {
            return;
        };
        if let Some(free_mb) = Self::warn_if_insufficient_space(needed_mb) {
            self.pending_download = Some(PendingDownloadRequest::Item { id: Arc::from(id),
                                                                        title });
            cx.emit(LowDiskSpaceWarning { needed_mb, free_mb });
            return;
        }
        self.enqueue_download(id, title, cx);
    }

    /// Gates [`Self::enqueue_item_download`] behind a free-disk-space check.
    ///
    /// Same pattern as [`Self::request_download`], but for a single file.
    pub fn request_item_download(&mut self, id: &str, index: u32, title: impl Into<String>,
                                 cx: &mut Context<Self>) {
        let title = title.into();
        let Some(needed_mb) = self.catalog
                                  .iter()
                                  .find(|i| i.id.as_ref() == id)
                                  .and_then(|item| item.files.get(index as usize))
                                  .filter(|f| !f.downloaded)
                                  .map(|f| f.size_mb)
        else {
            return;
        };
        if let Some(free_mb) = Self::warn_if_insufficient_space(needed_mb) {
            self.pending_download = Some(PendingDownloadRequest::ItemFile { id: Arc::from(id),
                                                                            index,
                                                                            title });
            cx.emit(LowDiskSpaceWarning { needed_mb, free_mb });
            return;
        }
        self.enqueue_item_download(id, index, title, cx);
    }

    /// Gates [`Self::download_all_for_collection`] behind a free-disk-space
    /// check.
    ///
    /// The check is evaluated once as an aggregate across every matching
    /// item's missing files, not per item — see [`Self::request_download`].
    pub fn request_download_all_for_collection(&mut self, collection_id: u64,
                                               cx: &mut Context<Self>) {
        let Some(collection) = self.collections.iter().find(|c| c.id == collection_id)
        else {
            return;
        };
        let member_ids = Arc::clone(&collection.member_ids);
        let targets = collection_download_targets(&self.catalog, &member_ids);
        let needed_mb = missing_files_size_mb_for_items(&self.catalog,
                                                        targets.iter().map(|(id, _)| id.as_ref()));
        if let Some(free_mb) = Self::warn_if_insufficient_space(needed_mb) {
            self.pending_download = Some(PendingDownloadRequest::Collection { collection_id });
            cx.emit(LowDiskSpaceWarning { needed_mb, free_mb });
            return;
        }
        self.download_all_for_collection(collection_id, cx);
    }

    /// Gates [`Self::download_all_for_publisher`] behind a free-disk-space
    /// check.
    ///
    /// The check is evaluated once as an aggregate across every matching
    /// item's missing files, not per item — see [`Self::request_download`].
    pub fn request_download_all_for_publisher(&mut self, publisher: &str, cx: &mut Context<Self>) {
        let targets = publisher_download_targets(&self.catalog, publisher);
        let needed_mb = missing_files_size_mb_for_items(&self.catalog,
                                                        targets.iter().map(|(id, _)| id.as_ref()));
        if let Some(free_mb) = Self::warn_if_insufficient_space(needed_mb) {
            self.pending_download =
                Some(PendingDownloadRequest::Publisher { publisher: Arc::from(publisher), });
            cx.emit(LowDiskSpaceWarning { needed_mb, free_mb });
            return;
        }
        self.download_all_for_publisher(publisher, cx);
    }

    /// Returns `Some(free_mb)` if `needed_mb` exceeds the free space at the
    /// storage root; `None` if space is sufficient or can't be determined
    /// (fail open — see `download-disk-space-check`).
    fn warn_if_insufficient_space(needed_mb: f64) -> Option<f64> {
        let free_mb = crate::data::storage::available_space_mb()?;
        (needed_mb > free_mb).then_some(free_mb)
    }

    /// Queues the download stashed in `pending_download` (if any) via the
    /// corresponding unconditional method, then clears it.
    ///
    /// Calls the unconditional method directly — never a `request_*`
    /// wrapper — so confirming can't re-trigger the same space check and
    /// loop back into another warning.
    pub fn confirm_pending_download(&mut self, cx: &mut Context<Self>) {
        match self.pending_download.take() {
            Some(PendingDownloadRequest::Item { id, title }) => {
                self.enqueue_download(&id, title, cx);
            }
            Some(PendingDownloadRequest::ItemFile { id, index, title }) => {
                self.enqueue_item_download(&id, index, title, cx);
            }
            Some(PendingDownloadRequest::Collection { collection_id }) => {
                self.download_all_for_collection(collection_id, cx);
            }
            Some(PendingDownloadRequest::Publisher { publisher }) => {
                self.download_all_for_publisher(&publisher, cx);
            }
            None => {}
        }
    }

    /// Clears a stashed `pending_download` without queuing anything.
    pub fn cancel_pending_download(&mut self, _cx: &mut Context<Self>) {
        self.pending_download = None;
    }

    /// Cancels a single file's download that has not yet completed.
    ///
    /// If `(id, index)` is still waiting in `download_queue`, removes it —
    /// no activity entry existed yet for a queued item. If it is actively
    /// fetching (holds a concurrency slot), signals its cancellation flag
    /// and removes its activity panel entry immediately; the in-flight
    /// task observes the flag at its next checkpoint, skips marking the
    /// file `downloaded`, and frees the slot itself (see
    /// [`Self::dispatch_download`]) — `active_downloads` is only ever
    /// decremented there, never here, to avoid double-counting the slot.
    /// Other files queued or in flight for the same entry are unaffected.
    ///
    /// No-op if `(id, index)` is not queued or in flight.
    pub fn cancel_download(&mut self, id: &str, index: u32, cx: &mut Context<Self>) {
        if dequeue_file(&mut self.download_queue, id, index) {
            return;
        }

        let key = (Arc::<str>::from(id), index);
        let Some(flag) = self.download_cancel_flags.remove(&key)
        else {
            return;
        };
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(activity_id) = self.download_activity_ids.remove(&key) {
            self.activity
                .update(cx, |a, cx| a.remove_in_progress(activity_id, cx));
        }
    }

    /// Returns `true` if the file at `(id, index)` is currently queued or
    /// actively downloading.
    ///
    /// Used by the detail tab's per-item list to show an in-progress
    /// indicator on a row instead of its download/downloaded affordance.
    #[must_use]
    pub fn is_file_queued_or_active(&self, id: &str, index: u32) -> bool {
        self.download_queue
            .iter()
            .any(|(qid, qidx, _)| qid.as_ref() == id && *qidx == index)
        || self.download_activity_ids
               .keys()
               .any(|(aid, aidx)| aid.as_ref() == id && *aidx == index)
    }

    /// Dispatches downloads for as many queued files as there are free
    /// concurrency slots (see [`Self::available_slots`]).
    fn drain_download_queue(&mut self, cx: &mut Context<Self>) {
        while self.available_slots() > 0 {
            let Some((item_id, index, title)) = self.download_queue.pop_front()
            else {
                break;
            };
            self.active_downloads += 1;
            self.dispatch_download(item_id, index, title, cx);
        }
    }

    /// Starts a single file's download task. Assumes the caller has already
    /// reserved a concurrency slot by incrementing `active_downloads`.
    ///
    /// Resolves `item.files[index]` to a destination under the configured
    /// storage root, then calls [`LibraryService::download_item`] on the
    /// background executor.
    fn dispatch_download(&mut self, item_id: Arc<str>, index: u32, title: String,
                         cx: &mut Context<Self>) {
        let key = (Arc::clone(&item_id), index);
        let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.download_cancel_flags
            .insert(key.clone(), Arc::clone(&cancel_flag));

        let cancel_fn: Arc<dyn Fn() + Send + Sync> = {
            let flag = Arc::clone(&cancel_flag);
            Arc::new(move || flag.store(true, std::sync::atomic::Ordering::SeqCst))
        };
        // Catalog data isn't `Send` and must stay on this thread; resolve
        // what to fetch (and the file name for the activity label) into
        // owned, Send-safe values before spawning.
        let found = self.catalog
                        .iter()
                        .find(|i| i.id == item_id)
                        .and_then(|item| {
                            item.files.get(index as usize).map(|file| {
                    let dest = resolved_file_path(&StorageConfig::load(), item, file);
                    (item.order_product_id, file.name.to_string(), dest)
                })
                        });

        let label = match &found {
            Some((_, file_name, _)) => t!("activity.downloading_file_entry",
                                          title = title,
                                          file_name = file_name).to_string(),
            None => t!("activity.downloading_file", title = title).to_string(),
        };
        let activity_id = self.activity
                              .update(cx, |a, cx| a.start(&label, Some(cancel_fn), cx));
        self.download_activity_ids.insert(key.clone(), activity_id);

        let weak_activity = self.activity.downgrade();
        let task_item_id = Arc::clone(&item_id);
        let fetch_target = found.map(|(order_product_id, _, dest)| (order_product_id, dest));
        // Captured up front for the failure log below — `fetch_target` itself
        // is moved into the spawned task and consumed by `download_item`.
        let order_product_id_for_log = fetch_target.as_ref().map(|(id, _)| *id);
        let dest_for_log = fetch_target.as_ref()
                                       .map(|(_, dest)| dest.display().to_string());
        // Captured up front for symlinking into `collections/` on success,
        // for the same reason as `dest_for_log` above.
        let dest_for_symlink = fetch_target.as_ref().map(|(_, dest)| dest.clone());
        let service_arc = self.vm.service_arc();
        let title_for_retry = title.clone();

        cx.spawn(async move |this, async_cx| {
              let outcome: Result<(), String> = match fetch_target {
                  Some((order_product_id, dest)) => {
                      let cancel_for_fetch = Arc::clone(&cancel_flag);
                      let (retry_tx, retry_rx) =
                          std::sync::mpsc::channel::<(u32, std::time::Duration)>();
                      let fetch = async_cx.background_executor().spawn(async move {
                                                                    service_arc.download_item(
                              order_product_id,
                              index,
                              &dest,
                              &cancel_for_fetch,
                              Some(&mut |attempt, delay| { retry_tx.send((attempt, delay)).ok(); }),
                          )
                                     .map_err(|e| e.to_string())
                                                                });

                      // Drain retry-progress notifications and update the
                      // activity label until `download_item` returns and drops
                      // `retry_tx`, disconnecting the channel — mirroring the
                      // page-progress channel loop used for catalog fetches.
                      let mut retry_rx = Some(retry_rx);
                      loop {
                          let Some(receiver) = retry_rx.take()
                          else {
                              break;
                          };
                          let (msg, returned_rx) = async_cx.background_executor()
                                                           .spawn(async move {
                                                               let msg = receiver.recv();
                                                               (msg, receiver)
                                                           })
                                                           .await;
                          match msg {
                              Ok((attempt, delay)) => {
                                  let label = t!("activity.downloading_retry",
                                                 title = title_for_retry,
                                                 attempt = attempt,
                                                 delay_secs = delay.as_secs()).to_string();
                                  weak_activity.update(async_cx, |a, cx| {
                                                   a.update_label(activity_id, label, cx)
                                               })
                                               .ok();
                                  retry_rx = Some(returned_rx);
                              }
                              Err(_) => break,
                          }
                      }

                      fetch.await
                  }
                  None => Err("no downloadable file found for this item".to_string()),
              };
              let cancelled = cancel_flag.load(std::sync::atomic::Ordering::SeqCst);

              let activity_id_after =
                  this.update(async_cx, |ctrl, cx| {
                          ctrl.active_downloads = ctrl.active_downloads.saturating_sub(1);
                          let key = (task_item_id.clone(), index);
                          ctrl.download_cancel_flags.remove(&key);
                          let activity_id = ctrl.download_activity_ids.remove(&key);
                          if !cancelled {
                              if let Some(item) =
                                  ctrl.catalog.iter_mut().find(|i| i.id == task_item_id)
                              {
                                  if outcome.is_ok() {
                                      if let Some(file) = item.files.get_mut(index as usize) {
                                          file.downloaded = true;
                                      }
                                      ctrl.file_info_cache
                                          .remove(&(task_item_id.clone(), index as usize));
                                      if let Some(dest) = &dest_for_symlink {
                                          create_collections_symlinks(dest,
                                                                      item.order_product_id,
                                                                      item.product_id,
                                                                      &ctrl.collections);
                                      }
                                  }
                                  item.recompute_status();
                              }
                              ctrl.section_counts =
                                  section_counts(&ctrl.catalog,
                                                 now_secs(),
                                                 ctrl.recently_updated_window_days);
                              ctrl.invalidate_cache();
                              cx.emit(LibraryChanged);
                          }
                          ctrl.drain_download_queue(cx);
                          ctrl.drain_thumbnail_queue(cx);
                          activity_id
                      })
                      .ok()
                      .flatten();

              // Persist the download to the on-disk catalog cache immediately —
              // otherwise a restart before the next live-fetch cache write (see
              // `start_load_inner`) loads a cache that never recorded this
              // download, showing `Cloud` status regardless of what
              // `reconcile_catalog` does (see `catalog-live-data-swap`).
              if !cancelled && outcome.is_ok() {
                  let items_to_save = this.update(async_cx, |ctrl, _cx| ctrl.catalog.clone())
                                          .unwrap_or_default();
                  let save_root = cache_dir();
                  async_cx.background_executor()
                          .spawn(async move {
                              if let Err(e) = save_catalog_cache(&save_root, &items_to_save) {
                                  tracing::warn!(error = %e, "failed to save catalog cache");
                              }
                          })
                          .await;
              }

              if let Err(e) = &outcome
                 && !cancelled
              {
                  tracing::warn!(item_id = %task_item_id,
                                 index,
                                 order_product_id = order_product_id_for_log,
                                 dest = dest_for_log.as_deref().unwrap_or("<unresolved>"),
                                 label = %label,
                                 error = %e,
                                 "download failed");
              }

              if !cancelled && let Some(id) = activity_id_after {
                  match &outcome {
                      Ok(()) => {
                          weak_activity.update(async_cx, |a, cx| a.complete(id, cx))
                                       .ok();
                      }
                      Err(e) => {
                          weak_activity.update(async_cx, |a, cx| a.error(id, e.clone(), cx))
                                       .ok();
                      }
                  }
              }
          })
          .detach();
    }

    // ── Theme / density mutations (dispatched via callbacks)
    // ──────────────────

    /// Applies a new theme key (updates the GPUI global) and persists it via
    /// [`crate::data::ui_preferences::UiPreferences`] so it survives a restart
    /// instead of always resetting to Parchment.
    ///
    /// Also re-syncs `gpui_component::Theme`'s colors (see
    /// [`crate::data::theme::apply_theme_colors`]) so buttons, inputs,
    /// popovers, scrollbars, the sidebar, and the catalog `DataTable` all
    /// track the newly selected Libri palette instead of staying on whichever
    /// palette was active at startup.
    pub fn set_theme(&self, key: ThemeKey, cx: &mut Context<Self>) {
        let current = cx.global::<LibriTheme>();
        let new_theme = LibriTheme::new(key, current.density, current.fonts.clone());
        let colors = new_theme.colors.clone();
        cx.set_global(new_theme);
        cx.update_global::<gpui_component::Theme, _>(|theme, _cx| {
              apply_theme_colors(theme, &colors);
          });
        crate::data::ui_preferences::UiPreferences::load().save_theme_key(key.as_str());
        cx.notify();
    }

    /// Applies a new density (updates the GPUI global).
    pub fn set_density(&self, density: Density, cx: &mut Context<Self>) {
        let current = cx.global::<LibriTheme>();
        let new_theme = LibriTheme::new(current.key, density, current.fonts.clone());
        cx.set_global(new_theme);
        cx.notify();
    }

    /// Switches the active locale (updates `rust_i18n`'s global state) and
    /// persists it via [`crate::data::ui_preferences::UiPreferences`] so it
    /// takes precedence over OS-locale detection on the next launch.
    pub fn set_locale(&self, locale: crate::i18n::Locale, cx: &mut Context<Self>) {
        rust_i18n::set_locale(locale.code());
        crate::data::ui_preferences::UiPreferences::load().save_locale(locale.code());
        cx.notify();
    }

    /// Applies a new body font family (any font installed on the user's
    /// system, not a curated list — see `settings_appearance_view`), updating
    /// the GPUI global, `gpui_component`'s `Theme.font_family`, and
    /// persisting the selection via
    /// [`crate::data::ui_preferences::UiPreferences`].
    pub fn set_body_font(&self, font: impl Into<SharedString>, cx: &mut Context<Self>) {
        let font = font.into();
        let current = cx.global::<LibriTheme>();
        let mut fonts = current.fonts.clone();
        fonts.body_font = font.clone();
        let new_theme = LibriTheme::new(current.key, current.density, fonts);
        cx.set_global(new_theme);
        cx.update_global::<gpui_component::Theme, _>(|theme, _cx| {
              theme.font_family = font.clone();
          });
        crate::data::ui_preferences::UiPreferences::load().save_body_font_name(&font);
        cx.notify();
    }

    /// Applies a new value font family and persists the selection via
    /// [`crate::data::ui_preferences::UiPreferences`].
    pub fn set_value_font(&self, font: impl Into<SharedString>, cx: &mut Context<Self>) {
        let font = font.into();
        let current = cx.global::<LibriTheme>();
        let mut fonts = current.fonts.clone();
        fonts.value_font = font.clone();
        let new_theme = LibriTheme::new(current.key, current.density, fonts);
        cx.set_global(new_theme);
        crate::data::ui_preferences::UiPreferences::load().save_value_font_name(&font);
        cx.notify();
    }

    /// Applies a new label font family and persists the selection via
    /// [`crate::data::ui_preferences::UiPreferences`].
    pub fn set_label_font(&self, font: impl Into<SharedString>, cx: &mut Context<Self>) {
        let font = font.into();
        let current = cx.global::<LibriTheme>();
        let mut fonts = current.fonts.clone();
        fonts.label_font = font.clone();
        let new_theme = LibriTheme::new(current.key, current.density, fonts);
        cx.set_global(new_theme);
        crate::data::ui_preferences::UiPreferences::load().save_label_font_name(&font);
        cx.notify();
    }

    /// Applies a new monospace font family and persists the selection via
    /// [`crate::data::ui_preferences::UiPreferences`].
    pub fn set_mono_font(&self, font: impl Into<SharedString>, cx: &mut Context<Self>) {
        let font = font.into();
        let current = cx.global::<LibriTheme>();
        let mut fonts = current.fonts.clone();
        fonts.mono_font = font.clone();
        let new_theme = LibriTheme::new(current.key, current.density, fonts);
        cx.set_global(new_theme);
        crate::data::ui_preferences::UiPreferences::load().save_mono_font_name(&font);
        cx.notify();
    }

    /// Applies a new shared UI text size (see [`FontSelections::ui_text_size`])
    /// and persists it via [`crate::data::ui_preferences::UiPreferences`].
    /// Applied to each window via `Window::set_rem_size` in
    /// `LibraryRootView::render` and `SettingsWindowView::render`, so every
    /// `rems(...)`-based size utility scales together, like zooming a page.
    pub fn set_ui_text_size(&self, size: Pixels, cx: &mut Context<Self>) {
        let current = cx.global::<LibriTheme>();
        let mut fonts = current.fonts.clone();
        fonts.ui_text_size = size;
        let new_theme = LibriTheme::new(current.key, current.density, fonts);
        cx.set_global(new_theme);
        // Persisted as the scale multiplier shown in Settings > Appearance
        // (e.g. 1.1), not the derived absolute pixel size, so the file's
        // stored value matches what the UI displays.
        let scale = size.as_f32() / crate::data::constants::DEFAULT_UI_TEXT_SIZE;
        crate::data::ui_preferences::UiPreferences::load().save_text_scale(scale);
        cx.notify();
    }

    // ── Helper accessors
    // ──────────────────────────────────────────────────────

    /// Returns the selected `LibraryItem`, if any.
    #[must_use]
    pub fn selected_item(&self) -> Option<&LibraryItem> {
        match &self.selection {
            Selection::Item(id) => self.catalog.iter().find(|i| &i.id == id),
            Selection::None => None,
        }
    }

    /// Total file size of all items in the catalog, in MB.
    #[must_use]
    pub fn total_size_mb(&self) -> f64 {
        self.catalog.iter().map(|i| i.size_mb).sum()
    }
}

/// If the "Create collections" storage setting is enabled, creates a symlink
/// to `dest` under `{root}/collections/{collection name}/` for each of
/// `collections` that the downloaded item belongs to.
///
/// Best-effort: a failure to create any single symlink (or its parent
/// directory) is logged via `tracing::warn!` and does not affect the
/// caller's own success/failure outcome — see
/// [`crate::util::symlink::ensure_symlink`].
fn create_collections_symlinks(dest: &Path, order_product_id: u64, product_id: u64,
                               collections: &[CollectionEntry]) {
    let storage = StorageConfig::load();
    if !storage.create_collections() {
        return;
    }
    let Some(file_name) = dest.file_name()
    else {
        return;
    };
    let root = storage.root_path();
    for collection in collections {
        if !member_ids_contain(&collection.member_ids, order_product_id, product_id) {
            continue;
        }
        let link_dir = crate::data::storage::collection_dir(&root, &collection.name);
        if let Err(e) = std::fs::create_dir_all(&link_dir) {
            tracing::warn!(dir = %link_dir.display(), error = %e,
                           "failed to create collections symlink directory");
            continue;
        }
        let link = link_dir.join(file_name);
        if let Err(e) = crate::util::symlink::ensure_symlink(dest, &link) {
            tracing::warn!(target = %dest.display(), link = %link.display(), error = %e,
                           "failed to create collections symlink");
        }
    }
}

/// Captures a backtrace and returns only the frames from app crates
/// (`dtrpg_*`).
///
/// Each retained frame is one symbol line followed by its `at file:line:col`
/// line. Returns a hint string when `RUST_BACKTRACE` is not set or no app
/// frames are found.
fn app_backtrace() -> String {
    let bt = std::backtrace::Backtrace::capture();
    if bt.status() != std::backtrace::BacktraceStatus::Captured {
        return "<set RUST_BACKTRACE=1 to capture backtraces>".to_string();
    }
    let full = format!("{bt}");
    let mut out: Vec<&str> = Vec::new();
    let mut take_location = false;
    for line in full.lines() {
        if line.trim_start().starts_with("at ") {
            if take_location {
                out.push(line);
                take_location = false;
            }
        }
        else if line.contains("dtrpg_") {
            out.push(line);
            take_location = true;
        }
        else {
            take_location = false;
        }
    }
    if out.is_empty() {
        "<no app frames found in backtrace>".to_string()
    }
    else {
        out.join("\n")
    }
}

// //! Library UI state and interaction controller.

// use crate::app::shell::{AppCommand, AppShell, AppShellState,
// SessionPresentationState}; use crate::services::LibraryItem;
// use crate::services::sdk::RustSdkLibraryService;
// use crate::view_models::library::{LibraryPaneState, LibraryViewModel};

// use crate::ui::library::model::library_data::{
//     FilterScope, LibraryViewMode, MatchPresentation, SortMethod, TreeNode,
// filter_presets,     grouped_items, item_matches, mode_is_grid, mode_label,
// next_sort, root_matches, sort_label,     sorted_flat_items,
// };

// #[derive(Clone, Debug, Eq, PartialEq)]
// pub enum Selection {
//     Publisher(String),
//     ProductType(String),
//     Item(u64),
// }

// #[derive(Clone, Copy, Debug, Eq, PartialEq)]
// pub enum SortPopup {
//     Flat,
//     Outer,
//     Inner,
// }

// /// UI state for the compact DriveThruRPG account menu.
// #[derive(Clone, Debug, Eq, PartialEq)]
// pub struct AccountMenuState {
//     /// User-facing account label shown in the account button/menu.
//     pub display_name: String,
//     /// Human-readable connection or token status.
//     pub connection_status: String,
//     /// Whether an access token is currently configured.
//     pub token_configured: bool,
//     /// Whether the compact account menu is visible.
//     pub menu_open: bool,
// }

// impl AccountMenuState {
//     fn signed_out() -> Self {
//         Self {
//             display_name: "DriveThruRPG account".to_string(),
//             connection_status: "Access token required".to_string(),
//             token_configured: std::env::var("DTRPG_ACCESS_TOKEN").is_ok(),
//             menu_open: false,
//         }
//     }
// }

// /// UI state for low-profile library sync/update reporting.
// #[derive(Clone, Debug, Eq, PartialEq)]
// pub struct SyncStatus {
//     /// Whether a sync or refresh operation is currently active.
//     pub active: bool,
//     /// Human-readable progress summary.
//     pub progress_label: String,
//     /// Human-readable network latency summary.
//     pub latency_label: String,
//     /// Human-readable last-update summary.
//     pub last_update_label: String,
// }

// impl SyncStatus {
//     fn idle() -> Self {
//         Self {
//             active: false,
//             progress_label: "Idle".to_string(),
//             latency_label: "Latency unavailable".to_string(),
//             last_update_label: "Not synced this session".to_string(),
//         }
//     }
// }

// pub struct LibraryController {
//     pub shell: AppShell,
//     pub view_mode: LibraryViewMode,
//     pub filter_scope: FilterScope,
//     pub match_presentation: MatchPresentation,
//     pub flat_sort: SortMethod,
//     pub outer_sort: SortMethod,
//     pub inner_sort: SortMethod,
//     pub filter_query: String,
//     pub search_editing: bool,
//     pub controls_disclosed: bool,
//     pub open_sort_popup: Option<SortPopup>,
//     pub selection: Option<Selection>,
//     pub account: AccountMenuState,
//     pub sync_status: SyncStatus,
// }

// impl LibraryController {
//     pub fn new() -> Self {
//         let service = RustSdkLibraryService::from_environment();
//         let library = LibraryViewModel::new(Box::new(service));

//         let mut shell = AppShell::new(
//             AppShellState {
//                 session: SessionPresentationState::Restoring,
//                 library: LibraryPaneState::Loading,
//                 selected_item_id: None,
//                 status_message: "Loading your library…".to_string(),
//             },
//             library,
//         );

//         shell.dispatch(AppCommand::LoadLibrary);

//         let selection = shell.first_item_id().map(Selection::Item);
//         if let Some(Selection::Item(first)) = selection {
//             shell.dispatch(AppCommand::SelectLibraryItem(first));
//         }

//         Self {
//             shell,
//             view_mode: LibraryViewMode::TreeByPublisher,
//             filter_scope: FilterScope::ChildOnly,
//             match_presentation: MatchPresentation::HideNonMatching,
//             flat_sort: SortMethod::AtoZ,
//             outer_sort: SortMethod::AtoZ,
//             inner_sort: SortMethod::AtoZ,
//             filter_query: String::new(),
//             search_editing: false,
//             open_sort_popup: None,
//             selection,
//             controls_disclosed: true,
//             account: AccountMenuState::signed_out(),
//             sync_status: SyncStatus::idle(),
//         }
//     }

//     pub fn cycle_view_mode(&mut self) {
//         self.view_mode = match self.view_mode {
//             LibraryViewMode::FlatList => LibraryViewMode::TreeByPublisher,
//             LibraryViewMode::TreeByPublisher =>
// LibraryViewMode::TreeByProductType,
// LibraryViewMode::TreeByProductType => LibraryViewMode::GridByPublisher,
//             LibraryViewMode::GridByPublisher =>
// LibraryViewMode::GridByProductType,
// LibraryViewMode::GridByProductType => LibraryViewMode::FlatList,         };
//         self.selection = None;
//         self.shell.dispatch(AppCommand::ClearSelection);
//     }

//     pub fn set_view_mode(&mut self, mode: LibraryViewMode) {
//         if self.view_mode != mode {
//             self.view_mode = mode;
//             self.selection = None;
//             self.shell.dispatch(AppCommand::ClearSelection);
//         }
//     }

//     pub fn cycle_filter_scope(&mut self) {
//         self.filter_scope = match self.filter_scope {
//             FilterScope::ChildOnly => FilterScope::RootAndChild,
//             FilterScope::RootAndChild => FilterScope::RootOnly,
//             FilterScope::RootOnly => FilterScope::ChildOnly,
//         };
//     }

//     pub fn set_filter_scope(&mut self, scope: FilterScope) {
//         self.filter_scope = scope;
//     }

//     pub fn set_match_presentation(&mut self, mode: MatchPresentation) {
//         self.match_presentation = mode;
//     }

//     pub fn toggle_match_presentation(&mut self) {
//         self.match_presentation = match self.match_presentation {
//             MatchPresentation::HideNonMatching =>
// MatchPresentation::HighlightMatching,
// MatchPresentation::HighlightMatching => MatchPresentation::HideNonMatching,
//         };
//     }

//     pub fn cycle_flat_sort(&mut self) {
//         self.flat_sort = next_sort(self.flat_sort);
//     }

//     pub fn set_flat_sort(&mut self, sort: SortMethod) {
//         self.flat_sort = sort;
//         self.open_sort_popup = None;
//     }

//     pub fn cycle_outer_sort(&mut self) {
//         self.outer_sort = next_sort(self.outer_sort);
//     }

//     pub fn set_outer_sort(&mut self, sort: SortMethod) {
//         self.outer_sort = sort;
//         self.open_sort_popup = None;
//     }

//     pub fn cycle_inner_sort(&mut self) {
//         self.inner_sort = next_sort(self.inner_sort);
//     }

//     pub fn set_inner_sort(&mut self, sort: SortMethod) {
//         self.inner_sort = sort;
//         self.open_sort_popup = None;
//     }

//     pub fn toggle_sort_popup(&mut self, popup: SortPopup) {
//         self.open_sort_popup = match self.open_sort_popup {
//             Some(current) if current == popup => None,
//             _ => Some(popup),
//         };
//     }

//     pub fn close_sort_popup(&mut self) {
//         self.open_sort_popup = None;
//     }

//     pub fn toggle_controls_disclosure(&mut self) {
//         self.controls_disclosed = !self.controls_disclosed;
//     }

//     pub fn toggle_account_menu(&mut self) {
//         self.account.menu_open = !self.account.menu_open;
//     }

//     pub fn mark_token_set_action(&mut self) {
//         self.account.token_configured = true;
//         self.account.connection_status = "Access token action
// selected".to_string();         self.account.menu_open = false;
//     }

//     pub fn mark_token_reset_action(&mut self) {
//         self.account.token_configured = false;
//         self.account.connection_status = "Access token reset
// requested".to_string();         self.account.menu_open = false;
//     }

//     pub fn open_settings_action(&mut self) {
//         self.account.connection_status = "Settings action
// selected".to_string();         self.account.menu_open = false;
//     }

//     pub fn cycle_filter_query(&mut self) {
//         let presets = filter_presets();
//         let current = presets
//             .iter()
//             .position(|preset| *preset == self.filter_query)
//             .unwrap_or(0);
//         let next = (current + 1) % presets.len();
//         self.filter_query = presets[next].to_string();
//     }

//     pub fn set_filter_query(&mut self, query: impl Into<String>) {
//         self.filter_query = query.into();
//     }

//     pub fn begin_search_editing(&mut self) {
//         self.search_editing = true;
//     }

//     pub fn end_search_editing(&mut self) {
//         self.search_editing = false;
//     }

//     pub fn append_query_char(&mut self, ch: char) {
//         if !ch.is_control() {
//             self.filter_query.push(ch);
//         }
//     }

//     pub fn backspace_query(&mut self) {
//         self.filter_query.pop();
//     }

//     pub fn clear_filter_query(&mut self) {
//         self.filter_query.clear();
//     }

//     pub fn handle_global_key(&mut self, key: &str, modifiers:
// &gpui::Modifiers) {         if modifiers.secondary() &&
// key.eq_ignore_ascii_case("f") {             self.begin_search_editing();
//             return;
//         }

//         if modifiers.secondary() && key.eq_ignore_ascii_case("l") {
//             self.clear_filter_query();
//             self.begin_search_editing();
//             return;
//         }

//         if key == "/" {
//             self.begin_search_editing();
//             return;
//         }

//         if self.search_editing {
//             if key == "escape" {
//                 self.end_search_editing();
//             } else if key == "backspace" {
//                 self.backspace_query();
//             } else if key.chars().count() == 1
//                 && !modifiers.control
//                 && !modifiers.alt
//                 && !modifiers.platform
//                 && !modifiers.function
//             {
//                 if let Some(ch) = key.chars().next() {
//                     self.append_query_char(ch);
//                 }
//             }
//         }
//     }

//     pub fn refresh(&mut self) {
//         self.sync_status = SyncStatus {
//             active: true,
//             progress_label: "Refreshing library metadata".to_string(),
//             latency_label: "Last request pending".to_string(),
//             last_update_label: "Refresh in progress".to_string(),
//         };

//         self.shell.dispatch(AppCommand::RefreshLibrary);

//         if let Some(Selection::Item(item_id)) = self.selection {
//             self.shell.dispatch(AppCommand::SelectLibraryItem(item_id));
//         }

//         self.sync_status = SyncStatus {
//             active: false,
//             progress_label: "Library metadata current".to_string(),
//             latency_label: "Last request completed".to_string(),
//             last_update_label: "Updated this session".to_string(),
//         };
//     }

//     pub fn set_item_selection(&mut self, item_id: u64) {
//         self.selection = Some(Selection::Item(item_id));
//         self.shell.dispatch(AppCommand::SelectLibraryItem(item_id));
//     }

//     pub fn set_publisher_selection(&mut self, publisher: String) {
//         self.selection = Some(Selection::Publisher(publisher));
//         self.shell.dispatch(AppCommand::ClearSelection);
//     }

//     pub fn set_product_type_selection(&mut self, product_type: String) {
//         self.selection = Some(Selection::ProductType(product_type));
//         self.shell.dispatch(AppCommand::ClearSelection);
//     }

//     pub fn mode_label(&self) -> &'static str {
//         mode_label(self.view_mode)
//     }

//     pub fn flat_sort_label(&self) -> &'static str {
//         sort_label(self.flat_sort)
//     }

//     pub fn outer_sort_label(&self) -> &'static str {
//         sort_label(self.outer_sort)
//     }

//     pub fn inner_sort_label(&self) -> &'static str {
//         sort_label(self.inner_sort)
//     }

//     pub fn controls_summary(&self) -> String {
//         format!(
//             "{} | query: {} | {} | sections: {}",
//             self.mode_label(),
//             self.active_query_label(),
//             self.active_sort_summary(),
//             self.section_count()
//         )
//     }

//     pub fn active_sort_summary(&self) -> String {
//         match self.view_mode {
//             LibraryViewMode::FlatList => format!("sort: {}",
// self.flat_sort_label()),             _ => format!(
//                 "outer: {}, inner: {}",
//                 self.outer_sort_label(),
//                 self.inner_sort_label()
//             ),
//         }
//     }

//     pub fn account_summary(&self) -> String {
//         let token_status = if self.account.token_configured {
//             "token set"
//         } else {
//             "token missing"
//         };

//         format!("{} ({token_status})", self.account.display_name)
//     }

//     pub fn sync_status_summary(&self) -> String {
//         if self.sync_status.active {
//             format!("Syncing: {}", self.sync_status.progress_label)
//         } else {
//             format!("Sync: {}", self.sync_status.progress_label)
//         }
//     }

//     pub fn sync_status_detail(&self) -> String {
//         format!(
//             "{} | {} | {}",
//             self.sync_status.progress_label,
//             self.sync_status.latency_label,
//             self.sync_status.last_update_label
//         )
//     }

//     pub fn view_summary(&self) -> String {
//         format!(
//             "{} total | {} matched | {} sections",
//             self.shell.items().len(),
//             self.filtered_item_count(),
//             self.section_count()
//         )
//     }

//     pub fn match_presentation_label(&self) -> &'static str {
//         match self.match_presentation {
//             MatchPresentation::HideNonMatching => "Search mode: hide
// non-matching",             MatchPresentation::HighlightMatching => "Search
// mode: highlight matches",         }
//     }

//     pub fn active_query_label(&self) -> String {
//         if self.filter_query.is_empty() {
//             "(none)".to_string()
//         } else {
//             self.filter_query.clone()
//         }
//     }

//     pub fn flat_items(&self) -> Vec<LibraryItem> {
//         let mut items = sorted_flat_items(self.shell.items(),
// self.flat_sort);

//         if matches!(self.match_presentation,
// MatchPresentation::HideNonMatching)             &&
// !self.filter_query.is_empty()         {
//             items.retain(|item| item_matches(item, &self.filter_query));
//         }

//         items
//     }

//     pub fn tree_items(&self) -> Vec<TreeNode> {
//         let mut nodes = grouped_items(
//             self.shell.items(),
//             self.view_mode,
//             self.outer_sort,
//             self.inner_sort,
//         );

//         if self.filter_query.is_empty() {
//             return nodes;
//         }

//         if matches!(
//             self.match_presentation,
//             MatchPresentation::HighlightMatching
//         ) {
//             return nodes;
//         }

//         let query = self.filter_query.clone();

//         nodes.retain_mut(|node| {
//             let root_hit = root_matches(&node.root_label, &query);

//             match self.filter_scope {
//                 FilterScope::ChildOnly => {
//                     node.children.retain(|item| item_matches(item, &query));
//                 }
//                 FilterScope::RootAndChild => {
//                     node.children
//                         .retain(|item| root_hit || item_matches(item,
// &query));                 }
//                 FilterScope::RootOnly => {
//                     if !root_hit {
//                         node.children.clear();
//                     }
//                 }
//             }

//             !node.children.is_empty()
//         });

//         nodes
//     }

//     pub fn grid_sections(&self) -> Vec<TreeNode> {
//         self.tree_items()
//     }

//     pub fn is_item_match(&self, item: &LibraryItem) -> bool {
//         item_matches(item, &self.filter_query)
//     }

//     pub fn is_root_match(&self, root_label: &str) -> bool {
//         root_matches(root_label, &self.filter_query)
//     }

//     pub fn filtered_item_count(&self) -> usize {
//         match self.view_mode {
//             LibraryViewMode::FlatList => self.flat_items().len(),
//             _ => self
//                 .tree_items()
//                 .into_iter()
//                 .map(|node| node.children.len())
//                 .sum(),
//         }
//     }

//     pub fn section_count(&self) -> usize {
//         match self.view_mode {
//             LibraryViewMode::FlatList => 0,
//             _ => self.tree_items().len(),
//         }
//     }

//     pub fn renders_grid(&self) -> bool {
//         mode_is_grid(self.view_mode)
//     }

//     pub fn detail_lines(&self) -> Vec<String> {
//         match &self.selection {
//             Some(Selection::Item(item_id)) => {
//                 if let Some(item) = self.shell.items().iter().find(|item|
// item.id == *item_id) {                     return vec![
//                         "Catalog item detail".to_string(),
//                         format!("Title: {}", item.title),
//                         format!("Publisher: {}", item.publisher),
//                         format!("Type: {}", item.product_type),
//                         format!("Added order: {}", item.added_order),
//                         format!("Updated order: {}", item.updated_order),
//                         format!("Summary: {}", item.summary),
//                     ];
//                 }

//                 vec!["Catalog item detail unavailable.".to_string()]
//             }
//             Some(Selection::Publisher(publisher)) => {
//                 let count = self
//                     .shell
//                     .items()
//                     .iter()
//                     .filter(|item| &item.publisher == publisher)
//                     .count();

//                 vec![
//                     "Publisher detail".to_string(),
//                     format!("Publisher: {}", publisher),
//                     format!("Items in library: {}", count),
//                     "Publisher metadata is derived from SDK library
// responses.".to_string(),                 ]
//             }
//             Some(Selection::ProductType(product_type)) => {
//                 let count = self
//                     .shell
//                     .items()
//                     .iter()
//                     .filter(|item| &item.product_type == product_type)
//                     .count();

//                 vec![
//                     "Product type detail".to_string(),
//                     format!("Type: {}", product_type),
//                     format!("Items in library: {}", count),
//                     "Suggested arrangement enabled: tree grouped by product
// type.".to_string(),                 ]
//             }
//             None => vec!["Select a publisher or catalog item to view
// details.".to_string()],         }
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::services::stub::{StubLibraryService, StubMode};

//     fn make_controller() -> LibraryController {
//         let library =
// LibraryViewModel::new(Box::new(StubLibraryService::new(StubMode::Seeded)));
//         let mut shell = AppShell::new(
//             AppShellState {
//                 session: SessionPresentationState::SignedIn,
//                 library: LibraryPaneState::Loading,
//                 selected_item_id: None,
//                 status_message: "Loading your library…".to_string(),
//             },
//             library,
//         );
//         shell.dispatch(AppCommand::LoadLibrary);

//         LibraryController {
//             shell,
//             view_mode: LibraryViewMode::TreeByPublisher,
//             filter_scope: FilterScope::ChildOnly,
//             match_presentation: MatchPresentation::HideNonMatching,
//             flat_sort: SortMethod::AtoZ,
//             outer_sort: SortMethod::AtoZ,
//             inner_sort: SortMethod::AtoZ,
//             filter_query: String::new(),
//             search_editing: false,
//             controls_disclosed: true,
//             open_sort_popup: None,
//             selection: None,
//             account: AccountMenuState::signed_out(),
//             sync_status: SyncStatus::idle(),
//         }
//     }

//     #[test]
//     fn controls_disclosure_preserves_browsing_summary() {
//         let mut controller = make_controller();
//         controller.set_filter_query("atlas");
//         controller.set_view_mode(LibraryViewMode::GridByPublisher);

//         let expanded_summary = controller.controls_summary();
//         controller.toggle_controls_disclosure();

//         assert!(!controller.controls_disclosed);
//         assert_eq!(controller.filter_query, "atlas");
//         assert_eq!(controller.controls_summary(), expanded_summary);
//         assert!(controller.controls_summary().contains("Grid by publisher"));
//     }

//     #[test]
//     fn grid_and_tree_presentations_share_filtered_result_state() {
//         let mut controller = make_controller();
//         controller.set_filter_query("atlas");

//         controller.set_view_mode(LibraryViewMode::TreeByPublisher);
//         let tree_count = controller.filtered_item_count();
//         let tree_sections = controller.section_count();

//         controller.set_view_mode(LibraryViewMode::GridByPublisher);

//         assert!(controller.renders_grid());
//         assert_eq!(controller.filtered_item_count(), tree_count);
//         assert_eq!(controller.section_count(), tree_sections);
//     }

//     #[test]
//     fn account_actions_do_not_store_raw_token_values() {
//         let mut controller = make_controller();

//         controller.toggle_account_menu();
//         assert!(controller.account.menu_open);

//         controller.mark_token_set_action();
//         assert!(controller.account.token_configured);
//         assert!(!controller.account.menu_open);
//         assert!(!controller.account_summary().contains("DTRPG_ACCESS_TOKEN"
// ));

//         controller.mark_token_reset_action();
//         assert!(!controller.account.token_configured);
//         assert!(!controller.account_summary().contains("DTRPG_ACCESS_TOKEN"
// ));     }

//     #[test]
//     fn refresh_updates_sync_status_without_changing_browsing_state() {
//         let mut controller = make_controller();
//         controller.set_filter_query("atlas");
//         controller.set_view_mode(LibraryViewMode::GridByProductType);
//         let summary = controller.controls_summary();

//         controller.refresh();

//         assert!(!controller.sync_status.active);
//         assert_eq!(controller.filter_query, "atlas");
//         assert_eq!(controller.view_mode, LibraryViewMode::GridByProductType);
//         assert_eq!(controller.controls_summary(), summary);
//         assert!(
//             controller
//                 .sync_status_summary()
//                 .contains("Library metadata")
//         );
//     }
// }

#[cfg(test)]
mod advanced_details_tests {
    use std::collections::HashMap;

    use super::toggle_bool_flag;

    #[test]
    fn missing_entry_defaults_to_false() {
        let map: HashMap<&str, bool> = HashMap::new();
        assert!(!map.get("entry-a").copied().unwrap_or(false));
    }

    #[test]
    fn toggle_flips_repeatedly_for_same_key() {
        let mut map: HashMap<&str, bool> = HashMap::new();

        assert!(toggle_bool_flag(&mut map, "entry-a"));
        assert!(!toggle_bool_flag(&mut map, "entry-a"));
        assert!(toggle_bool_flag(&mut map, "entry-a"));
    }

    #[test]
    fn toggle_does_not_affect_other_keys() {
        let mut map: HashMap<&str, bool> = HashMap::new();

        toggle_bool_flag(&mut map, "entry-a");

        assert!(map.get("entry-a").copied().unwrap_or(false));
        assert!(!map.get("entry-b").copied().unwrap_or(false));
    }
}

#[cfg(test)]
mod item_row_selection_tests {
    use std::collections::HashMap;

    use super::toggle_row_selection;

    #[test]
    fn selecting_a_row_selects_only_that_row() {
        // Regression: two files in a bundle can share the same download id
        // (see `LibraryItem::dedupe_files`), so selection must be tracked by
        // row position, not by id, or selecting one row would also select
        // any other row sharing its id.
        let mut map: HashMap<&str, usize> = HashMap::new();

        toggle_row_selection(&mut map, "entry-a", 0);

        assert_eq!(map.get("entry-a").copied(), Some(0));
    }

    #[test]
    fn selecting_a_different_row_switches_the_selection() {
        let mut map: HashMap<&str, usize> = HashMap::new();
        toggle_row_selection(&mut map, "entry-a", 0);

        toggle_row_selection(&mut map, "entry-a", 1);

        assert_eq!(map.get("entry-a").copied(), Some(1));
    }

    #[test]
    fn selecting_the_same_row_again_deselects_it() {
        let mut map: HashMap<&str, usize> = HashMap::new();
        toggle_row_selection(&mut map, "entry-a", 0);

        toggle_row_selection(&mut map, "entry-a", 0);

        assert_eq!(map.get("entry-a"), None);
    }

    #[test]
    fn selection_does_not_affect_other_entries() {
        let mut map: HashMap<&str, usize> = HashMap::new();

        toggle_row_selection(&mut map, "entry-a", 0);
        toggle_row_selection(&mut map, "entry-b", 0);

        assert_eq!(map.get("entry-a").copied(), Some(0));
        assert_eq!(map.get("entry-b").copied(), Some(0));
    }
}

#[cfg(test)]
mod download_queue_tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use super::{dequeue_file, missing_file_indices};
    use crate::data::library::LibraryItemFile;

    fn file(downloaded: bool) -> LibraryItemFile {
        LibraryItemFile { id: "f".into(),
                          index: 0,
                          name: "File".into(),
                          format: "PDF".into(),
                          size_mb: 1.0,
                          downloaded }
    }

    #[test]
    fn missing_file_indices_returns_every_index_when_none_downloaded() {
        let files = vec![file(false), file(false), file(false)];

        assert_eq!(missing_file_indices(&files), vec![0, 1, 2]);
    }

    #[test]
    fn missing_file_indices_skips_already_downloaded_files() {
        let files = vec![file(true), file(false), file(true), file(false)];

        assert_eq!(missing_file_indices(&files), vec![1, 3]);
    }

    #[test]
    fn missing_file_indices_is_empty_when_all_downloaded() {
        let files = vec![file(true), file(true)];

        assert!(missing_file_indices(&files).is_empty());
    }

    #[test]
    fn dequeue_file_removes_only_the_matching_entry() {
        let mut queue: VecDeque<(Arc<str>, u32, String)> =
            VecDeque::from([(Arc::from("entry-a"), 0, "File 0".to_string()),
                            (Arc::from("entry-a"), 1, "File 1".to_string())]);

        let removed = dequeue_file(&mut queue, "entry-a", 0);

        assert!(removed);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].1, 1);
    }

    #[test]
    fn dequeue_file_leaves_other_entries_untouched() {
        let mut queue: VecDeque<(Arc<str>, u32, String)> =
            VecDeque::from([(Arc::from("entry-a"), 0, "A".to_string()),
                            (Arc::from("entry-b"), 0, "B".to_string())]);

        dequeue_file(&mut queue, "entry-a", 0);

        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].0.as_ref(), "entry-b");
    }

    #[test]
    fn dequeue_file_returns_false_when_nothing_matches() {
        let mut queue: VecDeque<(Arc<str>, u32, String)> =
            VecDeque::from([(Arc::from("entry-a"), 0, "A".to_string())]);

        let removed = dequeue_file(&mut queue, "entry-a", 5);

        assert!(!removed);
        assert_eq!(queue.len(), 1);
    }
}

#[cfg(test)]
mod disk_space_check_tests {
    use super::{missing_files_size_mb, missing_files_size_mb_for_items};
    use crate::data::enums::ItemStatus;
    use crate::data::library::{LibraryItem, LibraryItemFile};

    fn file(size_mb: f64, downloaded: bool) -> LibraryItemFile {
        LibraryItemFile { id: "f".into(),
                          index: 0,
                          name: "File".into(),
                          format: "PDF".into(),
                          size_mb,
                          downloaded }
    }

    fn item(id: &str, files: Vec<LibraryItemFile>) -> LibraryItem {
        let mut item = LibraryItem::new(id,
                                        "Title",
                                        "Pub",
                                        "Line",
                                        "Core",
                                        "PDF",
                                        0,
                                        0.0,
                                        2020,
                                        0,
                                        ItemStatus::Cloud,
                                        "#000000",
                                        "",
                                        None);
        item.files = files;
        item
    }

    #[test]
    fn missing_files_size_mb_sums_only_not_yet_downloaded_files() {
        let files = vec![file(10.0, false), file(5.0, true), file(2.5, false)];

        assert_eq!(missing_files_size_mb(&files), 12.5);
    }

    #[test]
    fn missing_files_size_mb_is_zero_when_all_downloaded() {
        let files = vec![file(10.0, true), file(5.0, true)];

        assert_eq!(missing_files_size_mb(&files), 0.0);
    }

    #[test]
    fn missing_files_size_mb_for_items_sums_across_matching_items_only() {
        let catalog = vec![item("a", vec![file(10.0, false), file(5.0, true)]),
                           item("b", vec![file(3.0, false)]),
                           item("c", vec![file(100.0, false)])];

        let total = missing_files_size_mb_for_items(&catalog, ["a", "b"].into_iter());

        assert_eq!(total, 13.0);
    }

    #[test]
    fn missing_files_size_mb_for_items_ignores_unknown_ids() {
        let catalog = vec![item("a", vec![file(10.0, false)])];

        let total = missing_files_size_mb_for_items(&catalog, ["nonexistent"].into_iter());

        assert_eq!(total, 0.0);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod file_info_cache_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::cached_file_info;
    use crate::util::file_info::FileInfo;

    #[test]
    fn undownloaded_file_returns_none_without_caching() {
        let mut cache = HashMap::new();
        let entry_id: Arc<str> = Arc::from("entry-a");
        let path = std::path::Path::new("/does/not/exist.pdf");

        let info = cached_file_info(&mut cache, &entry_id, 0, false, path, "PDF");

        assert_eq!(info, FileInfo::None);
        assert!(cache.is_empty());
    }

    #[test]
    fn downloaded_unsupported_format_caches_none() {
        let mut cache = HashMap::new();
        let entry_id: Arc<str> = Arc::from("entry-a");
        let path = std::path::Path::new("/does/not/exist.txt");

        let info = cached_file_info(&mut cache, &entry_id, 0, true, path, "TXT");

        assert_eq!(info, FileInfo::None);
        assert_eq!(cache.get(&(Arc::clone(&entry_id), 0)),
                   Some(&FileInfo::None));
    }

    #[test]
    fn second_call_reuses_cached_value_without_re_reading_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.png");
        image::RgbImage::new(5, 6).save(&path).unwrap();
        let mut cache = HashMap::new();
        let entry_id: Arc<str> = Arc::from("entry-a");

        let first = cached_file_info(&mut cache, &entry_id, 0, true, &path, "PNG");
        assert_eq!(first, FileInfo::Dimensions(5, 6));

        // Delete the underlying file: if the second call re-read from disk,
        // extraction would fail and return `FileInfo::None` instead of the
        // cached dimensions.
        std::fs::remove_file(&path).unwrap();

        let second = cached_file_info(&mut cache, &entry_id, 0, true, &path, "PNG");
        assert_eq!(second, FileInfo::Dimensions(5, 6));
    }

    #[test]
    fn different_rows_cache_independently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.png");
        image::RgbImage::new(2, 2).save(&path).unwrap();
        let mut cache = HashMap::new();
        let entry_id: Arc<str> = Arc::from("entry-a");

        cached_file_info(&mut cache, &entry_id, 0, true, &path, "PNG");

        assert_eq!(cache.get(&(Arc::clone(&entry_id), 1)), None);
    }
}

#[cfg(test)]
mod bulk_download_target_tests {
    use super::{collection_download_targets, publisher_download_targets};
    use crate::data::enums::ItemStatus;
    use crate::data::library::LibraryItem;

    fn item(id: &str, title: &str, publisher: &str, order_product_id: u64, product_id: u64)
            -> LibraryItem {
        let mut item = LibraryItem::new(id,
                                        title,
                                        publisher,
                                        "Line",
                                        "Core",
                                        "PDF",
                                        0,
                                        0.0,
                                        2020,
                                        0,
                                        ItemStatus::Cloud,
                                        "#000000",
                                        "",
                                        None);
        item.order_product_id = order_product_id;
        item.product_id = product_id;
        item
    }

    #[test]
    fn collection_download_targets_includes_every_member() {
        let catalog = vec![item("b1", "Book One", "Pub", 1, 0),
                           item("b2", "Book Two", "Pub", 2, 0),
                           item("b3", "Not A Member", "Pub", 3, 0)];

        let targets = collection_download_targets(&catalog, &[1, 2]);

        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].0.as_ref(), "b1");
        assert_eq!(targets[1].0.as_ref(), "b2");
    }

    #[test]
    fn collection_download_targets_matches_on_product_id_fallback() {
        let catalog = vec![item("b1", "Book One", "Pub", 0, 42)];

        let targets = collection_download_targets(&catalog, &[42]);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0.as_ref(), "b1");
    }

    #[test]
    fn collection_download_targets_is_empty_for_unknown_collection() {
        let catalog = vec![item("b1", "Book One", "Pub", 1, 0)];

        let targets = collection_download_targets(&catalog, &[999]);

        assert!(targets.is_empty());
    }

    #[test]
    fn publisher_download_targets_includes_every_matching_item() {
        let catalog = vec![item("b1", "Book One", "Acme", 1, 0),
                           item("b2", "Book Two", "Acme", 2, 0),
                           item("b3", "Other Publisher", "Other Co", 3, 0)];

        let targets = publisher_download_targets(&catalog, "Acme");

        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].0.as_ref(), "b1");
        assert_eq!(targets[1].0.as_ref(), "b2");
    }

    #[test]
    fn publisher_download_targets_is_empty_for_unknown_publisher() {
        let catalog = vec![item("b1", "Book One", "Acme", 1, 0)];

        let targets = publisher_download_targets(&catalog, "Nobody");

        assert!(targets.is_empty());
    }
}

#[cfg(test)]
mod reconcile_catalog_tests {
    use super::reconcile_catalog;
    use crate::data::enums::ItemStatus;
    use crate::data::library::{LibraryItem, LibraryItemFile};

    fn item(id: &str, title: &str) -> LibraryItem {
        LibraryItem::new(id,
                         title,
                         "Publisher",
                         "Line",
                         "Core",
                         "PDF",
                         0,
                         0.0,
                         2020,
                         0,
                         ItemStatus::Cloud,
                         "#000000",
                         "",
                         None)
    }

    fn file(id: &str, downloaded: bool) -> LibraryItemFile {
        LibraryItemFile { id: id.into(),
                          index: 0,
                          name: "File".into(),
                          format: "PDF".into(),
                          size_mb: 1.0,
                          downloaded }
    }

    #[test]
    fn item_present_in_both_is_refreshed_and_marked_available() {
        let mut local = item("b1", "Old Title");
        local.is_available = false;
        let live = item("b1", "New Title");

        let reconciled = reconcile_catalog(vec![local], vec![live]);

        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].title.as_ref(), "New Title");
        assert!(reconciled[0].is_available);
    }

    #[test]
    fn item_only_local_is_kept_and_flagged_unavailable() {
        let local = item("b1", "Local Only");

        let reconciled = reconcile_catalog(vec![local], vec![]);

        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].id.as_ref(), "b1");
        assert!(!reconciled[0].is_available);
    }

    #[test]
    fn item_only_live_is_appended_and_available() {
        let live = item("b1", "New Item");

        let reconciled = reconcile_catalog(vec![], vec![live]);

        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].id.as_ref(), "b1");
        assert!(reconciled[0].is_available);
    }

    #[test]
    fn previously_unavailable_item_reappearing_clears_the_flag() {
        let mut local = item("b1", "Was Gone");
        local.is_available = false;
        let live = item("b1", "Was Gone");

        let reconciled = reconcile_catalog(vec![local], vec![live]);

        assert!(reconciled[0].is_available);
    }

    #[test]
    fn existing_catalog_order_is_preserved_for_known_items() {
        let existing = vec![item("b1", "First"), item("b2", "Second")];
        let live = vec![item("b2", "Second Updated"), item("b1", "First Updated")];

        let reconciled = reconcile_catalog(existing, live);

        assert_eq!(reconciled.len(), 2);
        assert_eq!(reconciled[0].id.as_ref(), "b1");
        assert_eq!(reconciled[1].id.as_ref(), "b2");
    }

    #[test]
    fn downloaded_status_survives_reconcile_against_a_live_fetch() {
        let mut local = item("b1", "Old Title");
        local.files = vec![file("f1", true), file("f2", true)];
        local.recompute_status();
        assert_eq!(local.status, ItemStatus::Downloaded);

        let mut live = item("b1", "New Title");
        live.files = vec![file("f1", false), file("f2", false)];

        let reconciled = reconcile_catalog(vec![local], vec![live]);

        assert!(reconciled[0].files.iter().all(|f| f.downloaded));
        assert_eq!(reconciled[0].status, ItemStatus::Downloaded);
    }

    #[test]
    fn live_file_with_no_cached_counterpart_is_not_downloaded() {
        let mut local = item("b1", "Title");
        local.files = vec![file("f1", true)];

        let mut live = item("b1", "Title");
        live.files = vec![file("f1", false), file("f2", false)];

        let reconciled = reconcile_catalog(vec![local], vec![live]);

        let f2_downloaded = reconciled[0].files
                                         .iter()
                                         .find(|f| f.id.as_ref() == "f2")
                                         .map(|f| f.downloaded);
        assert_eq!(f2_downloaded, Some(false));
    }

    #[test]
    fn partially_downloaded_item_keeps_its_per_file_state() {
        let mut local = item("b1", "Title");
        local.files = vec![file("f1", true), file("f2", false)];

        let mut live = item("b1", "Title");
        live.files = vec![file("f1", false), file("f2", false)];

        let reconciled = reconcile_catalog(vec![local], vec![live]);

        let f1_downloaded = reconciled[0].files
                                         .iter()
                                         .find(|f| f.id.as_ref() == "f1")
                                         .map(|f| f.downloaded);
        let f2_downloaded = reconciled[0].files
                                         .iter()
                                         .find(|f| f.id.as_ref() == "f2")
                                         .map(|f| f.downloaded);
        assert_eq!(f1_downloaded, Some(true));
        assert_eq!(f2_downloaded, Some(false));
        assert_eq!(reconciled[0].status, ItemStatus::Cloud);
    }
}

#[cfg(test)]
mod reload_cooldown_tests {
    use super::reload_cooldown_active;
    use crate::data::catalog_cache::CacheMetadata;

    fn meta(saved_at_secs: u64) -> CacheMetadata {
        CacheMetadata { saved_at_secs,
                        item_count: 10,
                        schema_version: 2,
                        last_item_check_batch_secs: None,
                        last_fresh_install_request_secs: None }
    }

    #[test]
    fn reload_is_suppressed_when_cache_is_recent() {
        let now = 1_000;
        let recent = meta(now - 10); // 10s ago, within the 60s cooldown

        assert!(reload_cooldown_active(Some(&recent), now));
    }

    #[test]
    fn reload_proceeds_when_cache_is_older_than_cooldown() {
        let now = 1_000;
        let old = meta(now - 61); // just past the 60s cooldown

        assert!(!reload_cooldown_active(Some(&old), now));
    }

    #[test]
    fn reload_proceeds_when_no_metadata_exists() {
        let now = 1_000;

        assert!(!reload_cooldown_active(None, now));
    }
}

#[cfg(test)]
mod fresh_install_gating_tests {
    use super::fresh_install_request_gated;

    #[test]
    fn request_is_suppressed_within_the_minimum_interval() {
        let now = 1_000;
        assert!(fresh_install_request_gated(Some(now - 10), now)); // 10s ago, within 60s
    }

    #[test]
    fn request_proceeds_once_the_minimum_interval_elapses() {
        let now = 1_000;
        assert!(!fresh_install_request_gated(Some(now - 61), now)); // just past 60s
    }

    #[test]
    fn request_proceeds_when_never_recorded() {
        let now = 1_000;
        assert!(!fresh_install_request_gated(None, now));
    }
}

#[cfg(test)]
mod item_check_tests {
    use std::time::{Duration, SystemTime};

    use super::{apply_check_result, item_check_due};
    use crate::data::enums::ItemStatus;
    use crate::data::library::{LibraryItem, LibraryItemFile};
    use crate::services::{LibraryServiceError, LibraryServiceErrorKind};

    fn item(id: &str, title: &str) -> LibraryItem {
        LibraryItem::new(id,
                         title,
                         "Publisher",
                         "Line",
                         "Core",
                         "PDF",
                         0,
                         0.0,
                         2020,
                         0,
                         ItemStatus::Cloud,
                         "#000000",
                         "",
                         None)
    }

    fn file(id: &str, downloaded: bool) -> LibraryItemFile {
        LibraryItemFile { id: id.into(),
                          index: 0,
                          name: format!("{id}.pdf").into(),
                          format: "PDF".into(),
                          size_mb: 1.0,
                          downloaded }
    }

    #[test]
    fn check_is_due_when_never_checked() {
        assert!(item_check_due(None));
    }

    #[test]
    fn check_is_suppressed_within_cooldown() {
        assert!(!item_check_due(Some(SystemTime::now())));
    }

    #[test]
    fn check_is_due_once_cooldown_elapses() {
        let long_ago = SystemTime::now() - Duration::from_secs(600);

        assert!(item_check_due(Some(long_ago)));
    }

    #[test]
    fn ok_result_refreshes_fields_and_marks_available() {
        let mut existing = item("b1", "Old Title");
        existing.is_available = false;
        let fresh = item("b1", "New Title");
        let now = SystemTime::now();

        apply_check_result(&mut existing, Ok(fresh), now);

        assert_eq!(existing.title.as_ref(), "New Title");
        assert!(existing.is_available);
        assert_eq!(existing.availability_last_checked, Some(now));
    }

    #[test]
    fn ok_result_preserves_identity_and_membership_ids_even_if_the_fresh_response_differs() {
        // Regression: a single-item availability re-check must never let the
        // fresh response clobber `numeric_id`/`order_product_id`/`product_id` —
        // those drive `collection_member_id` lookups, and a mismatched or
        // zeroed value from the single-item endpoint silently broke collection
        // membership display for the checked item.
        let mut existing = item("b1", "Title");
        existing.numeric_id = 111;
        existing.order_product_id = 222;
        existing.product_id = 333;
        let mut fresh = item("b1", "Title");
        fresh.numeric_id = 999;
        fresh.order_product_id = 0;
        fresh.product_id = 0;

        apply_check_result(&mut existing, Ok(fresh), SystemTime::now());

        assert_eq!(existing.numeric_id, 111);
        assert_eq!(existing.order_product_id, 222);
        assert_eq!(existing.product_id, 333);
    }

    #[test]
    fn not_found_result_marks_unavailable_without_removing() {
        let mut existing = item("b1", "Still Here");
        let now = SystemTime::now();

        apply_check_result(&mut existing,
                           Err(LibraryServiceError::new(LibraryServiceErrorKind::NotFound,
                                                        "not found")),
                           now);

        assert_eq!(existing.title.as_ref(), "Still Here");
        assert!(!existing.is_available);
        assert_eq!(existing.availability_last_checked, Some(now));
    }

    #[test]
    fn transient_error_leaves_item_and_flag_unchanged() {
        let mut existing = item("b1", "Unchanged");
        existing.is_available = true;
        let now = SystemTime::now();

        apply_check_result(&mut existing,
                           Err(LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                                        "network error")),
                           now);

        assert!(existing.is_available);
        assert_eq!(existing.availability_last_checked, None);
    }

    #[test]
    fn ok_result_preserves_downloaded_files_and_keeps_status_downloaded() {
        // Regression: a single-item availability re-check must not let the
        // fresh server response (which never reports `downloaded` — that's a
        // local-only fact) wipe files already marked downloaded on disk. See
        // `verify-downloaded-status-against-disk`, whose on-demand disk
        // verification races this same check on item selection.
        let mut existing = item("b1", "Title");
        existing.files = vec![file("f1", true), file("f2", true)];
        existing.recompute_status();
        assert_eq!(existing.status, ItemStatus::Downloaded);

        let mut fresh = item("b1", "Title");
        fresh.files = vec![file("f1", false), file("f2", false)];

        apply_check_result(&mut existing, Ok(fresh), SystemTime::now());

        assert!(existing.files.iter().all(|f| f.downloaded));
        assert_eq!(existing.status, ItemStatus::Downloaded);
    }

    #[test]
    fn ok_result_defaults_a_new_file_id_to_not_downloaded() {
        let mut existing = item("b1", "Title");
        existing.files = vec![file("f1", true)];
        existing.recompute_status();

        let mut fresh = item("b1", "Title");
        fresh.files = vec![file("f1", false), file("f2", false)];

        apply_check_result(&mut existing, Ok(fresh), SystemTime::now());

        assert!(existing.files[0].downloaded);
        assert!(!existing.files[1].downloaded);
        assert_eq!(existing.status, ItemStatus::Cloud);
    }

    #[test]
    fn ok_result_preserves_partial_download_and_keeps_status_cloud() {
        let mut existing = item("b1", "Title");
        existing.files = vec![file("f1", true), file("f2", false)];
        existing.recompute_status();
        assert_eq!(existing.status, ItemStatus::Cloud);

        let mut fresh = item("b1", "Title");
        fresh.files = vec![file("f1", false), file("f2", false)];

        apply_check_result(&mut existing, Ok(fresh), SystemTime::now());

        assert!(existing.files[0].downloaded);
        assert!(!existing.files[1].downloaded);
        assert_eq!(existing.status, ItemStatus::Cloud);
    }
}

#[cfg(test)]
mod concurrency_tests {
    use super::remaining_slots;

    #[test]
    fn full_limit_available_when_nothing_is_active() {
        assert_eq!(remaining_slots(3, 0, 0), 3);
    }

    #[test]
    fn thumbnail_and_download_activity_share_one_limit() {
        // 1 thumbnail fetch + 1 download active against a limit of 3 leaves
        // exactly 1 slot, not 2 — they draw from the same aggregate count.
        assert_eq!(remaining_slots(3, 1, 1), 1);
    }

    #[test]
    fn no_slots_available_at_the_limit() {
        assert_eq!(remaining_slots(3, 2, 1), 0);
    }

    #[test]
    fn never_exceeds_the_limit_even_if_active_counts_somehow_overrun_it() {
        // Defensive: `saturating_sub` must not wrap/panic if active counts
        // ever exceed the configured limit (e.g. a limit lowered mid-batch).
        assert_eq!(remaining_slots(3, 4, 2), 0);
    }

    #[test]
    fn total_active_fetches_never_exceeds_the_limit_across_repeated_dispatch() {
        // Simulates repeatedly dispatching from both queues until neither has
        // room, verifying the running total of "active" slots taken never
        // exceeds `max_concurrent_downloads`.
        let max = 3;
        let mut active_thumbnail_fetches = 0;
        let mut active_downloads = 0;
        let mut thumbnail_queue_len = 5;
        let mut download_queue_len = 5;

        loop {
            let slots = remaining_slots(max, active_thumbnail_fetches, active_downloads);
            if slots == 0 || (thumbnail_queue_len == 0 && download_queue_len == 0) {
                break;
            }
            if thumbnail_queue_len > 0 {
                thumbnail_queue_len -= 1;
                active_thumbnail_fetches += 1;
            }
            else if download_queue_len > 0 {
                download_queue_len -= 1;
                active_downloads += 1;
            }
            assert!(active_thumbnail_fetches + active_downloads <= max,
                    "active fetches exceeded max_concurrent_downloads");
        }
    }
}

#[cfg(test)]
mod check_queue_tests {
    use std::collections::{HashSet, VecDeque};
    use std::sync::Arc;

    use super::should_enqueue_check;

    #[test]
    fn id_not_queued_or_in_flight_should_be_enqueued() {
        let queue: VecDeque<Arc<str>> = VecDeque::new();
        let checking: HashSet<Arc<str>> = HashSet::new();

        assert!(should_enqueue_check(&queue, &checking, &Arc::from("b1")));
    }

    #[test]
    fn id_already_queued_is_not_re_enqueued() {
        let queue: VecDeque<Arc<str>> = VecDeque::from([Arc::from("b1")]);
        let checking: HashSet<Arc<str>> = HashSet::new();

        assert!(!should_enqueue_check(&queue, &checking, &Arc::from("b1")));
    }

    #[test]
    fn id_already_in_flight_is_not_re_enqueued() {
        let queue: VecDeque<Arc<str>> = VecDeque::new();
        let checking: HashSet<Arc<str>> = HashSet::from([Arc::from("b1")]);

        assert!(!should_enqueue_check(&queue, &checking, &Arc::from("b1")));
    }
}

#[cfg(test)]
mod check_batch_tests {
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    use super::{check_batch_cooldown_active, select_check_batch};
    use crate::data::enums::ItemStatus;
    use crate::data::library::LibraryItem;

    fn item(id: &str) -> LibraryItem {
        LibraryItem::new(id,
                         "Title",
                         "Publisher",
                         "Line",
                         "Core",
                         "PDF",
                         0,
                         0.0,
                         2020,
                         0,
                         ItemStatus::Cloud,
                         "#000000",
                         "",
                         None)
    }

    #[test]
    fn batch_is_suppressed_when_a_recent_batch_was_already_enqueued() {
        let now = 10_000;
        assert!(check_batch_cooldown_active(Some(now - 10), now));
    }

    #[test]
    fn batch_proceeds_once_cooldown_elapses() {
        let now = 10_000;
        assert!(!check_batch_cooldown_active(Some(now - 901), now));
    }

    #[test]
    fn batch_proceeds_when_no_prior_batch_recorded() {
        assert!(!check_batch_cooldown_active(None, 10_000));
    }

    #[test]
    fn batch_selection_prefers_oldest_checked_items_first() {
        let mut never_checked = item("b1");
        never_checked.availability_last_checked = None;
        let mut checked_long_ago = item("b2");
        checked_long_ago.availability_last_checked =
            Some(SystemTime::now() - Duration::from_secs(10_000));
        let mut checked_recently_within_cooldown = item("b3");
        checked_recently_within_cooldown.availability_last_checked = Some(SystemTime::now());

        let catalog = [checked_recently_within_cooldown,
                       checked_long_ago.clone(),
                       never_checked.clone()];
        let batch = select_check_batch(&catalog, 10);

        // The recently-checked item is within its cooldown and not due, so it's
        // excluded; the never-checked item sorts before the long-ago-checked
        // one.
        assert_eq!(batch,
                   vec![Arc::clone(&never_checked.id),
                        Arc::clone(&checked_long_ago.id)]);
    }

    #[test]
    fn batch_selection_is_bounded_by_limit() {
        let catalog: Vec<LibraryItem> = (0..10).map(|i| item(&format!("b{i}"))).collect();

        let batch = select_check_batch(&catalog, 3);

        assert_eq!(batch.len(), 3);
    }
}

#[cfg(test)]
mod partial_fetch_tests {
    use super::{merge_partial_fetch, partial_fetch_since, should_attempt_partial_fetch};
    use crate::data::enums::ItemStatus;
    use crate::data::library::LibraryItem;

    fn item(id: &str, title: &str) -> LibraryItem {
        LibraryItem::new(id,
                         title,
                         "Publisher",
                         "Line",
                         "Core",
                         "PDF",
                         0,
                         0.0,
                         2020,
                         0,
                         ItemStatus::Cloud,
                         "#000000",
                         "",
                         None)
    }

    #[test]
    fn growth_only_mismatch_triggers_partial_fetch() {
        assert!(should_attempt_partial_fetch(12, 10));
    }

    #[test]
    fn decrease_does_not_trigger_partial_fetch() {
        assert!(!should_attempt_partial_fetch(8, 10));
    }

    #[test]
    fn matching_count_does_not_trigger_partial_fetch() {
        assert!(!should_attempt_partial_fetch(10, 10));
    }

    #[test]
    fn partial_fetch_merges_new_and_refreshed_items_without_flagging_unavailable() {
        let mut existing_a = item("b1", "Old Title");
        existing_a.is_available = false; // already unavailable before the partial fetch
        let existing_b = item("b2", "Unrelated");
        let refreshed_a = item("b1", "New Title");
        let new_c = item("b3", "Brand New");

        let merged = merge_partial_fetch(vec![existing_a, existing_b.clone()],
                                         vec![refreshed_a, new_c]);

        assert_eq!(merged.len(), 3);
        match merged.iter().find(|i| i.id.as_ref() == "b1") {
            Some(a) => {
                assert_eq!(a.title.as_ref(), "New Title");
                assert!(a.is_available); // refreshed via the partial fetch, so flag clears
            }
            None => panic!("expected item b1 to be present"),
        }
        match merged.iter().find(|i| i.id.as_ref() == "b3") {
            Some(c) => assert!(c.is_available),
            None => panic!("expected item b3 to be present"),
        }
    }

    #[test]
    fn partial_fetch_leaves_items_absent_from_the_response_untouched() {
        let mut existing = item("b1", "Untouched");
        existing.is_available = false;

        let merged = merge_partial_fetch(vec![existing], vec![]);

        // Absence from a partial response proves nothing — the flag must not
        // change.
        assert!(!merged[0].is_available);
    }

    #[test]
    fn since_is_derived_from_the_most_recent_timestamp() {
        let mut older = item("b1", "Older");
        older.date_updated = Some(1_700_000_000);
        let mut newer = item("b2", "Newer");
        newer.date_updated = Some(1_800_000_000);

        let since = partial_fetch_since(&[older, newer]);

        assert_eq!(since.as_deref(), Some("2027-01-15T08:00:00+00:00"));
    }

    #[test]
    fn since_is_none_for_an_empty_catalog() {
        assert_eq!(partial_fetch_since(&[]), None);
    }
}
