//! Bookkeeping for thumbnail fetches submitted to the work queue.
//!
//! The controller keeps one of these so it can tell which fetches are still
//! waiting, cancel them, and match a queue event back to the cover it was for.
//! It holds no GPUI types, so its transitions are tested directly.

use std::{collections::HashMap, sync::Arc};

use dtrpg_core::work::{JobId, ResultSlot};

/// What the controller remembers about one unfinished thumbnail fetch.
pub(super) struct ThumbnailJob {
    /// The cover's cache key: an item id, or a detail-cover key.
    pub(super) key:   Arc<str>,
    /// The URL being fetched, for logging.
    pub(super) url:   Arc<str>,
    /// Where the fetch leaves the image bytes when it succeeds.
    pub(super) bytes: ResultSlot<Vec<u8>>,
}

/// Every thumbnail fetch that has been submitted and has not finished.
#[derive(Default)]
pub(super) struct ThumbnailJobs {
    jobs:    HashMap<JobId, ThumbnailJob>,
    /// The jobs that have not started yet, by cache key. At most one per key.
    pending: HashMap<Arc<str>, JobId>,
}

impl ThumbnailJobs {
    /// Records a newly submitted job as pending.
    pub(super) fn add(&mut self, id: JobId, job: ThumbnailJob) {
        self.pending.insert(Arc::clone(&job.key), id);
        self.jobs.insert(id, job);
    }

    /// How many fetches are waiting to start.
    #[cfg(test)]
    pub(super) fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// How many fetches have not finished, whether waiting or running.
    pub(super) fn outstanding(&self) -> usize {
        self.jobs.len()
    }

    /// Whether no fetch is waiting or running.
    pub(super) fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// Forgets the pending job for `key`, returning its id so it can be
    /// cancelled. A job that has already started is not affected.
    pub(super) fn remove_pending(&mut self, key: &str) -> Option<JobId> {
        let id = self.pending.remove(key)?;
        self.jobs.remove(&id);
        Some(id)
    }

    /// Forgets every pending job, returning each with its key.
    pub(super) fn remove_all_pending(&mut self) -> Vec<(Arc<str>, JobId)> {
        let pending: Vec<_> = self.pending.drain().collect();
        for (_, id) in &pending {
            self.jobs.remove(id);
        }
        pending
    }

    /// Notes that a job has started, so it no longer counts as pending.
    /// Returns whether the job is one of ours.
    pub(super) fn mark_started(&mut self, id: JobId) -> bool {
        let Some(job) = self.jobs.get(&id)
        else {
            return false;
        };
        if self.pending.get(&job.key) == Some(&id) {
            self.pending.remove(&job.key);
        }
        true
    }

    /// Removes a finished job and returns it, or `None` if it is not one of
    /// ours or was already forgotten by a cancel.
    pub(super) fn finish(&mut self, id: JobId) -> Option<ThumbnailJob> {
        let job = self.jobs.remove(&id)?;
        if self.pending.get(&job.key) == Some(&id) {
            self.pending.remove(&job.key);
        }
        Some(job)
    }
}

#[cfg(test)]
mod tests {
    use dtrpg_core::work::{JobKind, JobSpec, Priority, WorkQueue};

    use super::*;

    /// Mints distinct job ids by submitting no-op jobs to a throwaway queue.
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
    fn a_new_job_is_pending() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();

        jobs.add(ids[0], job("a"));

        assert_eq!(jobs.pending_count(), 1);
        assert_eq!(jobs.outstanding(), 1);
        assert!(!jobs.is_empty());
    }

    #[test]
    fn a_started_job_is_no_longer_pending_but_is_still_unfinished() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("a"));

        assert!(jobs.mark_started(ids[0]));

        assert_eq!(jobs.pending_count(), 0);
        assert_eq!(jobs.outstanding(), 1);
        assert!(!jobs.is_empty());
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
        assert!(jobs.is_empty());
    }

    #[test]
    fn a_job_that_finishes_without_starting_is_no_longer_pending() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("a"));

        jobs.finish(ids[0]);

        assert_eq!(jobs.pending_count(), 0);
    }

    #[test]
    fn removing_a_pending_job_returns_its_id_and_forgets_it() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("a"));

        assert_eq!(jobs.remove_pending("a"), Some(ids[0]));

        assert!(jobs.is_empty());
        assert!(jobs.finish(ids[0]).is_none());
    }

    #[test]
    fn a_running_job_cannot_be_removed_as_pending() {
        let ids = job_ids(1);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("a"));
        jobs.mark_started(ids[0]);

        assert_eq!(jobs.remove_pending("a"), None);

        assert!(!jobs.is_empty());
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
        assert_eq!(jobs.pending_count(), 0);
        assert!(!jobs.is_empty());
        assert!(jobs.finish(ids[0]).is_some());
    }

    #[test]
    fn a_replaced_jobs_late_events_do_not_disturb_its_replacement() {
        let ids = job_ids(2);
        let mut jobs = ThumbnailJobs::default();
        jobs.add(ids[0], job("a"));
        jobs.remove_pending("a");
        jobs.add(ids[1], job("a"));

        // The cancelled job's own events arrive afterwards.
        assert!(!jobs.mark_started(ids[0]));
        assert!(jobs.finish(ids[0]).is_none());

        assert_eq!(jobs.pending_count(), 1);
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
