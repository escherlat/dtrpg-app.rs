//! Job-queue orchestration: ADR 0002 Slice 4.
//!
//! `Library` owns the three job-tracking maps (thumbnail fetches, file
//! downloads, availability checks) the UI controller used to hold, plus
//! their submit/cancel/finish methods, all operating on `self.work` (the
//! `WorkQueue` Slice 1 already moved here). The existing `WorkQueue`
//! `JobEvent` channel (flowing core → controller since Slice 1) is the
//! controller's reaction point: it calls the query/finish methods here
//! instead of touching bookkeeping fields directly, then does whatever is
//! left that only the UI can do (writing decoded bytes into `CoverCache`,
//! driving `ActivityController`).

mod checks;
mod downloads;
mod thumbnails;

pub(super) use checks::CheckJobs;
pub use checks::{CheckJob, CheckResult};
pub(super) use downloads::DownloadJobs;
pub use downloads::{DownloadJob, DownloadKey, DownloadSubmission};
pub use thumbnails::ThumbnailJob;
pub(super) use thumbnails::ThumbnailJobs;
