//! Pure helper functions used by the library controller and its tests.

use super::*;

/// Flips a per-key boolean flag in `map`, treating a missing entry as `false`.
///
/// Used by per-entry ephemeral UI toggles (e.g. the "Other details"
/// disclosure) that default to a collapsed/off state until first toggled.
pub(crate) fn toggle_bool_flag<K: std::hash::Hash + Eq>(
    map: &mut HashMap<K, bool>, key: K,
) -> bool {
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
pub(crate) fn cached_file_info(
    cache: &mut HashMap<(Arc<str>, usize), crate::util::file_info::FileInfo>, entry_id: &Arc<str>,
    row_ix: usize, downloaded: bool, path: &Path, format: &str,
) -> crate::util::file_info::FileInfo {
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
pub(crate) fn toggle_row_selection<K: std::hash::Hash + Eq>(
    map: &mut HashMap<K, usize>, key: K, row_ix: usize,
) {
    if map.get(&key) == Some(&row_ix) {
        map.remove(&key);
    } else {
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
pub(crate) fn reconcile_catalog(
    existing: Vec<LibraryItem>, live: Vec<LibraryItem>,
) -> Vec<LibraryItem> {
    let mut live_by_id: HashMap<Arc<str>, LibraryItem> = live
        .into_iter()
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
            live_id_by_product_id
                .entry(item.product_id)
                .or_insert_with(|| Arc::clone(&item.id));
        }
    }

    let mut reconciled: Vec<LibraryItem> = existing
        .into_iter()
        .map(|mut item| {
            let matched_id = if live_by_id.contains_key(&item.id) {
                Some(Arc::clone(&item.id))
            } else if item.product_id != 0 {
                live_id_by_product_id.get(&item.product_id).cloned()
            } else {
                None
            };

            if let Some(mut live_item) = matched_id.and_then(|id| live_by_id.remove(&id)) {
                let downloaded_by_id: HashMap<Arc<str>, bool> = item
                    .files
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
            } else {
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
pub(crate) fn dedupe_by_product_id(items: Vec<LibraryItem>) -> Vec<LibraryItem> {
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
pub(crate) fn missing_file_indices(files: &[LibraryItemFile]) -> Vec<u32> {
    files
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.downloaded)
        .map(|(idx, _)| idx as u32)
        .collect()
}

/// Sums `size_mb` across every not-yet-downloaded file in `files`.
///
/// Used by the `request_*` gating wrappers to compute the aggregate size a
/// queuing action would enqueue, for comparison against
/// [`dtrpg_core::data::storage::available_space_mb`].
pub(crate) fn missing_files_size_mb(files: &[LibraryItemFile]) -> f64 {
    files
        .iter()
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
pub(crate) fn missing_files_size_mb_for_items<'a>(
    catalog: &[LibraryItem], ids: impl Iterator<Item = &'a str>,
) -> f64 {
    let ids: HashSet<&str> = ids.collect();
    catalog
        .iter()
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
pub(crate) fn collection_download_targets(
    catalog: &[LibraryItem], member_ids: &[u64],
) -> Vec<(Arc<str>, Arc<str>)> {
    catalog
        .iter()
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
pub(crate) fn publisher_download_targets(
    catalog: &[LibraryItem], publisher: &str,
) -> Vec<(Arc<str>, Arc<str>)> {
    catalog
        .iter()
        .filter(|item| item.publisher.as_ref() == publisher)
        .map(|item| (Arc::clone(&item.id), Arc::clone(&item.title)))
        .collect()
}

/// Returns `(id, title)` for every `catalog` item matching `filter`, in
/// catalog order — the same predicate [`LibraryController::visible_items`]
/// uses, so "download all" for a sidebar smart filter targets exactly what
/// that filter currently shows.
///
/// Used by [`LibraryController::download_all_for_filter`] to determine which
/// items to pass to [`LibraryController::enqueue_download`]. Filter matching
/// (not download status) is this function's only concern —
/// already-downloaded items are filtered out downstream by
/// [`enqueue_download`](LibraryController::enqueue_download) via
/// [`missing_file_indices`], not here.
pub(crate) fn filter_download_targets(
    catalog: &[LibraryItem], filter: &SidebarFilter, collection_members: &HashSet<u64>,
    now_secs: i64, window_days: u32,
) -> Vec<(Arc<str>, Arc<str>)> {
    catalog
        .iter()
        .filter(|item| item_matches_filter(item, filter, collection_members, now_secs, window_days))
        .map(|item| (Arc::clone(&item.id), Arc::clone(&item.title)))
        .collect()
}

/// Removes the queued download for `(id, index)` from `queue`, if present.
///
/// Returns `true` if an entry was removed. Only the matching `(id, index)`
/// pair is removed — other queued files for the same entry (different
/// index) or for other entries are left untouched.
pub(crate) fn dequeue_file(
    queue: &mut VecDeque<(Arc<str>, u32, String)>, id: &str, index: u32,
) -> bool {
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
pub(crate) fn reload_cooldown_active(meta: Option<&CacheMetadata>, now_secs: u64) -> bool {
    let Some(meta) = meta else {
        return false;
    };
    now_secs.saturating_sub(meta.saved_at_secs)
        < dtrpg_core::data::constants::FORCE_RELOAD_COOLDOWN_SECS
}

/// Returns `true` if a fresh-install catalog-initialization request should be
/// suppressed because the last one happened too recently, per
/// `CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS`.
///
/// `last_request_secs` is `None` when no fresh-install request has ever been
/// recorded (first launch, or metadata missing/unreadable) — treated as "not
/// recently requested," so a missing timestamp never blocks the request.
pub(crate) fn fresh_install_request_gated(last_request_secs: Option<u64>, now_secs: u64) -> bool {
    let Some(last) = last_request_secs else {
        return false;
    };
    now_secs.saturating_sub(last) < CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS
}

/// Returns `true` if an item last checked at `last_checked` is due for
/// another availability check, per `ITEM_CHECK_COOLDOWN_SECS`. `None` (never
/// checked) is always due.
pub(crate) fn item_check_due(last_checked: Option<std::time::SystemTime>) -> bool {
    match last_checked {
        None => true,
        Some(last) => {
            last.elapsed().map(|e| e.as_secs()).unwrap_or(0)
                >= dtrpg_core::data::constants::ITEM_CHECK_COOLDOWN_SECS
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
pub(crate) fn apply_check_result(
    item: &mut LibraryItem, result: Result<LibraryItem, LibraryServiceError>,
    checked_at: std::time::SystemTime,
) {
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
            let downloaded_by_id: HashMap<Arc<str>, bool> = item
                .files
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
pub(crate) fn should_enqueue_check(
    check_queue: &VecDeque<Arc<str>>, checking_items: &HashSet<Arc<str>>, id: &Arc<str>,
) -> bool {
    !checking_items.contains(id) && !check_queue.contains(id)
}

/// Returns how many more thumbnail fetches and/or file downloads can be
/// dispatched without exceeding `max_concurrent_downloads`, given the current
/// count of each already in flight.
///
/// Thumbnail and download slots share one aggregate limit, so this always
/// checks their sum rather than either count individually.
pub(crate) fn remaining_slots(
    max_concurrent_downloads: usize, active_thumbnail_fetches: usize, active_downloads: usize,
) -> usize {
    max_concurrent_downloads.saturating_sub(active_thumbnail_fetches + active_downloads)
}

/// Returns `true` if a check-batch request (manual or automatic) should be
/// suppressed because a batch was already enqueued too recently, per
/// `ITEM_CHECK_BATCH_COOLDOWN_SECS`. `None` (no prior batch recorded) never
/// blocks a request.
pub(crate) fn check_batch_cooldown_active(last_batch_secs: Option<u64>, now_secs: u64) -> bool {
    let Some(last) = last_batch_secs else {
        return false;
    };
    now_secs.saturating_sub(last) < dtrpg_core::data::constants::ITEM_CHECK_BATCH_COOLDOWN_SECS
}

/// Selects up to `limit` catalog item ids overdue for an availability check
/// (per [`item_check_due`]), oldest-checked first — never-checked items
/// (`None`) sort first as maximally overdue, per `Option`'s `Ord`
/// implementation (`None < Some(_)`).
pub(crate) fn select_check_batch(catalog: &[LibraryItem], limit: usize) -> Vec<Arc<str>> {
    let mut candidates: Vec<&LibraryItem> = catalog
        .iter()
        .filter(|item| item_check_due(item.availability_last_checked))
        .collect();
    candidates.sort_by_key(|item| item.availability_last_checked);
    candidates
        .into_iter()
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
pub(crate) fn merge_partial_fetch(
    mut existing: Vec<LibraryItem>, partial: Vec<LibraryItem>,
) -> Vec<LibraryItem> {
    for mut item in partial {
        item.is_available = true;
        // Same `product_id` fallback as `reconcile_catalog`: a republished
        // title's `id` can change between fetches, so matching on `id` alone
        // would append a duplicate instead of refreshing the existing entry.
        let existing_match = existing
            .iter_mut()
            .find(|e| e.id == item.id || (item.product_id != 0 && e.product_id == item.product_id));
        if let Some(existing_item) = existing_match {
            *existing_item = item;
        } else {
            existing.push(item);
        }
    }
    existing
}

/// Derives the `since` timestamp for a partial date-filtered fetch: the most
/// recent `date_updated` (falling back to `date_added`) across `catalog`,
/// formatted as RFC 3339. Returns `None` if the catalog is empty or every
/// item has neither timestamp populated.
pub(crate) fn partial_fetch_since(catalog: &[LibraryItem]) -> Option<String> {
    let max_epoch = catalog
        .iter()
        .filter_map(|item| item.date_updated.or(item.date_added))
        .max()?;
    dtrpg_core::util::datetime::epoch_to_rfc3339(max_epoch)
}

/// Determines whether a startup remote-vs-cached item count mismatch should
/// be resolved with a cheaper additive partial fetch, per
/// `catalog-auto-load-policy`'s startup count-based decision.
///
/// `true` only for a growth-only mismatch (`remote_count > cached_count`) —
/// a decrease, or any mismatch a pure addition can't explain, always
/// requires a full reconciliation fetch, since only that can identify which
/// item is missing.
pub(crate) fn should_attempt_partial_fetch(remote_count: usize, cached_count: usize) -> bool {
    remote_count > cached_count
}

/// Builds the map of every catalog entry that is not idle, from the ids being
/// verified against disk and the ids with a file queued or downloading. An id
/// in both is downloading (see [`ItemActivity::of`]).
pub(crate) fn item_activity_map<'a>(
    verifying: &HashSet<Arc<str>>, downloading: impl Iterator<Item = &'a Arc<str>>,
) -> HashMap<Arc<str>, ItemActivity> {
    let mut activity: HashMap<Arc<str>, ItemActivity> = verifying
        .iter()
        .map(|id| (Arc::clone(id), ItemActivity::Verifying))
        .collect();
    for id in downloading {
        activity.insert(Arc::clone(id), ItemActivity::Downloading);
    }
    activity
}
