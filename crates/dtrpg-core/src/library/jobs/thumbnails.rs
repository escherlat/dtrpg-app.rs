//! Thumbnail fetch bookkeeping and submission, moved from the UI
//! controller's `thumbnail_jobs.rs`/`thumbnails.rs` in ADR 0002 Slice 4.
//!
//! The controller reacts to the existing `WorkQueue` `JobEvent`s (flowing
//! since Slice 1) by calling the query/finish methods below instead of
//! touching bookkeeping fields directly; it still owns `CoverCache` (holds
//! `gpui::Image`, so it can never move into `Library`) and applies the
//! fetched bytes to it itself.

use std::collections::HashMap;
use std::sync::{Arc, PoisonError};

use crate::library::Library;
use crate::work::{JobId, JobKind, JobSpec, Priority, ResultSlot};

/// What `Library` remembers about one unfinished thumbnail fetch.
pub struct ThumbnailJob {
    /// The cover's cache key: an item id, or a detail-cover key.
    pub key:   Arc<str>,
    /// The URL being fetched, for logging.
    pub url:   Arc<str>,
    /// Where the fetch leaves the image bytes when it succeeds.
    pub bytes: ResultSlot<Vec<u8>>,
}

/// Every thumbnail fetch that has been submitted and has not finished.
#[derive(Default)]
pub(crate) struct ThumbnailJobs {
    jobs:    HashMap<JobId, ThumbnailJob>,
    /// The jobs that have not started yet, by cache key. At most one per key.
    pending: HashMap<Arc<str>, JobId>,
}

impl ThumbnailJobs {
    fn add(&mut self, id: JobId, job: ThumbnailJob) {
        self.pending.insert(Arc::clone(&job.key), id);
        self.jobs.insert(id, job);
    }

    fn outstanding(&self) -> usize {
        self.jobs.len()
    }

    fn pending_count(&self) -> usize {
        self.pending.len()
    }

    fn remove_pending(&mut self, key: &str) -> Option<JobId> {
        let id = self.pending.remove(key)?;
        self.jobs.remove(&id);
        Some(id)
    }

    fn remove_all_pending(&mut self) -> Vec<(Arc<str>, JobId)> {
        let pending: Vec<_> = self.pending.drain().collect();
        for (_, id) in &pending {
            self.jobs.remove(id);
        }
        pending
    }

    fn mark_started(&mut self, id: JobId) -> bool {
        let Some(job) = self.jobs.get(&id)
        else {
            return false;
        };
        if self.pending.get(&job.key) == Some(&id) {
            self.pending.remove(&job.key);
        }
        true
    }

    fn finish(&mut self, id: JobId) -> Option<ThumbnailJob> {
        let job = self.jobs.remove(&id)?;
        if self.pending.get(&job.key) == Some(&id) {
            self.pending.remove(&job.key);
        }
        Some(job)
    }
}

impl Library {
    /// Submits one cover fetch to the work queue and marks it in `Library`'s
    /// own bookkeeping. Any fetch for the same cache key that has not
    /// started is replaced, so a key is never queued twice.
    ///
    /// Fetches through `self.cover_fetcher` directly — the only reason this
    /// used to live on the UI controller was that `CoverCache` (the decoded
    /// `gpui::Image` cache) lives there; marking a key in flight in that
    /// cache, and writing the fetched bytes into it, is still the
    /// controller's job once this returns.
    pub fn submit_thumbnail(&self, key: Arc<str>, url: Arc<str>, force_network: bool,
                            priority: Priority)
                            -> JobId {
        self.cancel_pending_thumbnail(&key);

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
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .add(id, ThumbnailJob { key, url, bytes });
        id
    }

    /// Cancels the fetch for `key` if it has not started. A fetch already
    /// running is left to finish.
    pub fn cancel_pending_thumbnail(&self, key: &str) {
        let pending = self.thumbnail_jobs
                          .lock()
                          .unwrap_or_else(PoisonError::into_inner)
                          .remove_pending(key);
        if let Some(id) = pending {
            self.work.cancel(id);
        }
    }

    /// Cancels every fetch that has not started, returning each with its
    /// cache key so the caller can clear it from `CoverCache`'s in-flight
    /// marker. Fetches already running are left to finish.
    pub fn cancel_pending_thumbnails(&self) -> Vec<(Arc<str>, JobId)> {
        let removed = self.thumbnail_jobs
                          .lock()
                          .unwrap_or_else(PoisonError::into_inner)
                          .remove_all_pending();
        for (_, id) in &removed {
            self.work.cancel(*id);
        }
        removed
    }

