//! Cover-thumbnail queueing, fetching, caching, and detail-cover refresh.

use super::*;

impl LibraryController {
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
    pub(super) fn enqueue_thumbnails(&mut self, items: &[LibraryItem], cx: &mut Context<Self>) {
        let covers_root = covers_dir();
        let to_enqueue: Vec<(Arc<str>, Arc<str>)> = {
            let cache = cx.global_mut::<CoverCache>();
            items
                .iter()
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
    pub(super) fn drain_thumbnail_queue(&mut self, cx: &mut Context<Self>) {
        while self.available_slots() > 0 {
            let Some((item_id, url, force_network)) = self.thumbnail_queue.pop_front() else {
                break;
            };
            self.active_thumbnail_fetches += 1;
            self.dispatch_thumbnail_fetch(item_id, url, force_network, cx);
        }
    }

    /// Starts a single thumbnail fetch task. Assumes the caller has already
    /// reserved a concurrency slot by incrementing `active_thumbnail_fetches`.
    pub(super) fn dispatch_thumbnail_fetch(
        &mut self, item_id: Arc<str>, url: Arc<str>, force_network: bool, cx: &mut Context<Self>,
    ) {
        let activity_id = if let Some(id) = self.thumbnail_activity_id {
            id
        } else {
            let label = if self.thumbnail_refresh_active {
                t!("activity.refreshing_thumbnails")
            } else {
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
            // gpui's executors are not a Tokio runtime, and `dtrpg-ui` does
            // not depend on `tokio` directly, so the async
            // `reqwest::get(...).await` used here previously
            // had no reactor to run on and always failed. `reqwest::blocking`
            // manages its own internal runtime per call and
            // works from a plain OS thread, matching the
            // pattern the SDK gateway already uses
            // (`tokio::runtime::Runtime::block_on`) to
            // run network calls from these same background-executor threads.
            //
            // Disk-cache-first: the in-memory `CoverCache` is always empty at
            // startup, so without a persistent disk cache every
            // launch would re-download every cover
            // from the network. Check disk before hitting the network, and
            // persist a fresh network fetch to disk so the next
            // launch is a cache hit.
            let fetch_url = url_str.clone();
            let disk_key = Arc::clone(&item_id);
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let retry_url = fetch_url.clone();
            let result: Result<Vec<u8>, ThumbnailFetchError> = async_cx
                .background_executor()
                .spawn(async move {
                    let covers_root = covers_dir();
                    if !force_network
                        && let Some(bytes) = load_cached_cover(&covers_root, &disk_key)
                    {
                        return Ok(bytes);
                    }
                    if !network_monitor.check_endpoint(crate::data::constants::DTRPG_API_HOST) {
                        return Err(ThumbnailFetchError {
                            message: "DriveThruRPG API endpoint unreachable".to_string(),
                            retry_after: None,
                        });
                    }
                    let config = retry::RetryConfig {
                        max_attempts: crate::data::constants::IMAGE_CACHE_MAX_ATTEMPTS,
                        base_secs: crate::data::constants::IMAGE_CACHE_RETRY_BASE_DELAY_SECS,
                        max_secs: crate::data::constants::IMAGE_CACHE_RETRY_MAX_DELAY_SECS,
                    };
                    let mut on_retry =
                        |attempt: u32, delay: std::time::Duration, reason: &ThumbnailFetchError| {
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
                            let resp = reqwest::blocking::get(&fetch_url).map_err(|e| {
                                ThumbnailFetchError {
                                    message: e.to_string(),
                                    retry_after: None,
                                }
                            })?;
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
                                    message: e.to_string(),
                                    retry_after: None,
                                })?
                                .to_vec())
                        },
                        |_| true,
                        |e: &ThumbnailFetchError| e.retry_after,
                        || ThumbnailFetchError {
                            message: "cover thumbnail fetch cancelled".to_string(),
                            retry_after: None,
                        },
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
                        if let Some(item) = ctrl.catalog.iter_mut().find(|i| i.id == item_id) {
                            item.thumbnail_last_attempted = Some(std::time::SystemTime::now());
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
                        if let Some(item) = ctrl.catalog.iter_mut().find(|i| i.id == item_id) {
                            item.thumbnail_last_attempted = Some(std::time::SystemTime::now());
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

            // Count this attempt (success or failure) toward the batch total
            // so the progress bar reflects real throughput
            // instead of sitting indeterminate.
            let (processed, remaining, refresh_active) = this
                .update(async_cx, |ctrl, _cx| {
                    ctrl.thumbnail_processed += 1;
                    (
                        ctrl.thumbnail_processed,
                        ctrl.thumbnail_queue.len(),
                        ctrl.thumbnail_refresh_active,
                    )
                })
                .unwrap_or((0, 0, false));

            if remaining == 0 {
                weak_activity
                    .update(async_cx, |a, cx| a.complete(activity_id, cx))
                    .ok();
                this.update(async_cx, |ctrl, _cx| {
                    ctrl.thumbnail_activity_id = None;
                    ctrl.thumbnail_processed = 0;
                })
                .ok();
            } else {
                let label = if refresh_active {
                    t!(
                        "activity.refreshing_thumbnails_remaining",
                        remaining = remaining
                    )
                    .to_string()
                } else {
                    t!(
                        "activity.loading_thumbnails_remaining",
                        remaining = remaining
                    )
                    .to_string()
                };
                let total = (processed + remaining) as f32;
                let progress = if total > 0.0 {
                    processed as f32 / total
                } else {
                    0.0
                };
                weak_activity
                    .update(async_cx, |a, cx| {
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
        let item_id = self
            .catalog
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
        let Some(url) = self
            .catalog
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
        let Some(item) = self.catalog.iter().find(|i| i.id == item_id) else {
            return;
        };
        if let Some(url) = item.detail_cover_url.clone() {
            let cache_key = detail_cache_key(&item_id);
            self.thumbnail_queue.retain(|(i, _, _)| *i != cache_key);
            cx.global_mut::<CoverCache>()
                .mark_in_flight(Arc::clone(&cache_key));
            self.thumbnail_queue.push_front((cache_key, url, true));
            self.drain_thumbnail_queue(cx);
        } else if let Some(url) = item.cover_url.clone() {
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
        let to_enqueue: Vec<(Arc<str>, Arc<str>, bool)> = self
            .catalog
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
        cx.emit(ThumbnailRefreshStarted {
            count: to_enqueue.len(),
        });
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
    pub(super) fn record_refresh_result(
        &mut self, item_id: &Arc<str>, failed: bool, cx: &mut Context<Self>,
    ) {
        if !self.thumbnail_refresh_pending.remove(item_id) {
            return;
        }
        if failed {
            self.thumbnail_refresh_failed += 1;
        } else {
            self.thumbnail_refresh_succeeded += 1;
        }
        if self.thumbnail_refresh_active && self.thumbnail_refresh_pending.is_empty() {
            self.thumbnail_refresh_active = false;
            let succeeded = std::mem::take(&mut self.thumbnail_refresh_succeeded);
            let failed = std::mem::take(&mut self.thumbnail_refresh_failed);
            cx.emit(ThumbnailRefreshCompleted { succeeded, failed });
        }
    }
}

impl std::fmt::Display for ThumbnailFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
