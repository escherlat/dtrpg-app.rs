//! The queue's bookkeeping: what is pending, what is running, and which
//! pending job may start next. Pure data, no threads, so it is easy to test.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, atomic::AtomicBool},
};

use super::{
    context::JobContext,
    types::{JobId, JobKind, JobResult, JobSpec, Priority},
};

/// A job's closure, boxed so jobs of any shape share one list.
pub(super) type Job = Box<dyn FnOnce(&JobContext) -> JobResult + Send + 'static>;

/// A submitted job that has not started yet.
pub(super) struct Pending {
    pub(super) id: JobId,
    pub(super) spec: JobSpec,
    pub(super) job: Job,
    pub(super) cancel: Arc<AtomicBool>,
}

/// What the queue remembers about a job that is running.
struct Running {
    kind: JobKind,
    cancel: Arc<AtomicBool>,
}

pub(super) struct State {
    limit: usize,
    next_id: u64,
    interactive: VecDeque<Pending>,
    background: VecDeque<Pending>,
    running: HashMap<JobId, Running>,
}

impl State {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            next_id: 0,
            interactive: VecDeque::new(),
            background: VecDeque::new(),
            running: HashMap::new(),
        }
    }

    pub(super) fn next_job_id(&mut self) -> JobId {
        self.next_id += 1;
        JobId(self.next_id)
    }

    /// Sets how many jobs may run at once. Values below 1 are treated as 1,
    /// because a limit of 0 would stop the queue for good.
    pub(super) fn set_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
    }

    pub(super) fn push(&mut self, pending: Pending) {
        match pending.spec.priority {
            Priority::Interactive => self.interactive.push_back(pending),
            Priority::Background => self.background.push_back(pending),
        }
    }

    /// Removes the next job that may start now and records it as running.
    ///
    /// Interactive jobs come before background ones, and order is first in,
    /// first out within each. A job that cannot start yet, because its kind is
    /// at its cap or the overall limit is reached, is skipped, not waited for,
    /// so it cannot block jobs that can start behind it.
    pub(super) fn take_runnable(&mut self) -> Option<Pending> {
        let pending = match self.first_runnable(&self.interactive) {
            Some(position) => self.interactive.remove(position),
            None => self.first_runnable(&self.background)
                        .and_then(|position| self.background.remove(position)),
        }?;
        self.running.insert(pending.id,
                            Running { kind: pending.spec.kind,
                                      cancel: Arc::clone(&pending.cancel) });
        Some(pending)
    }

    /// Removes a job that has not started. Returns whether it was pending.
    pub(super) fn remove_pending(&mut self, id: JobId) -> bool {
        [&mut self.interactive, &mut self.background].into_iter()
                                                     .any(|lane| {
                                                         let before = lane.len();
                                                         lane.retain(|pending| pending.id != id);
                                                         lane.len() != before
                                                     })
    }

    /// Asks a running job to stop. Does nothing if the job is not running.
    pub(super) fn signal_cancel(&self, id: JobId) {
        if let Some(running) = self.running.get(&id) {
            running.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    /// Frees the slot held by a job that has ended.
    pub(super) fn finish(&mut self, id: JobId) {
        self.running.remove(&id);
    }

    fn first_runnable(&self, lane: &VecDeque<Pending>) -> Option<usize> {
        lane.iter()
            .position(|pending| self.kind_has_room(pending.spec.kind))
    }

    /// Whether a job of `kind` may start now: its own cap, if it has one, and
    /// the overall limit, if it counts toward it.
    fn kind_has_room(&self, kind: JobKind) -> bool {
        let under_cap = kind.max_concurrent().is_none_or(|cap| self.running_of(kind) < cap);
        let under_limit = !kind.counts_toward_limit() || self.running_toward_limit() < self.limit;
        under_cap && under_limit
    }

    fn running_of(&self, kind: JobKind) -> usize {
        self.running.values().filter(|running| running.kind == kind).count()
    }

    fn running_toward_limit(&self) -> usize {
        self.running
            .values()
            .filter(|running| running.kind.counts_toward_limit())
            .count()
    }
}
