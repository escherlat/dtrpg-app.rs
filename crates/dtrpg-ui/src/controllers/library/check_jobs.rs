//! Bookkeeping for availability checks submitted to the work queue.
//!
//! The controller keeps one of these so it can tell which items have a check
//! waiting or running, replace or cancel a waiting one, and match a queue
//! event back to the item it was for. It holds no GPUI types, so its
//! transitions are tested directly.

use std::{collections::HashMap, sync::Arc};

use dtrpg_core::{
    data::library::LibraryItem,
    services::LibraryServiceError,
    work::{JobId, ResultSlot},
};

/// What a check leaves for the controller: the item as the server now
/// describes it, or why it could not be fetched.
pub(super) type CheckResult = Result<LibraryItem, LibraryServiceError>;

/// What the controller remembers about one check that has not finished.
pub(super) struct CheckJob {
    /// The catalog entry being checked.
    pub(super) item_id: Arc<str>,
    /// Whether the check has left the queue and begun.
    started:            bool,
    /// Where the check leaves what the server said.
    pub(super) result:  ResultSlot<CheckResult>,
}

impl CheckJob {
    /// A check for `item_id` that has not started.
    pub(super) fn new(item_id: Arc<str>, result: ResultSlot<CheckResult>) -> Self {
        Self { item_id,
               started: false,
               result }
    }
}

/// Every availability check that has been submitted and has not finished.
#[derive(Default)]
pub(super) struct CheckJobs {
    jobs:    HashMap<JobId, CheckJob>,
    /// The check for each item that has one waiting or running. At most one.
    by_item: HashMap<Arc<str>, JobId>,
}

impl CheckJobs {
    /// Records a newly submitted check as waiting.
    pub(super) fn add(&mut self, id: JobId, job: CheckJob) {
        self.by_item.insert(Arc::clone(&job.item_id), id);
        self.jobs.insert(id, job);
    }

    /// Whether `item_id` has a check that has not started.
    pub(super) fn is_pending(&self, item_id: &str) -> bool {
        self.job_for(item_id).is_some_and(|job| !job.started)
    }

    /// Whether `item_id` has a check that is running.
    pub(super) fn is_running(&self, item_id: &str) -> bool {
        self.job_for(item_id).is_some_and(|job| job.started)
    }

    /// Forgets the waiting check for `item_id`, returning its id so it can be
    /// cancelled. A check that has already started is not affected.
    pub(super) fn remove_pending(&mut self, item_id: &str) -> Option<JobId> {
        if !self.is_pending(item_id) {
            return None;
        }
        let id = self.by_item.remove(item_id)?;
        self.jobs.remove(&id);
        Some(id)
    }

    /// Forgets every waiting check, returning each id so it can be cancelled.
    /// Checks that have started are left to finish.
    pub(super) fn remove_all_pending(&mut self) -> Vec<JobId> {
        let waiting: Vec<(Arc<str>, JobId)> = self.jobs
                                                  .iter()
                                                  .filter(|(_, job)| !job.started)
                                                  .map(|(id, job)| (Arc::clone(&job.item_id), *id))
                                                  .collect();
        for (item_id, id) in &waiting {
            self.by_item.remove(item_id);
            self.jobs.remove(id);
        }
        waiting.into_iter().map(|(_, id)| id).collect()
    }

    /// Notes that a check has started, returning the item it is for, or `None`
    /// if it is not one of ours.
    pub(super) fn mark_started(&mut self, id: JobId) -> Option<Arc<str>> {
        let job = self.jobs.get_mut(&id)?;
        job.started = true;
        Some(Arc::clone(&job.item_id))
    }

    /// Removes a finished check and returns it, or `None` if it is not one of
    /// ours or was already forgotten by a cancel.
    pub(super) fn finish(&mut self, id: JobId) -> Option<CheckJob> {
        let job = self.jobs.remove(&id)?;
        if self.by_item.get(&job.item_id) == Some(&id) {
            self.by_item.remove(&job.item_id);
        }
        Some(job)
    }