    /// Notes that a thumbnail job has started. Returns whether it is one of
    /// `Library`'s own.
    pub fn thumbnail_started(&self, id: JobId) -> bool {
        self.thumbnail_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .mark_started(id)
    }

    /// Removes a finished thumbnail job and returns it, or `None` if it is
    /// not one of ours or was already forgotten by a cancel.
    pub fn thumbnail_finished(&self, id: JobId) -> Option<ThumbnailJob> {
        self.thumbnail_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .finish(id)
    }

    /// How many thumbnail fetches have not finished, whether waiting or
    /// running.
    #[must_use]
    pub fn thumbnail_jobs_outstanding(&self) -> usize {
        self.thumbnail_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .outstanding()
    }

    /// Whether no thumbnail fetch is waiting or running.
    #[must_use]
    pub fn thumbnail_jobs_is_empty(&self) -> bool {
        self.thumbnail_jobs_outstanding() == 0
    }

    /// How many thumbnail fetches are waiting to start.
    #[must_use]
    pub fn thumbnail_jobs_pending_count(&self) -> usize {
        self.thumbnail_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pending_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::{JobKind, JobSpec, Priority, WorkQueue};

    fn job_ids(count: usize) -> Vec<JobId> {
        let (queue, _events) = WorkQueue::new(1);
        (0..count).map(|_| {
                      let spec = JobSpec { kind:     JobKind::Thumbnail,
                                           priority: Priority::Background,
                                           label:    String::new(), };
                      queue.submit(spec, |_context| Ok(()))
                  })
                  .collect()
    }

    fn job(key: &str) -> ThumbnailJob {
        ThumbnailJob { key:   Arc::from(key),
                       url:   Arc::from("https://example.com/cover.jpg"),
                       bytes: ResultSlot::new(), }
    }

    #[test]
    fn a_new_job_is_pending_and_outstanding() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();

        jobs.add(ids[0], job("a"));

        assert_eq!(jobs.outstanding(), 1);
    }

    #[test]
    fn a_started_job_is_no_longer_pending_but_is_still_unfinished() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("a"));

        assert!(jobs.mark_started(ids[0]));

        assert_eq!(jobs.outstanding(), 1);
        assert_eq!(jobs.remove_pending("a"), None);
    }

    #[test]
    fn a_finished_job_is_returned_once_and_forgotten() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("a"));
        jobs.mark_started(ids[0]);

        let finished = jobs.finish(ids[0]);

        assert_eq!(finished.map(|job| job.key), Some(Arc::from("a")));
        assert!(jobs.finish(ids[0]).is_none());
        assert_eq!(jobs.outstanding(), 0);
    }

    #[test]
    fn removing_a_pending_job_returns_its_id_and_forgets_it() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("a"));

        assert_eq!(jobs.remove_pending("a"), Some(ids[0]));

        assert_eq!(jobs.outstanding(), 0);
        assert!(jobs.finish(ids[0]).is_none());
    }

    #[test]
    fn a_running_job_cannot_be_removed_as_pending() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("a"));
        jobs.mark_started(ids[0]);

        assert_eq!(jobs.remove_pending("a"), None);

        assert_eq!(jobs.outstanding(), 1);
    }

    #[test]
    fn removing_all_pending_leaves_running_jobs() {
        let ids = job_ids(3);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("running"));
        jobs.mark_started(ids[0]);
        jobs.add(ids[1], job("b"));
        jobs.add(ids[2], job("c"));

        let mut removed: Vec<_> = jobs.remove_all_pending()
                                      .into_iter()
                                      .map(|(key, _)| key.to_string())
                                      .collect();
        removed.sort();

        assert_eq!(removed, ["b", "c"]);
        assert_eq!(jobs.outstanding(), 1);
        assert!(jobs.finish(ids[0]).is_some());
    }

    #[test]
    fn a_replaced_jobs_late_events_do_not_disturb_its_replacement() {
        let ids = job_ids(2);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("a"));
        jobs.remove_pending("a");
        jobs.add(ids[1], job("a"));

        assert!(!jobs.mark_started(ids[0]));
        assert!(jobs.finish(ids[0]).is_none());

        assert!(jobs.mark_started(ids[1]));
    }

    #[test]
    fn events_for_unknown_jobs_are_ignored() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();

        assert!(!jobs.mark_started(ids[0]));
        assert!(jobs.finish(ids[0]).is_none());
    }
}
