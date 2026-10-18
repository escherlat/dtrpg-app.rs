//! The vocabulary shared by the queue and its callers.

use std::{fmt, time::Duration};

/// Identifies one submitted job. Ids are unique for the life of a queue.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct JobId(pub(super) u64);

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "job-{}", self.0)
    }
}

/// What a job does. The kind decides which per-kind cap applies.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JobKind {
    /// Fetch one cover thumbnail.
    Thumbnail,
    /// Download one file.
    Download,
    /// Check one item's availability against the server.
    Check,
}

impl JobKind {
    /// How many jobs of this kind may run at once, or `None` when only the
    /// queue's overall limit applies.
    ///
    /// Availability checks run one at a time, so a batch of them cannot hit
    /// the API in parallel.
    pub(super) const fn max_concurrent(self) -> Option<usize> {
        match self {
            Self::Check => Some(1),
            Self::Thumbnail | Self::Download => None,
        }
    }
}

/// Which pending jobs are dispatched first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Priority {
    /// Work the user is waiting on. Dispatched before any background work.
    Interactive,
    /// Work nobody is waiting on, such as automatic thumbnail loading.
    Background,
}

/// Describes a job to the queue.
#[derive(Clone, Debug)]
pub struct JobSpec {
    /// What the job does.
    pub kind: JobKind,
    /// How soon it should run relative to other pending jobs.
    pub priority: Priority,
    /// A short description, for display by whoever observes the queue.
    pub label: String,
}

/// What a job's closure returns. `Err` carries a message for the user or log.
pub type JobResult = Result<(), String>;

/// How a job ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// The job ran to completion.
    Succeeded,
    /// The job returned an error or panicked.
    Failed(String),
    /// The job was cancelled, before or while it ran.
    Cancelled,
}

/// The receiving end of a queue's [`JobEvent`] stream. Read it with
/// `recv().await` from async code, or `blocking_recv()` from a plain thread.
pub type JobEvents = tokio::sync::mpsc::UnboundedReceiver<JobEvent>;

/// A change in a job's lifecycle.
///
/// Events for one job arrive in order: `Started`, any number of `Progress` and
/// `Retrying`, then `Finished`. A job cancelled while pending only ever produces
/// `Finished`. Events from different jobs may interleave, and an event can
/// arrive before the call that submitted its job has returned.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobEvent {
    /// The job left the pending list and began running.
    Started {
        /// The job that started.
        id: JobId,
    },
    /// The job reported how far along it is.
    Progress {
        /// The job reporting.
        id: JobId,
        /// Units completed so far, such as bytes.
        done: u64,
        /// Total units, when known.
        total: Option<u64>,
    },
    /// The job's latest attempt failed and it will try again after `delay`.
    Retrying {
        /// The job retrying.
        id: JobId,
        /// The attempt about to be made, counting the first as 1.
        attempt: u32,
        /// How long the job waits before that attempt.
        delay: Duration,
    },
    /// The job is over and its slot is free.
    Finished {
        /// The job that finished.
        id: JobId,
        /// How it ended.
        outcome: Outcome,
    },
}
