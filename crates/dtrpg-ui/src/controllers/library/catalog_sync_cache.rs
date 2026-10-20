//! The disk-cache pre-populate and collections stages of catalog sync. See
//! `catalog_sync.rs` for the orchestrator these stages are called from.

use super::catalog_sync::CatalogSyncCtx;
use super::*;

/// Pre-populates the in-memory catalog from the on-disk cache, ahead of any
/// network call. Returns the cached items (deduped, possibly empty) so later
/// stages can decide whether a live fetch is still needed.
pub(super) async fn prepopulate_from_cache(ctx: &CatalogSyncCtx, async_cx: &mut AsyncApp,
                                           force_reload: bool)
                                           -> Option<Vec<LibraryItem>> {
    let cache_root = ctx.storage_root.clone();
    let cached =
        async_cx.background_executor()
                .spawn(async move { load_catalog_cache(&cache_root).map(dedupe_by_product_id) })
                .await;
    if let Some(items) = cached.as_ref() {
        ctx.this
           .update(async_cx, |ctrl, cx| {
               if ctrl.load_generation != ctx.generation {
                   return; // superseded by a newer load (e.g. cache-clear reload)
               }
               if force_reload && let Err(e) = ctrl.index.replace_all_items(&[]) {
                   tracing::error!(error = %e, "failed to clear the library index");
               }
               // Only pre-populate when there's nothing in the index yet (fresh
               // controller, or just cleared above) — `append_catalog_page`
               // does a plain `extend` with no dedup, so calling it again on an
               // already-populated catalog (e.g. a periodic refresh tick after
               // the initial load completed, or any later call to
               // `start_load`/`start_load_inner`) would duplicate every item.
               if ctrl.index.count_all().unwrap_or(0) == 0 {
                   ctrl.append_catalog_page(items.clone(), cx);
               }
               // Whatever the network does next, the tags are on disk.
               ctrl.begin_tag_sync(force_reload, cx);
           })
           .ok();
    }
    cached
}

/// Loads the cached collections list, then refreshes it from the live
/// service. Non-fatal on failure: collections and catalog are independent
/// datasets sharing one activity item, so a collections failure does not
/// abort the catalog stages that follow.
pub(super) async fn sync_collections(ctx: &CatalogSyncCtx, async_cx: &mut AsyncApp,
                                     collections_service: Arc<dyn CollectionsService>) {
    ctx.weak_activity
       .update(async_cx, |a, cx| {
           a.update_label(ctx.activity_id,
                          t!("activity.loading_library_collections"),
                          cx)
       })
       .ok();

    let col_cache_root = ctx.storage_root.clone();
    let cached_collections = async_cx.background_executor()
                                     .spawn(async move { load_collections_cache(&col_cache_root) })
                                     .await;
    if let Some(entries) = cached_collections {
        ctx.this
           .update(async_cx, |ctrl, cx| {
               if ctrl.load_generation != ctx.generation {
                   return; // superseded by a newer load (e.g. cache-clear reload)
               }
               ctrl.apply_collections(entries, cx);
           })
           .ok();
    }

    let col_save_root = ctx.storage_root.clone();
    let live_collections = async_cx.background_executor()
                                   .spawn(async move { collections_service.list_collections() })
                                   .await;
    match live_collections {
        Ok(entries) => {
            let to_save = entries.clone();
            async_cx.background_executor()
                    .spawn(async move {
                        if let Err(e) = save_collections_cache(&col_save_root, &to_save) {
                            tracing::warn!(error = %e, "failed to save collections cache");
                        }
                    })
                    .await;
            ctx.this
               .update(async_cx, |ctrl, cx| {
                   if ctrl.load_generation != ctx.generation {
                       return; // superseded by a newer load
                   }
                   ctrl.apply_collections(entries, cx);
               })
               .ok();
        }
        Err(e) => {
            // WARN, not debug: the default log filter is `warn` (see
            // `logging::init`), and "not signed in yet" is exactly the kind of
            // routine-but-worth-knowing condition a user reading logs should be
            // able to see without setting RUST_LOG.
            if e.kind == CollectionsServiceErrorKind::Session {
                tracing::warn!(error = %e, "collections load skipped: no authenticated session");
            }
            else {
                tracing::warn!(error = %e, "collections load failed");
            }
        }
    }
}
