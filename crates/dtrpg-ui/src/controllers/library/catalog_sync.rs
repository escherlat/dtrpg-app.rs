//! The multi-stage catalog synchronization sequence (fresh install, disk
//! cache, collections, and page-by-page fetch).

use super::*;

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
                                || {
                                    LibraryServiceError::new(
                                        LibraryServiceErrorKind::Network,
                                        "totals request cancelled",
                                    )
                                },
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
}
