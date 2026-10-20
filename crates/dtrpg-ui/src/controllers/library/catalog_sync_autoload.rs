//! The auto-load policy stage of catalog sync: skip the live fetch when the
//! disk cache is fresh and its count matches the remote, or do a cheaper
//! partial fetch when it's a growth-only mismatch. See `catalog_sync.rs` for
//! the orchestrator this stage is called from.

use super::catalog_sync::CatalogSyncCtx;
use super::*;

/// Whether the auto-load policy fully handled this sync (caller returns) or
/// found no shortcut available (caller falls through to the full fetch).
pub(super) enum AutoLoadOutcome {
    Handled,
    FallThrough,
}

pub(super) async fn try_skip_or_partial_fetch(ctx: &CatalogSyncCtx, async_cx: &mut AsyncApp,
                                              force_reload: bool,
                                              cached: &Option<Vec<LibraryItem>>)
                                              -> AutoLoadOutcome {
    // Skip the live fetch when: the cache is non-empty, was written within
    // the last 7 days, and force_reload is false.
    let Some(items) = (!force_reload).then(|| cached.as_ref().filter(|items| !items.is_empty()))
                                     .flatten()
    else {
        return AutoLoadOutcome::FallThrough;
    };

    let meta_root = ctx.storage_root.clone();
    let meta = async_cx.background_executor()
                       .spawn(async move { load_cache_metadata(&meta_root) })
                       .await;
    let is_fresh = meta.as_ref().is_some_and(|m| !m.is_stale());
    if !is_fresh {
        return AutoLoadOutcome::FallThrough;
    }

    // Check remote count if the service supports it cheaply — this is a
    // stage of the same load, surfaced via a label update on the existing
    // activity rather than a separate item, so the user sees one operation
    // moving through phases instead of a flurry of independent-looking
    // activities.
    ctx.weak_activity
       .update(async_cx, |a, cx| {
           a.update_label(ctx.activity_id, t!("activity.loading_library_count"), cx)
       })
       .ok();
    let svc = ctx.service_arc.clone();
    let remote_count = async_cx.background_executor()
                               .spawn(async move { svc.count_items() })
                               .await;
    let count_matches = match remote_count {
        Some(Ok(n)) => n == items.len(),
        Some(Err(_)) => false, // count fetch failed; do live fetch
        None => true,          // count not supported; trust fresh cache
    };

    if count_matches {
        tracing::debug!(cached = items.len(),
                        "catalog auto-load: cache is fresh, skipping live fetch");
        ctx.weak_activity
           .update(async_cx, |a, cx| a.complete(ctx.activity_id, cx))
           .ok();
        ctx.this
           .update(async_cx, |ctrl, cx| {
               ctrl.finish_catalog_sync(cx);
               if ctrl.load_generation != ctx.generation {
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
        return AutoLoadOutcome::Handled;
    }

    let Some(Ok(remote_n)) = remote_count
    else {
        return AutoLoadOutcome::FallThrough;
    };
    if !should_attempt_partial_fetch(remote_n, items.len()) {
        return AutoLoadOutcome::FallThrough;
    }
    let Some(since) = partial_fetch_since(items)
    else {
        return AutoLoadOutcome::FallThrough;
    };

    // Growth-only mismatch: try a cheaper additive partial fetch before
    // falling back to a full paginated one. A decrease (or a mismatch a pure
    // addition can't explain) always falls through to the full fetch below,
    // since only reconciliation can identify *which* item is missing.
    ctx.weak_activity
       .update(async_cx, |a, cx| {
           a.update_label(ctx.activity_id, t!("activity.loading_library_partial"), cx)
       })
       .ok();
    let svc = ctx.service_arc.clone();
    let partial = async_cx.background_executor()
                          .spawn(async move {
                              let mut items = Vec::new();
                              let result = svc.list_items_updated_since(&since, &mut |page| {
                                                  items.extend(page)
                                              });
                              result.map(|r| r.map(|()| items))
                          })
                          .await;
    let Some(Ok(partial_items)) = partial
    else {
        // `None` (unsupported), an error, or a superseded load: fall through
        // to the full paginated fetch below.
        return AutoLoadOutcome::FallThrough;
    };
    let is_current = ctx.this
                        .update(async_cx, |ctrl, _cx| ctrl.load_generation == ctx.generation)
                        .unwrap_or(false);
    if !is_current {
        return AutoLoadOutcome::FallThrough;
    }

    ctx.this
       .update(async_cx, |ctrl, cx| {
           ctrl.apply_partial_fetch(partial_items, cx);
           ctrl.verify_catalog_downloads(cx);
       })
       .ok();
    let items_to_save = ctx.this
                           .update(async_cx, |ctrl, _cx| ctrl.catalog.clone())
                           .unwrap_or_default();
    let save_root = ctx.storage_root.clone();
    async_cx.background_executor()
            .spawn(async move {
                if let Err(e) = save_catalog_cache(&save_root, &items_to_save) {
                    tracing::warn!(error = %e, "failed to save catalog cache");
                }
            })
            .await;
    ctx.weak_activity
       .update(async_cx, |a, cx| a.complete(ctx.activity_id, cx))
       .ok();
    ctx.this
       .update(async_cx, |ctrl, cx| ctrl.finish_catalog_sync(cx))
       .ok();
    AutoLoadOutcome::Handled
}
