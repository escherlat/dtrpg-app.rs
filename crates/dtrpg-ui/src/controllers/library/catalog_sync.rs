//! The multi-stage catalog synchronization sequence (fresh install, disk
//! cache, collections, and page-by-page fetch). The stages themselves live
//! in sibling files (`catalog_sync_cache`, `catalog_sync_autoload`,
//! `catalog_sync_fresh_install`, `catalog_sync_fetch`); this file only wires
//! them together in order.

use dtrpg_core::services::network_monitor::NetworkMonitor;

use super::catalog_sync_autoload::{AutoLoadOutcome, try_skip_or_partial_fetch};
use super::catalog_sync_cache::{prepopulate_from_cache, sync_collections};
use super::catalog_sync_fetch::run_full_fetch;
use super::catalog_sync_fresh_install::fresh_install_totals;
use super::*;

/// Context shared by every stage of one catalog-sync run. Built once in
/// [`LibraryController::start_load_inner`] and passed by reference to each
/// stage, since the stages are free functions (not methods: the sync runs
/// inside a `cx.spawn` closure that only holds a `WeakEntity`, not `&mut
/// self`) rather than individual parameters.
pub(super) struct CatalogSyncCtx {
    pub(super) this:            WeakEntity<LibraryController>,
    pub(super) weak_activity:   WeakEntity<ActivityController>,
    pub(super) activity_id:     u64,
    pub(super) generation:      u64,
    pub(super) service_arc:     Arc<dyn LibraryService>,
    pub(super) storage_root:    PathBuf,
    pub(super) network_monitor: Arc<NetworkMonitor>,
}

impl LibraryController {
    pub(super) fn start_load_inner(&mut self, cx: &mut Context<Self>, force_reload: bool) {
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
        let storage_root = self.cache_root().clone();
        let network_monitor = Arc::clone(self.network_monitor());

        cx.spawn(async move |this, async_cx| {
              // Single activity item for the whole load. Its label is updated
              // in place as the load moves through stages
              // (collections, count check, live fetch,
              // page-by-page progress) rather than starting a new
              // activity per stage — the stages are not independent
              // operations, they are phases of one catalog load.
              let activity_id = weak_activity.update(async_cx, |a, cx| {
                                                 a.start(&t!("activity.loading_library"), None, cx)
                                             })
                                             .unwrap_or(0);

              let ctx = CatalogSyncCtx { this: this.clone(),
                                         weak_activity: weak_activity.clone(),
                                         activity_id,
                                         generation,
                                         service_arc,
                                         storage_root,
                                         network_monitor };

              // ── Pre-populate catalog from disk cache, then collections
              // ────────── Runs first, ahead of the collections
              // stage: it's a local disk read with no network
              // dependency, so any catalog just cleared by
              // this load's caller (e.g. `replace_service` swapping in an
              // authenticated service, or `clear_and_reload`) reappears
              // immediately instead of sitting empty for however long the
              // collections network call takes to resolve — see
              // `catalog-live-merge`.
              let cached = prepopulate_from_cache(&ctx, async_cx, force_reload).await;
              sync_collections(&ctx, async_cx, collections_service).await;

              // ── Auto-load policy
              // ──────────────────────────────────────────────
              // Skip the live fetch when: the cache is non-empty, was written
              // within the last 7 days, and force_reload is
              // false.
              if let AutoLoadOutcome::Handled =
                  try_skip_or_partial_fetch(&ctx, async_cx, force_reload, &cached).await
              {
                  return;
              }

              // ── Fresh-install initialization
              // ─────────────────────────────────────
              // No local catalog data was loaded from disk above: this is a
              // fresh install (or the cache was
              // cleared/relocated), per
              // `rust-catalog-fresh-install-initialization`. Credentials are
              // not pre-checked here — `list_items_paged` below
              // already returns a `Session` error, handled as a
              // quiet completion, when no authenticated session
              // exists yet, which this same code path relies
              // on regardless of fresh-install status.
              let fresh_install_total =
                  fresh_install_totals(&ctx, async_cx, force_reload, &cached).await;

              // ── Stage: page-by-page fetch from API
              // ──────────────────────────────
              run_full_fetch(&ctx, async_cx, cached, fresh_install_total).await;
          })
          .detach();
    }
}
