//! Cover-thumbnail queueing, fetching, caching, and detail-cover refresh.
//!
//! Fetches run as jobs on the core work queue. This module submits them and
//! turns the queue's events back into cache, catalog, and activity updates.

use dtrpg_core::work::{JobId, JobKind, JobSpec, Outcome, Priority, ResultSlot};

use super::{thumbnail_jobs::ThumbnailJob, *};

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
        for (id, url) in to_enqueue {
            self.submit_thumbnail(id, url, false, Priority::Background, cx);
        }
    }

    /// Submits one cover fetch to the work queue and marks it in flight.
    ///
    /// Any fetch for the same cache key that has not started is replaced, so a
    /// key is never queued twice. `force_network` skips the disk cache and
    /// always re-fetches — set for manual "Load Thumbnail"/"Refresh
    /// Thumbnails" actions, which exist specifically to bypass a stale cached
    /// image; left `false` for automatic background loads.
    fn submit_thumbnail(&mut self, key: Arc<str>, url: Arc<str>, force_network: bool,
                        priority: Priority, cx: &mut Context<Self>) {
        self.cancel_pending_thumbnail(&key);
        cx.global_mut::<CoverCache>()
          .mark_in_flight(Arc::clone(&key));

        let bytes = ResultSlot::new();
        let fetcher = Arc::clone(&self.cover_fetcher);
        let job_key = Arc::clone(&key);
        let job_url = Arc::clone(&url);
        let job_bytes = bytes.clone();
        let spec = JobSpec { kind: JobKind::Thumbnail,
                             priority,
                             label: key.to_string() };
        let id = self.work.submit(spec, move |context| {
                              let fetched = fetcher.fetch(&job_key,
                                                          &job_url,
                                                          force_network,
                                                          context.cancel_flag())
                                                   .map_err(|error| error.to_string())?;
                              job_bytes.put(fetched);
                              Ok(())
                          });
        self.thumbnail_jobs
            .add(id, ThumbnailJob { key, url, bytes });
    }

    /// Cancels the fetch for `key` if it has not started. A fetch already
    /// running is left to finish.
    fn cancel_pending_thumbnail(&mut self, key: &str) {
        if let Some(id) = self.thumbnail_jobs.remove_pending(key) {
            self.work.cancel(id);
        }
    }

    /// Cancels every fetch that has not started and clears its in-flight
    /// marker, so the cover can be requested again later. Fetches already
    /// running are left to finish.
    pub(super) fn cancel_pending_thumbnails(&mut self, cx: &mut Context<Self>) {
        for (key, id) in self.thumbnail_jobs.remove_all_pending() {
            self.work.cancel(id);
            cx.global_mut::<CoverCache>().in_flight.remove(&key);
        }
    }

    /// Starts the aggregated "loading thumbnails" activity entry when the
    /// first fetch of a batch begins.
    pub(super) fn thumbnail_started(&mut self, id: JobId, cx: &mut Context<Self>) {
        if !self.thumbnail_jobs.mark_started(id) || self.thumbnail_activity_id.is_some() {
            return;
        }
        let label = if self.thumbnail_refresh_active {
            t!("activity.refreshing_thumbnails")
        }
        else {
            t!("activity.loading_thumbnails")
        };
        let activity_id = self.activity.update(cx, |a, cx| a.start(&label, None, cx));
        self.thumbnail_activity_id = Some(activity_id);
        self.thumbnail_processed = 0;
    }

    /// Applies a finished fetch: caches the image (or records the failure),
    /// then advances the batch's progress.
    pub(super) fn thumbnail_finished(&mut self, id: JobId, outcome: Outcome,
                                     cx: &mut Context<Self>) {
        let Some(job) = self.thumbnail_jobs.finish(id)
        else {
            // Cancelled before it started, and already forgotten.
            return;
        };
        let bytes = match outcome {
            Outcome::Succeeded => job.bytes.take(),
            Outcome::Failed(ref message) => {
                tracing::warn!(url = %job.url, error = %message, "thumbnail fetch failed");
                None
            }
            Outcome::Cancelled => None,
        };

        let now = std::time::SystemTime::now();
        if let Some(item) = self.catalog.iter_mut().find(|i| i.id == job.key) {
            item.thumbnail_last_attempted = Some(now);
        }
        match bytes {
            Some(bytes) => {
                cx.global_mut::<CoverCache>()
                  .insert(Arc::clone(&job.key), bytes);
                self.invalidate_cache();
                self.record_refresh_result(&job.key, false, cx);
                cx.emit(LibraryChanged);
            }
            None => {
                cx.global_mut::<CoverCache>().in_flight.remove(&job.key);
                self.invalidate_cache();
                self.record_refresh_result(&job.key, true, cx);
            }
        }
        self.advance_thumbnail_progress(cx);
    }

    /// Counts one more attempt (success or failure) toward the batch total so
    /// the progress bar reflects real throughput instead of sitting
    /// indeterminate, and completes the activity entry once every fetch in the
    /// batch has finished.
    fn advance_thumbnail_progress(&mut self, cx: &mut Context<Self>) {
        self.thumbnail_processed += 1;
        let Some(activity_id) = self.thumbnail_activity_id
        else {
            return;
        };

        let processed = self.thumbnail_processed;
        let remaining = self.thumbnail_jobs.outstanding();
        if remaining == 0 {
            self.activity
                .update(cx, |a, cx| a.complete(activity_id, cx));
            self.thumbnail_activity_id = None;
            self.thumbnail_processed = 0;
            return;
        }

        let label = if self.thumbnail_refresh_active {
            t!("activity.refreshing_thumbnails_remaining",
               remaining = remaining).to_string()
        }
        else {
            t!("activity.loading_thumbnails_remaining",
               remaining = remaining).to_string()
        };
        let total = (processed + remaining) as f32;
        let progress = processed as f32 / total;
        self.activity.update(cx, |a, cx| {
                         a.update_label(activity_id, label, cx);
                         a.update_progress(activity_id, progress, cx);
                     });
    }

    /// Enqueues a single cover URL ahead of any background thumbnail work.
    /// Used by the per-item "Load Thumbnail" context menu action.
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
            self.submit_thumbnail(id, cover_url, true, Priority::Interactive, cx);
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
        self.submit_thumbnail(cache_key, url, false, Priority::Interactive, cx);
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
            self.submit_thumbnail(cache_key, url, true, Priority::Interactive, cx);
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
        let to_enqueue: Vec<(Arc<str>, Arc<str>)> = self.catalog
                                                        .iter()
                                                        .filter_map(|item| {
                                                            let url = item.cover_url.as_ref()?;
                                                            Some((Arc::clone(&item.id),
                                                                  Arc::clone(url)))
                                                        })
                                                        .collect();

        if to_enqueue.is_empty() {
            cx.emit(ThumbnailRefreshNoOp);
            return;
        }

        self.cancel_pending_thumbnails(cx);
        // This forces a fresh full re-fetch batch — reset the processed counter
        // so the progress bar denominator isn't skewed by whatever
        // fraction of a prior batch had already completed before this
        // action was invoked.
        self.thumbnail_processed = 0;
        self.thumbnail_refresh_active = true;
        self.thumbnail_refresh_pending = to_enqueue.iter().map(|(id, _)| Arc::clone(id)).collect();
        self.thumbnail_refresh_succeeded = 0;
        self.thumbnail_refresh_failed = 0;
        cx.emit(ThumbnailRefreshStarted { count: to_enqueue.len(), });
        for (id, url) in to_enqueue {
            self.submit_thumbnail(id, url, true, Priority::Background, cx);
        }
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
    pub(super) fn record_refresh_result(&mut self, item_id: &Arc<str>, failed: bool,
                                        cx: &mut Context<Self>) {
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
}
