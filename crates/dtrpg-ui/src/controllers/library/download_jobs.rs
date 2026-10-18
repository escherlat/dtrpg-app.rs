//! Bookkeeping for file downloads submitted to the work queue.
//!
//! The controller keeps one of these so it can tell which files are queued or
//! downloading, cancel them, and match a queue event back to the file it was
//! for. It holds no GPUI types, so its transitions are tested directly.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use dtrpg_core::work::{JobId, ResultSlot};

/// Identifies one file of a catalog entry: the entry's id and the file's index.
pub(super) type DownloadKey = (Arc<str>, u32);

/// What the controller remembers about one download that has not finished.
pub(super) struct DownloadJob {
    /// The file being downloaded.
    pub(super) key:              DownloadKey,
    /// The entry's display title, for the activity label.
    pub(super) title:            String,
    /// The file's name, for the activity label.
    pub(super) file_name:        String,
    /// The order product the file belongs to, for logging.
    pub(super) order_product_id: u64,
    /// Where the file is being written. The job fills this in once it has
    /// worked out the destination.
    pub(super) dest:             ResultSlot<PathBuf>,
    /// The activity panel entry, once the download has started.
    pub(super) activity_id:      Option<u64>,
}

/// Every download that has been submitted and has not finished.
#[derive(Default)]
pub(super) struct DownloadJobs {
    jobs:   HashMap<JobId, DownloadJob>,
    /// The job for each file that is queued or downloading. A download that
    /// was cancelled while running is dropped from here at once, though its
    /// job stays in `jobs` until the queue reports it finished.
    by_key: HashMap<DownloadKey, JobId>,
}

impl DownloadJobs {
    /// Records a newly submitted download as queued.
    pub(super) fn add(&mut self, id: JobId, job: DownloadJob) {
        self.by_key.insert(job.key.clone(), id);
        self.jobs.insert(id, job);
    }

    /// Whether no download is tracked at all, queued, running, or winding down
    /// after a cancel.
    #[cfg(test)]
    pub(super) fn is_idle(&self) -> bool {
        self.jobs.is_empty()
    }

    /// The download tracked under `id`, if it is one of ours.
    pub(super) fn get(&self, id: JobId) -> Option<&DownloadJob> {
        self.jobs.get(&id)
    }

    /// Whether the file `(item_id, index)` is queued or downloading.
    pub(super) fn has_file(&self, item_id: &str, index: u32) -> bool {
        self.by_key
            .keys()
            .any(|(id, file_index)| id.as_ref() == item_id && *file_index == index)
    }

    /// Whether any file of the entry `item_id` is queued or downloading.
    pub(super) fn has_item(&self, item_id: &str) -> bool {
        self.by_key.keys().any(|(id, _)| id.as_ref() == item_id)
    }

    /// The entry id of every queued or downloading file, repeating an entry
    /// once per file.
    pub(super) fn item_ids(&self) -> impl Iterator<Item = &Arc<str>> {
        self.by_key.keys().map(|(id, _)| id)
    }

    /// Records the activity panel entry for a download that has started.
    pub(super) fn set_activity(&mut self, id: JobId, activity_id: u64) {
        if let Some(job) = self.jobs.get_mut(&id) {
            job.activity_id = Some(activity_id);
        }
    }

    /// Stops tracking a file the user cancelled, returning its job id so the
    /// queue can cancel it, and its activity entry if it had started.
    ///
    /// A download that has not started is forgotten entirely. One that has
    /// started stays known until the queue reports it finished, so that
    /// report can be recognized.
    pub(super) fn cancel(&mut self, key: &DownloadKey) -> Option<(JobId, Option<u64>)> {
        let id = self.by_key.remove(key)?;
        let activity_id = self.jobs.get(&id).and_then(|job| job.activity_id);
        if activity_id.is_none() {
            self.jobs.remove(&id);
        }
        Some((id, activity_id))
    }

