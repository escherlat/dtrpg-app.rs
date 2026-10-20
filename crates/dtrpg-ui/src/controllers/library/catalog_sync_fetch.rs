//! The page-by-page live fetch stage of catalog sync: the full paginated
//! fetch from the API, with checkpointing and progress reporting. See
//! `catalog_sync.rs` for the orchestrator this stage is called from.

use super::catalog_sync::CatalogSyncCtx;
use super::*;

pub(super) async fn run_full_fetch(ctx: &CatalogSyncCtx, async_cx: &mut AsyncApp,
                                   cached: Option<Vec<LibraryItem>>,
                                   fresh_install_total: Option<usize>) {
    // Query the network monitor before committing to the paginated fetch —
    // per `rust-resource-network-monitor`, a process needing a specific
    // endpoint stops rather than requesting when it's reported unreachable.
    let monitor_for_fetch = ctx.network_monitor.clone();
    let endpoint_reachable =
        async_cx.background_executor()
                .spawn(async move {
                    monitor_for_fetch.check_endpoint(dtrpg_core::data::constants::DTRPG_API_HOST)
                })
                .await;
    if !endpoint_reachable {
        tracing::debug!("catalog fetch skipped: DriveThruRPG API unreachable");
        ctx.weak_activity
           .update(async_cx, |a, cx| a.complete(ctx.activity_id, cx))
           .ok();
        ctx.this
           .update(async_cx, |ctrl, cx| {
               if ctrl.load_generation == ctx.generation {
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
    // below (`page N…`) replaces this as soon as the first page arrives.
    ctx.weak_activity
       .update(async_cx, |a, cx| {
           a.update_label(ctx.activity_id, t!("activity.loading_library"), cx)
       })
       .ok();

    // Two channels: one for page batches, one for the API-reported total, if
    // any.
    let (tx, rx) = std::sync::mpsc::channel::<Vec<LibraryItem>>();
    let (total_tx, total_rx) = std::sync::mpsc::channel::<usize>();

    // Cloned before `service_arc` moves into the fetch spawn below — used to
    // get a live item count for progress seeding when this isn't a fresh
    // install.
    let svc_for_count = ctx.service_arc.clone();
    let service_arc = ctx.service_arc.clone();

    // Run the paginated fetch on the background executor. Each page — and, when
    // the API reports one (via `links.last`), the estimated total — is sent
    // through its channel as it arrives; the result indicates overall
    // success/failure.
    let fetch = async_cx.background_executor().spawn(async move {
                                                  service_arc.list_items_paged(&mut |page| {
                                                                        tx.send(page).ok();
                                                                    },
                                                                    Some(&mut |total| {
                                                                        total_tx.send(total).ok();
                                                                    }))
                                              });

    // Seed the progress denominator, preferring a fresh-install totals
    // request (see `catalog_sync_fresh_install`) since it reflects the
    // remote count directly. For a full fetch triggered for any other reason
    // (stale cache, count mismatch, force reload), a single best-effort
    // `count_items()` call gets a live remote count instead — the local
    // cache's item count is exactly the value already known (or suspected)
    // to be wrong whenever this stage runs at all, so seeding progress from
    // it pins the bar near 100% after only a few pages once the real catalog
    // has grown since the cache was last saved. Only if that live request
    // also comes back empty does the stale cache count get used, so the bar
    // still starts determinate rather than not at all. The real
    // DriveThruRPG API never reports `links.last`, so `total_rx` below
    // essentially never fires in practice — this is the only correction this
    // estimate gets before the fetch completes.
    let live_count = if fresh_install_total.is_none() {
        async_cx.background_executor()
                .spawn(async move { svc_for_count.count_items() })
                .await
                .and_then(Result::ok)
    }
    else {
        None
    };
    let mut estimated_total: Option<usize> = fresh_install_total.or(live_count).or_else(|| {
                                                                                   cached.as_ref()
                                                    .map(|items| items.len())
                                                    .filter(|&n| n > 0)
                                                                               });
    if let Some(total) = estimated_total {
        ctx.weak_activity
           .update(async_cx, |a, cx| {
               a.update_progress(ctx.activity_id, 0.0, cx)
           })
           .ok();
        tracing::debug!(estimated_total = total, "catalog load: seeded total");
    }

    // Accumulate all live SDK pages into a local buffer so a *populated* cache
    // remains visible in the UI throughout the fetch instead of flashing down
    // to a single page (see `catalog-live-merge`). When there is nothing
    // cached to protect — first launch, or right after `clear_and_reload` —
    // there is no flash risk, so each page is appended to the visible
    // catalog (and its thumbnails enqueued) as it arrives instead of
    // waiting for the entire fetch to finish. This is what makes thumbnail
    // loading observable/testable without waiting through a full multi-page
    // fetch.
    let catalog_was_empty = ctx.this
                               .update(async_cx, |ctrl, _cx| {
                                   ctrl.index().count_all().unwrap_or(0) == 0
                               })
                               .unwrap_or(false);

    let mut rx = Some(rx);
    let mut total_rx = Some(total_rx);
    let mut live_items: Vec<LibraryItem> = Vec::new();
    let mut page_num: u32 = 0;
    // Checkpointing state for `catalog-cache-checkpointing`: the accumulating
    // `live_items` buffer (not `self.catalog`, which isn't swapped in until the
    // full fetch completes) is periodically saved to disk so an interrupted
    // load still leaves a recent cache for the next startup.
    // `checkpoint_task` is awaited — never dropped while pending — before
    // starting the next checkpoint or the final post-fetch save, so writes
    // never race each other or the final save over the same `.tmp` file.
    let mut checkpoint_task: Option<Task<()>> = None;
    let mut last_checkpoint_page: u32 = 0;
    let mut last_checkpoint_at = std::time::Instant::now();
    loop {
        let Some(receiver) = rx.take()
        else {
            break;
        };

        // Non-blocking check for an API-reported total before processing the
        // next page, so the bar re-bases onto the real count as soon as it's
        // known without ever blocking page delivery.
        if let Some(total_receiver) = total_rx.take() {
            match total_receiver.try_recv() {
                Ok(total) => {
                    estimated_total = Some(total);
                    tracing::debug!(estimated_total = total,
                                    "catalog load: API-reported item count");
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    total_rx = Some(total_receiver);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }

        let (msg, returned_rx) = async_cx.background_executor()
                                         .spawn(async move {
                                             let msg = receiver.recv();
                                             (msg, receiver)
                                         })
                                         .await;

        match msg {
            Ok(items) => {
                if catalog_was_empty {
                    let page = items.clone();
                    ctx.this
                       .update(async_cx, |ctrl, cx| {
                           if ctrl.load_generation == ctx.generation {
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
                      >= std::time::Duration::from_secs(CATALOG_CHECKPOINT_MIN_INTERVAL_SECS)
                {
                    if let Some(prev) = checkpoint_task.take() {
                        prev.await;
                    }
                    let snapshot = live_items.clone();
                    let checkpoint_root = ctx.storage_root.clone();
                    checkpoint_task = Some(async_cx.background_executor().spawn(async move {
                        if let Err(e) = save_catalog_cache(&checkpoint_root, &snapshot) {
                            tracing::warn!(error = %e, "failed to checkpoint catalog cache");
                        }
                    }));
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
                ctx.weak_activity
                   .update(async_cx, |a, cx| a.update_label(ctx.activity_id, label, cx))
                   .ok();
                // When a total estimate is available, also drive the
                // progress bar's percentage from it.
                if let Some(total) = estimated_total.filter(|&t| t > 0) {
                    let progress = (live_items.len() as f32 / total as f32).min(1.0);
                    ctx.weak_activity
                       .update(async_cx, |a, cx| {
                           a.update_progress(ctx.activity_id, progress, cx)
                       })
                       .ok();
                }
                rx = Some(returned_rx);
            }
            Err(_) => break, // sender dropped — all pages have been sent
        }
    }

    // Wait for the fetch task to complete and surface any error.
    // On success: swap the catalog atomically with the full live dataset — a
    // no-op   diff when `catalog_was_empty` already appended every page
    // incrementally   above, but still required to guarantee the final
    // state is exactly the live   dataset (e.g. if a page arrived out of
    // order or a duplicate id was   deduped differently) — then save to
    // disk and dismiss the loading indicator. On error: leave the cached
    // catalog unchanged in memory, but still clear   `catalog_loading` —
    // otherwise a failed fetch with nothing cached leaves the
    //   catalog view's spinner (`catalog_loading && item_count == 0`) spinning
    //   forever instead of falling through to the empty state.
    match fetch.await {
        Ok(()) => {
            let is_current =
                ctx.this
                   .update(async_cx, |ctrl, _cx| ctrl.load_generation == ctx.generation)
                   .unwrap_or(false);
            if !is_current {
                // A newer load (e.g. triggered by a cache clear) has already
                // taken over; discard this stale result instead of clobbering
                // it.
                ctx.weak_activity
                   .update(async_cx, |a, cx| a.complete(ctx.activity_id, cx))
                   .ok();
                ctx.this
                   .update(async_cx, |ctrl, cx| ctrl.finish_catalog_sync(cx))
                   .ok();
                return;
            }
            ctx.this
               .update(async_cx, |ctrl, cx| {
                   ctrl.set_catalog(live_items, !catalog_was_empty, cx);
                   ctrl.begin_tag_sync(false, cx);
                   ctrl.verify_catalog_downloads(cx);
               })
               .ok();
            // Wait for any in-flight checkpoint write to finish — it wrote
            // `save_root`'s `.tmp` file for the accumulating buffer during
            // the fetch; `set_catalog` above already wrote the finished
            // result straight to `ctrl.index()`, so no final save is needed.
            if let Some(prev) = checkpoint_task.take() {
                prev.await;
            }
            ctx.weak_activity
               .update(async_cx, |a, cx| a.complete(ctx.activity_id, cx))
               .ok();
        }
        Err(e) => {
            // Session errors are expected when starting before auth completes;
            // treat them as a quiet completion rather than a user-facing alert.
            // Network and other errors are genuine failures worth surfacing.
            if e.kind == LibraryServiceErrorKind::Session {
                // WARN, not debug: see the matching comment on the collections
                // load's Session-error branch in `catalog_sync_cache`.
                tracing::warn!(error = %e, "catalog load skipped: no authenticated session");
                ctx.weak_activity
                   .update(async_cx, |a, cx| a.complete(ctx.activity_id, cx))
                   .ok();
            }
            else {
                tracing::error!(error = %e, backtrace = %app_backtrace(), "catalog load failed");
                let detail = e.panel_detail();
                ctx.weak_activity
                   .update(async_cx, |a, cx| a.error(ctx.activity_id, detail, cx))
                   .ok();
            }
            ctx.this
               .update(async_cx, |ctrl, cx| {
                   if ctrl.load_generation != ctx.generation {
                       return; // superseded by a newer load
                   }
                   ctrl.catalog_loading = false;
                   cx.emit(LibraryChanged);
               })
               .ok();
        }
    }
    ctx.this
       .update(async_cx, |ctrl, cx| ctrl.finish_catalog_sync(cx))
       .ok();
}
