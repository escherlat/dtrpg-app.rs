//! A queue for background work: thumbnail fetches, file downloads, and
//! availability checks.
//!
//! The queue owns scheduling. It limits how many jobs run at once, runs
//! interactive jobs before background ones, lets pending or running jobs be
//! cancelled, and reports each job's progress. It knows nothing about the UI:
//! callers submit jobs and read [`JobEvent`]s from a channel. See
//! `docs/adr/0007-core-work-queue.md` for the design.

mod context;
mod queue;
mod state;
mod types;

#[cfg(test)]
mod tests;

pub use context::JobContext;
pub use queue::WorkQueue;
pub use types::{JobEvent, JobId, JobKind, JobResult, JobSpec, Outcome, Priority};
