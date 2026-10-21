//! Pure helper functions used by the library controller and its tests.

use super::*;

/// Flips a per-key boolean flag in `map`, treating a missing entry as `false`.
///
/// Used by per-entry ephemeral UI toggles (e.g. the "Other details"
/// disclosure) that default to a collapsed/off state until first toggled.
pub(crate) fn toggle_bool_flag<K: std::hash::Hash + Eq>(map: &mut HashMap<K, bool>, key: K)
                                                        -> bool {
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
pub(crate) fn cached_file_info(cache: &mut HashMap<(Arc<str>, usize),
                                            crate::util::file_info::FileInfo>,
                               entry_id: &Arc<str>, row_ix: usize, downloaded: bool,
                               path: &Path, format: &str)
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
pub(crate) fn toggle_row_selection<K: std::hash::Hash + Eq>(map: &mut HashMap<K, usize>, key: K,
                                                            row_ix: usize) {
    if map.get(&key) == Some(&row_ix) {
        map.remove(&key);
    }
    else {
        map.insert(key, row_ix);
    }
}

/// Returns the positions of every not-yet-downloaded file in `files`, in
/// order.
///
/// Used by [`LibraryController::enqueue_download`] to determine which files
/// of a (possibly multi-item) entry to queue: an already-downloaded file is
/// never re-enqueued, so a bundle with some items already present only
/// queues the missing ones.
pub(crate) fn missing_file_indices(files: &[LibraryItemFile]) -> Vec<u32> {
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
/// [`dtrpg_core::data::storage::available_space_mb`].
pub(crate) fn missing_files_size_mb(files: &[LibraryItemFile]) -> f64 {
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
pub(crate) fn missing_files_size_mb_for_items<'a>(catalog: &[LibraryItem],
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
pub(crate) fn collection_download_targets(catalog: &[LibraryItem], member_ids: &[u64])
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
pub(crate) fn publisher_download_targets(catalog: &[LibraryItem], publisher: &str)
                                         -> Vec<(Arc<str>, Arc<str>)> {
    catalog.iter()
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
pub(crate) fn filter_download_targets(catalog: &[LibraryItem], filter: &SidebarFilter,
                                      collection_members: &HashSet<u64>, now_secs: i64,
                                      window_days: u32)
                                      -> Vec<(Arc<str>, Arc<str>)> {
    catalog.iter()
           .filter(|item| {
               item_matches_filter(item, filter, collection_members, now_secs, window_days)
           })
           .map(|item| (Arc::clone(&item.id), Arc::clone(&item.title)))
           .collect()
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

/// Returns `true` if a check-batch request (manual or automatic) should be
/// suppressed because a batch was already enqueued too recently, per
/// `ITEM_CHECK_BATCH_COOLDOWN_SECS`. `None` (no prior batch recorded) never
/// blocks a request.
pub(crate) fn check_batch_cooldown_active(last_batch_secs: Option<u64>, now_secs: u64) -> bool {
    let Some(last) = last_batch_secs
    else {
        return false;
    };
    now_secs.saturating_sub(last) < dtrpg_core::data::constants::ITEM_CHECK_BATCH_COOLDOWN_SECS
}

/// Selects up to `limit` catalog item ids overdue for an availability check
/// (per [`item_check_due`]), oldest-checked first — never-checked items
/// (`None`) sort first as maximally overdue, per `Option`'s `Ord`
/// implementation (`None < Some(_)`).
pub(crate) fn select_check_batch(catalog: &[LibraryItem], limit: usize) -> Vec<Arc<str>> {
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

/// Builds the map of every catalog entry that is not idle, from the ids being
/// verified against disk and the ids with a file queued or downloading. An id
/// in both is downloading (see [`ItemActivity::of`]).
pub(crate) fn item_activity_map<'a>(verifying: &HashSet<Arc<str>>,
                                    downloading: impl Iterator<Item = &'a Arc<str>>)
                                    -> HashMap<Arc<str>, ItemActivity> {
    let mut activity: HashMap<Arc<str>, ItemActivity> =
        verifying.iter()
                 .map(|id| (Arc::clone(id), ItemActivity::Verifying))
                 .collect();
    for id in downloading {
        activity.insert(Arc::clone(id), ItemActivity::Downloading);
    }
    activity
}
