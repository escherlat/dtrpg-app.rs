//! The handle a running job uses to talk back to the queue.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::mpsc::UnboundedSender;

use super::types::{JobEvent, JobId};

/// Given to a job's closure so it can see cancellation and report progress.
pub struct JobContext {
    id: JobId,
    cancel: Arc<AtomicBool>,
    events: UnboundedSender<JobEvent>,
}

impl JobContext {
    pub(super) fn new(
        id: JobId, cancel: Arc<AtomicBool>, events: UnboundedSender<JobEvent>,
    ) -> Self {
        Self { id, cancel, events }
    }

    /// The id of the job this context belongs to.
    #[must_use]
    pub const fn id(&self) -> JobId {
        self.id
    }

    /// Whether the job has been cancelled. A job should check this at its
    /// natural checkpoints and return early when it is `true`.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// The cancellation flag itself, for passing to code that takes one, such
    /// as [`retry_with_backoff`](crate::services::retry::retry_with_backoff).
    #[must_use]
    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancel
    }

    /// Reports `done` of `total` units complete, if the total is known.
    pub fn report_progress(&self, done: u64, total: Option<u64>) {
        // A closed channel means nobody is watching; the job carries on.
        let _ = self.events.send(JobEvent::Progress {
            id: self.id,
            done,
            total,
        });
    }
}