    /// Removes a finished download and returns it, or `None` if it is not one
    /// of ours or was already forgotten by a cancel.
    pub(super) fn finish(&mut self, id: JobId) -> Option<DownloadJob> {
        let job = self.jobs.remove(&id)?;
        if self.by_key.get(&job.key) == Some(&id) {
            self.by_key.remove(&job.key);
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
                      let spec = JobSpec { kind:     JobKind::Download,
                                           priority: Priority::Interactive,
                                           label:    String::new(), };
                      queue.submit(spec, |_context| Ok(()))
                  })
                  .collect()
    }

    fn key(item: &str, index: u32) -> DownloadKey {
        (Arc::from(item), index)
    }

    fn job(item: &str, index: u32) -> DownloadJob {
        DownloadJob { key:              key(item, index),
                      title:            "Title".to_owned(),
                      file_name:        "File.pdf".to_owned(),
                      order_product_id: 7,
                      dest:             ResultSlot::new(),
                      activity_id:      None, }
    }

    #[test]
    fn a_queued_file_is_reported_by_file_and_by_entry() {
        let ids = job_ids(1);
        let mut jobs = DownloadJobs::default();

        jobs.add(ids[0], job("a", 1));

        assert!(jobs.has_file("a", 1));
        assert!(!jobs.has_file("a", 0));
        assert!(jobs.has_item("a"));
        assert!(!jobs.has_item("b"));
        assert_eq!(jobs.item_ids().count(), 1);
    }

    #[test]
    fn a_finished_download_is_returned_once_and_forgotten() {
        let ids = job_ids(1);
        let mut jobs = DownloadJobs::default();
        jobs.add(ids[0], job("a", 0));

        assert_eq!(jobs.finish(ids[0]).map(|job| job.key), Some(key("a", 0)));

        assert!(jobs.finish(ids[0]).is_none());
        assert!(!jobs.has_file("a", 0));
    }

    #[test]
    fn cancelling_a_queued_download_forgets_it_entirely() {
        let ids = job_ids(1);
        let mut jobs = DownloadJobs::default();
        jobs.add(ids[0], job("a", 0));

        assert_eq!(jobs.cancel(&key("a", 0)), Some((ids[0], None)));

        assert!(!jobs.has_file("a", 0));
        assert!(jobs.get(ids[0]).is_none());
        assert!(jobs.finish(ids[0]).is_none());
    }

    #[test]
    fn cancelling_a_running_download_returns_its_activity_and_keeps_the_job_until_it_finishes() {
        let ids = job_ids(1);
        let mut jobs = DownloadJobs::default();
        jobs.add(ids[0], job("a", 0));
        jobs.set_activity(ids[0], 42);

        assert_eq!(jobs.cancel(&key("a", 0)), Some((ids[0], Some(42))));

        assert!(!jobs.has_file("a", 0),
                "the file no longer counts as downloading");
        assert!(jobs.get(ids[0]).is_some(),
                "the job stays known until the queue finishes it");
        assert!(jobs.finish(ids[0]).is_some());
    }

    #[test]
    fn cancelling_a_file_that_is_not_tracked_does_nothing() {
        let mut jobs = DownloadJobs::default();

        assert_eq!(jobs.cancel(&key("a", 0)), None);
    }

    #[test]
    fn a_cancelled_jobs_late_finish_does_not_disturb_its_replacement() {
        let ids = job_ids(2);
        let mut jobs = DownloadJobs::default();
        jobs.add(ids[0], job("a", 0));
        jobs.set_activity(ids[0], 1);
        jobs.cancel(&key("a", 0));
        jobs.add(ids[1], job("a", 0));

        assert!(jobs.finish(ids[0]).is_some());

        assert!(jobs.has_file("a", 0),
                "the new download for the same file is still tracked");
        assert!(jobs.finish(ids[1]).is_some());
    }

    #[test]
    fn two_files_of_one_entry_are_tracked_independently() {
        let ids = job_ids(2);
        let mut jobs = DownloadJobs::default();
        jobs.add(ids[0], job("a", 0));
        jobs.add(ids[1], job("a", 1));

        jobs.cancel(&key("a", 0));

        assert!(!jobs.has_file("a", 0));
        assert!(jobs.has_file("a", 1));
        assert!(jobs.has_item("a"));
    }

    #[test]
    fn the_activity_entry_is_recorded_on_the_job() {
        let ids = job_ids(1);
        let mut jobs = DownloadJobs::default();
        jobs.add(ids[0], job("a", 0));

        jobs.set_activity(ids[0], 9);

        assert_eq!(jobs.get(ids[0]).and_then(|job| job.activity_id), Some(9));
    }
}