    fn job_for(&self, item_id: &str) -> Option<&CheckJob> {
        self.by_item.get(item_id).and_then(|id| self.jobs.get(id))
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
                      let spec = JobSpec { kind:     JobKind::Check,
                                           priority: Priority::Background,
                                           label:    String::new(), };
                      queue.submit(spec, |_context| Ok(()))
                  })
                  .collect()
    }

    fn job(item: &str) -> CheckJob {
        CheckJob::new(Arc::from(item), ResultSlot::new())
    }

    #[test]
    fn a_new_check_is_pending_not_running() {
        let ids = job_ids(1);
        let mut jobs = CheckJobs::default();

        jobs.add(ids[0], job("a"));

        assert!(jobs.is_pending("a"));
        assert!(!jobs.is_running("a"));
        assert!(!jobs.is_pending("b"));
    }

    #[test]
    fn a_started_check_is_running_and_names_its_item() {
        let ids = job_ids(1);
        let mut jobs = CheckJobs::default();
        jobs.add(ids[0], job("a"));

        assert_eq!(jobs.mark_started(ids[0]).as_deref(), Some("a"));

        assert!(jobs.is_running("a"));
        assert!(!jobs.is_pending("a"));
    }

    #[test]
    fn a_finished_check_is_returned_once_and_forgotten() {
        let ids = job_ids(1);
        let mut jobs = CheckJobs::default();
        jobs.add(ids[0], job("a"));
        jobs.mark_started(ids[0]);

        assert_eq!(jobs.finish(ids[0]).map(|job| job.item_id).as_deref(),
                   Some("a"));

        assert!(jobs.finish(ids[0]).is_none());
        assert!(!jobs.is_running("a"));
    }

    #[test]
    fn removing_a_pending_check_returns_its_id_and_forgets_it() {
        let ids = job_ids(1);
        let mut jobs = CheckJobs::default();
        jobs.add(ids[0], job("a"));

        assert_eq!(jobs.remove_pending("a"), Some(ids[0]));

        assert!(!jobs.is_pending("a"));
        assert!(jobs.finish(ids[0]).is_none());
    }

    #[test]
    fn a_running_check_cannot_be_removed_as_pending() {
        let ids = job_ids(1);
        let mut jobs = CheckJobs::default();
        jobs.add(ids[0], job("a"));
        jobs.mark_started(ids[0]);

        assert_eq!(jobs.remove_pending("a"), None);

        assert!(jobs.is_running("a"));
    }

    #[test]
    fn removing_all_pending_leaves_running_checks() {
        let ids = job_ids(3);
        let mut jobs = CheckJobs::default();
        jobs.add(ids[0], job("running"));
        jobs.mark_started(ids[0]);
        jobs.add(ids[1], job("b"));
        jobs.add(ids[2], job("c"));

        let mut removed = jobs.remove_all_pending();
        removed.sort_by_key(|id| id.to_string());

        let mut expected = vec![ids[1], ids[2]];
        expected.sort_by_key(|id| id.to_string());
        assert_eq!(removed, expected);
        assert!(!jobs.is_pending("b") && !jobs.is_pending("c"));
        assert!(jobs.is_running("running"));
    }

    #[test]
    fn a_replaced_checks_late_events_do_not_disturb_its_replacement() {
        let ids = job_ids(2);
        let mut jobs = CheckJobs::default();
        jobs.add(ids[0], job("a"));
        jobs.remove_pending("a");
        jobs.add(ids[1], job("a"));

        assert!(jobs.mark_started(ids[0]).is_none());
        assert!(jobs.finish(ids[0]).is_none());

        assert!(jobs.is_pending("a"), "the replacement is still waiting");
    }

    #[test]
    fn events_for_unknown_checks_are_ignored() {
        let ids = job_ids(1);
        let mut jobs = CheckJobs::default();

        assert!(jobs.mark_started(ids[0]).is_none());
        assert!(jobs.finish(ids[0]).is_none());
    }
}
