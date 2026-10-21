//! Availability-check bookkeeping and submission, moved from the UI
//! controller's `check_jobs.rs`/`checks.rs` in ADR 0002 Slice 4.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, PoisonError};

use crate::data::library::LibraryItem;
use crate::library::Library;
use crate::services::{LibraryService, LibraryServiceError};
use crate::work::{JobId, JobKind, JobSpec, Priority, ResultSlot};

/// What a check leaves for the caller: the item as the server now describes
/// it, or why it could not be fetched.
pub type CheckResult = Result<LibraryItem, LibraryServiceError>;

/// What `Library` remembers about one check that has not finished.
pub struct CheckJob {
    /// The catalog entry being checked.
    pub item_id: Arc<str>,
    /// Whether the check has left the queue and begun.
    started:     bool,
    /// Where the check leaves what the server said.
    pub result:  ResultSlot<CheckResult>,
}

impl CheckJob {
    fn new(item_id: Arc<str>, result: ResultSlot<CheckResult>) -> Self {
        Self { item_id,
               started: false,
               result }
    }
}

/// Every availability check that has been submitted and has not finished.
#[derive(Default)]
pub(crate) struct CheckJobs {
    jobs:    HashMap<JobId, CheckJob>,
    /// The check for each item that has one waiting or running. At most one.
    by_item: HashMap<Arc<str>, JobId>,
}

impl CheckJobs {
    fn add(&mut self, id: JobId, job: CheckJob) {
        self.by_item.insert(Arc::clone(&job.item_id), id);
        self.jobs.insert(id, job);
    }

    fn is_pending(&self, item_id: &str) -> bool {
        self.job_for(item_id).is_some_and(|job| !job.started)
    }

    fn is_running(&self, item_id: &str) -> bool {
        self.job_for(item_id).is_some_and(|job| job.started)
    }

    fn remove_pending(&mut self, item_id: &str) -> Option<JobId> {
        if !self.is_pending(item_id) {
            return None;
        }
        let id = self.by_item.remove(item_id)?;
        self.jobs.remove(&id);
        Some(id)
    }

    fn remove_all_pending(&mut self) -> Vec<JobId> {
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

    fn mark_started(&mut self, id: JobId) -> Option<Arc<str>> {
        let job = self.jobs.get_mut(&id)?;
        job.started = true;
        Some(Arc::clone(&job.item_id))
    }

    fn finish(&mut self, id: JobId) -> Option<CheckJob> {
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

impl Library {
    /// Submits an availability check for `id` to the work queue, bypassing
    /// the per-item cooldown (callers are expected to have already filtered
    /// by staleness). No-ops if a check for `id` is already running. A check
    /// already waiting is left alone, unless `priority` is `Interactive`, in
    /// which case it is replaced so the user's request is not stuck behind a
    /// background one.
    pub fn submit_check(&self, id: Arc<str>, numeric_id: u64, priority: Priority,
                        service: Arc<dyn LibraryService>) {
        let mut jobs = self.check_jobs
                           .lock()
                           .unwrap_or_else(PoisonError::into_inner);
        if jobs.is_running(&id) {
            return;
        }
        if jobs.is_pending(&id) {
            if priority != Priority::Interactive {
                return;
            }
            if let Some(waiting) = jobs.remove_pending(&id) {
                self.work.cancel(waiting);
            }
        }
        drop(jobs);

        let result = ResultSlot::new();
        let job_result = result.clone();
        let spec = JobSpec { kind: JobKind::Check,
                             priority,
                             label: id.to_string() };
        let job_id = self.work.submit(spec, move |_context| {
                                  // A failed check is still a result: the
                                  // caller decides from the error whether
                                  // the item is gone or the network was
                                  // merely unavailable.
                                  job_result.put(service.get_item(numeric_id));
                                  Ok(())
                              });
        self.check_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .add(job_id, CheckJob::new(id, result));
    }

    /// Notes that a check has started, returning the item it is for (and
    /// recording it in the "currently checking" set), or `None` if it is not
    /// one of ours.
    pub fn check_started(&self, id: JobId) -> Option<Arc<str>> {
        let item_id = self.check_jobs
                          .lock()
                          .unwrap_or_else(PoisonError::into_inner)
                          .mark_started(id)?;
        self.checking_items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(Arc::clone(&item_id));
        Some(item_id)
    }

    /// Removes a finished check and returns it (and its item from the
    /// "currently checking" set), or `None` if it is not one of ours or was
    /// already forgotten by a cancel.
    pub fn check_finished(&self, id: JobId) -> Option<CheckJob> {
        let job = self.check_jobs
                      .lock()
                      .unwrap_or_else(PoisonError::into_inner)
                      .finish(id)?;
        self.checking_items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&job.item_id);
        Some(job)
    }

    /// Cancels every availability check that has not started. A check
    /// already running is left to finish.
    pub fn cancel_pending_checks(&self) {
        let pending = self.check_jobs
                          .lock()
                          .unwrap_or_else(PoisonError::into_inner)
                          .remove_all_pending();
        for id in pending {
            self.work.cancel(id);
        }
    }

    /// Returns `true` if an availability check is currently in flight for
    /// catalog item `id` (on-demand or from the periodic queue).
    #[must_use]
    pub fn is_checking(&self, id: &str) -> bool {
        self.checking_items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(id)
    }

    /// Returns a snapshot of every item id with an availability check
    /// currently in flight.
    #[must_use]
    pub fn checking_items_snapshot(&self) -> HashSet<Arc<str>> {
        self.checking_items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::{JobKind, JobSpec, Priority, WorkQueue};

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
        removed.sort_by_key(ToString::to_string);

        let mut expected = vec![ids[1], ids[2]];
        expected.sort_by_key(ToString::to_string);
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

        assert!(jobs.is_pending("a"));
    }

    #[test]
    fn events_for_unknown_checks_are_ignored() {
        let ids = job_ids(1);
        let mut jobs = CheckJobs::default();

        assert!(jobs.mark_started(ids[0]).is_none());
        assert!(jobs.finish(ids[0]).is_none());
    }
}
