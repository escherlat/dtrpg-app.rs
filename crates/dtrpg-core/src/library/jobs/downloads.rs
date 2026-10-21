//! File-download bookkeeping and submission, moved from the UI controller's
//! `download_jobs.rs`/`download_dispatch.rs` in ADR 0002 Slice 4.
//!
//! `Library::submit_download` takes the job closure as a parameter rather
//! than building it itself: the closure resolves the file's on-disk
//! destination via `dtrpg_ui`-only path-resolution helpers (collection
//! symlink placement, iCloud status) that have no reason to move into
//! `dtrpg_core` just for this. `Library` owns the queue submission and the
//! bookkeeping map; the controller still knows how to build one download's
//! closure.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError};

use crate::library::Library;
use crate::work::{JobContext, JobId, JobKind, JobResult, JobSpec, Priority, ResultSlot};

/// Identifies one file of a catalog entry: the entry's id and the file's
/// index.
pub type DownloadKey = (Arc<str>, u32);

/// What `Library` remembers about one download that has not finished.
pub struct DownloadJob {
    /// The file being downloaded.
    pub key:              DownloadKey,
    /// The entry's display title, for the activity label.
    pub title:            String,
    /// The file's name, for the activity label.
    pub file_name:        String,
    /// The order product the file belongs to, for logging.
    pub order_product_id: u64,
    /// Where the file is being written. The job fills this in once it has
    /// worked out the destination.
    pub dest:             ResultSlot<PathBuf>,
    /// The activity panel entry, once the download has started. Opaque to
    /// `Library` — it never interprets this id, only stores what the
    /// controller sets.
    pub activity_id:      Option<u64>,
}

/// Every download that has been submitted and has not finished.
#[derive(Default)]
pub(crate) struct DownloadJobs {
    jobs:   HashMap<JobId, DownloadJob>,
    /// The job for each file that is queued or downloading. A download that
    /// was cancelled while running is dropped from here at once, though its
    /// job stays in `jobs` until the queue reports it finished.
    by_key: HashMap<DownloadKey, JobId>,
}

impl DownloadJobs {
    fn add(&mut self, id: JobId, job: DownloadJob) {
        self.by_key.insert(job.key.clone(), id);
        self.jobs.insert(id, job);
    }

    fn get(&self, id: JobId) -> Option<&DownloadJob> {
        self.jobs.get(&id)
    }

    /// Whether no download is tracked at all, queued, running, or winding
    /// down after a cancel.
    fn is_idle(&self) -> bool {
        self.jobs.is_empty()
    }

    fn has_file(&self, item_id: &str, index: u32) -> bool {
        self.by_key
            .keys()
            .any(|(id, file_index)| id.as_ref() == item_id && *file_index == index)
    }

    fn has_item(&self, item_id: &str) -> bool {
        self.by_key.keys().any(|(id, _)| id.as_ref() == item_id)
    }

    fn item_ids(&self) -> Vec<Arc<str>> {
        self.by_key.keys().map(|(id, _)| Arc::clone(id)).collect()
    }

    fn set_activity(&mut self, id: JobId, activity_id: u64) {
        if let Some(job) = self.jobs.get_mut(&id) {
            job.activity_id = Some(activity_id);
        }
    }

    fn cancel(&mut self, key: &DownloadKey) -> Option<(JobId, Option<u64>)> {
        let id = self.by_key.remove(key)?;
        let activity_id = self.jobs.get(&id).and_then(|job| job.activity_id);
        if activity_id.is_none() {
            self.jobs.remove(&id);
        }
        Some((id, activity_id))
    }

    fn finish(&mut self, id: JobId) -> Option<DownloadJob> {
        let job = self.jobs.remove(&id)?;
        if self.by_key.get(&job.key) == Some(&id) {
            self.by_key.remove(&job.key);
        }
        Some(job)
    }
}

/// What [`Library::submit_download`] needs beyond the job closure itself.
pub struct DownloadSubmission {
    pub key:              DownloadKey,
    pub title:            String,
    pub file_name:        String,
    pub order_product_id: u64,
    pub priority:         Priority,
    pub label:            String,
    pub dest:             ResultSlot<PathBuf>,
}

impl Library {
    /// Submits a download job to the work queue and records it in
    /// `Library`'s bookkeeping. `job` is the fully-built closure (destination
    /// resolution, retry/progress reporting, and the actual
    /// `LibraryService::download_item` call); see this module's doc comment
    /// for why the controller still builds it.
    pub fn submit_download<F>(&self, submission: DownloadSubmission, job: F) -> JobId
        where F: FnOnce(&JobContext) -> JobResult + Send + 'static {
        let DownloadSubmission { key,
                                 title,
                                 file_name,
                                 order_product_id,
                                 priority,
                                 label,
                                 dest, } = submission;
        let spec = JobSpec { kind: JobKind::Download,
                             priority,
                             label };
        let id = self.work.submit(spec, job);
        self.download_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .add(id,
                 DownloadJob { key,
                               title,
                               file_name,
                               order_product_id,
                               dest,
                               activity_id: None });
        id
    }

    /// The download tracked under `id`'s title and file name, if it is one
    /// of ours.
    #[must_use]
    pub fn download_title_and_file(&self, id: JobId) -> Option<(String, String)> {
        self.download_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .map(|job| (job.title.clone(), job.file_name.clone()))
    }

    /// The activity panel entry for the download tracked under `id`, if any.
    #[must_use]
    pub fn download_activity_id(&self, id: JobId) -> Option<u64> {
        self.download_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .and_then(|job| job.activity_id)
    }

    /// Records the activity panel entry for a download that has started.
    pub fn download_set_activity(&self, id: JobId, activity_id: u64) {
        self.download_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .set_activity(id, activity_id);
    }

    /// Removes a finished download and returns it, or `None` if it is not
    /// one of ours or was already forgotten by a cancel.
    pub fn download_finished(&self, id: JobId) -> Option<DownloadJob> {
        self.download_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .finish(id)
    }

    /// Stops tracking a file the user cancelled, returning its job id so the
    /// caller can cancel it on the work queue, and its activity entry if it
    /// had started.
    #[must_use]
    pub fn cancel_download(&self, key: &DownloadKey) -> Option<(JobId, Option<u64>)> {
        let cancelled = self.download_jobs
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .cancel(key);
        if let Some((id, _)) = cancelled {
            self.work.cancel(id);
        }
        cancelled
    }

    /// Whether the file `(item_id, index)` is queued or downloading.
    #[must_use]
    pub fn is_file_queued_or_active(&self, item_id: &str, index: u32) -> bool {
        self.download_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .has_file(item_id, index)
    }

    /// Whether any file of the entry `item_id` is queued or downloading.
    #[must_use]
    pub fn is_downloading(&self, item_id: &str) -> bool {
        self.download_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .has_item(item_id)
    }

    /// The entry id of every queued or downloading file, repeating an entry
    /// once per file.
    #[must_use]
    pub fn downloading_item_ids(&self) -> Vec<Arc<str>> {
        self.download_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .item_ids()
    }

    /// Whether no download is tracked at all, queued, running, or winding
    /// down after a cancel.
    #[must_use]
    pub fn download_jobs_is_idle(&self) -> bool {
        self.download_jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_idle()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::work::{JobKind, JobSpec, Priority, WorkQueue};

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
        assert_eq!(jobs.item_ids().len(), 1);
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

        assert!(!jobs.has_file("a", 0));
        assert!(jobs.get(ids[0]).is_some());
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

        assert!(jobs.has_file("a", 0));
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
