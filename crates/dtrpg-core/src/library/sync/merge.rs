//! Pure catalog-merge and timing-gate helpers used by [`super::run`]. Moved
//! here from the UI controller's `helpers.rs` in ADR 0002 Slice 2 — they
//! never depended on `gpui`, only on `LibraryItem` and cache metadata.

use std::collections::HashMap;
use std::sync::Arc;

use crate::data::catalog_cache::CacheMetadata;
use crate::data::constants::{
    CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS, FORCE_RELOAD_COOLDOWN_SECS,
};
use crate::data::library::LibraryItem;
use crate::util::datetime::epoch_to_rfc3339;

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
pub(super) fn reconcile_catalog(existing: Vec<LibraryItem>, live: Vec<LibraryItem>)
                                -> Vec<LibraryItem> {
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
/// another — `0` is used elsewhere as a "SKU unknown" sentinel, not a real
/// shared identity.
pub(super) fn dedupe_by_product_id(items: Vec<LibraryItem>) -> Vec<LibraryItem> {
    let mut index_by_product_id: HashMap<u64, usize> = HashMap::new();
    let mut deduped: Vec<LibraryItem> = Vec::with_capacity(items.len());
    for item in items {
        if item.product_id == 0 {
            deduped.push(item);
            continue;
        }
        match index_by_product_id.get(&item.product_id) {
            Some(&existing_ix) => {
                let keep_new = match (deduped[existing_ix].is_available, item.is_available) {
                    (false, true) => true,
                    (true, false) => false,
                    _ => item.numeric_id > deduped[existing_ix].numeric_id,
                };
                if keep_new {
                    deduped[existing_ix] = item;
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

/// Merges a partial date-filtered fetch's `partial` results into `existing`
/// additively, per `catalog-availability-flag`'s partial-fetch requirement:
/// items present in `partial` are appended (if new) or have their fields
/// refreshed and `is_available` set to `true` (if already present); items
/// absent from `partial` are left completely untouched — a partial response
/// is never a complete listing, so absence from it means nothing.
pub(super) fn merge_partial_fetch(mut existing: Vec<LibraryItem>, partial: Vec<LibraryItem>)
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

/// Returns `true` if a user-requested full reload should be suppressed
/// because the cache was written too recently, per
/// `FORCE_RELOAD_COOLDOWN_SECS`.
///
/// `meta` is `None` when the cache metadata file is missing or unreadable
/// (e.g. before any successful sync, or right after `clear_and_reload`
/// clears the on-disk cache) — treated as "not recently reloaded," so a
/// missing timestamp never blocks a reload.
pub fn reload_cooldown_active(meta: Option<&CacheMetadata>, now_secs: u64) -> bool {
    let Some(meta) = meta
    else {
        return false;
    };
    now_secs.saturating_sub(meta.saved_at_secs) < FORCE_RELOAD_COOLDOWN_SECS
}

/// Returns `true` if a fresh-install catalog-initialization request should be
/// suppressed because the last one happened too recently, per
/// `CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS`.
///
/// `last_request_secs` is `None` when no fresh-install request has ever been
/// recorded (first launch, or metadata missing/unreadable) — treated as "not
/// recently requested," so a missing timestamp never blocks the request.
pub(super) fn fresh_install_request_gated(last_request_secs: Option<u64>, now_secs: u64) -> bool {
    let Some(last) = last_request_secs
    else {
        return false;
    };
    now_secs.saturating_sub(last) < CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS
}

/// Derives the `since` timestamp for a partial date-filtered fetch: the most
/// recent `date_updated` (falling back to `date_added`) across `catalog`,
/// formatted as RFC 3339. Returns `None` if the catalog is empty or every
/// item has neither timestamp populated.
pub(super) fn partial_fetch_since(catalog: &[LibraryItem]) -> Option<String> {
    let max_epoch = catalog.iter()
                           .filter_map(|item| item.date_updated.or(item.date_added))
                           .max()?;
    epoch_to_rfc3339(max_epoch)
}

/// Determines whether a startup remote-vs-cached item count mismatch should
/// be resolved with a cheaper additive partial fetch, per
/// `catalog-auto-load-policy`'s startup count-based decision.
///
/// `true` only for a growth-only mismatch (`remote_count > cached_count`) —
/// a decrease, or any mismatch a pure addition can't explain, always
/// requires a full reconciliation fetch, since only that can identify which
/// item is missing.
pub(super) fn should_attempt_partial_fetch(remote_count: usize, cached_count: usize) -> bool {
    remote_count > cached_count
}
