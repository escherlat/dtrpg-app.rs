//! The queue itself: submission, dispatch onto the runtime, and completion.

use std::{
    panic::{self, AssertUnwindSafe},
    sync::{Arc, Mutex, MutexGuard, PoisonError, atomic::AtomicBool},
    thread,
};

use tokio::{
    runtime::Handle,
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
};

use super::{
    context::JobContext,
    state::{Pending, State},
    types::{JobEvent, JobId, JobResult, JobSpec, Outcome},
};

/// Runs submitted jobs on the shared runtime, within a concurrency limit.
///
/// Cloning a `WorkQueue` gives another handle to the same queue.
#[derive(Clone)]
pub struct WorkQueue {
    shared: Arc<Shared>,
}

struct Shared {
    state: Mutex<State>,
    runtime: Handle,
    events: UnboundedSender<JobEvent>,
}

impl WorkQueue {
    /// Creates a queue that runs at most `limit` jobs at once on `runtime`,
    /// and the receiver for its [`JobEvent`]s.
    ///
    /// A `limit` below 1 is treated as 1. The receiver is unbounded, so the
    /// caller must keep reading it, or drop it if it does not want events; the
    /// queue keeps working either way.
    ///
    /// # Examples
    ///
    /// ```
    /// use dtrpg_core::{runtime, work::WorkQueue};
    ///
    /// let (queue, _events) = WorkQueue::new(runtime::handle()?, 3);
    /// # drop(queue);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    #[must_use]
    pub fn new(runtime: Handle, limit: usize) -> (Self, UnboundedReceiver<JobEvent>) {
        let (events, receiver) = mpsc::unbounded_channel();
        let shared = Shared { state: Mutex::new(State::new(limit)),
                              runtime,
                              events };
        (Self { shared: Arc::new(shared) }, receiver)
    }

    /// Adds a job to the queue and returns its id. The job starts as soon as
    /// the limits allow, which may be before this call returns.
    ///
    /// The closure runs on a blocking thread, so it may block. It should check
    /// [`JobContext::is_cancelled`] at its checkpoints.
    ///
    /// # Examples
    ///
    /// ```
    /// use dtrpg_core::{
    ///     runtime,
    ///     work::{JobEvent, JobKind, JobSpec, Outcome, Priority, WorkQueue},
    /// };
    ///
    /// let (queue, mut events) = WorkQueue::new(runtime::handle()?, 3);
    /// let spec = JobSpec { kind: JobKind::Download,
    ///                      priority: Priority::Interactive,
    ///                      label: "Example".to_owned() };
    /// let id = queue.submit(spec, |_context| Ok(()));
    ///
    /// assert_eq!(events.blocking_recv(), Some(JobEvent::Started { id }));
    /// assert_eq!(events.blocking_recv(),
    ///            Some(JobEvent::Finished { id, outcome: Outcome::Succeeded }));
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn submit<F>(&self, spec: JobSpec, job: F) -> JobId
        where F: FnOnce(&JobContext) -> JobResult + Send + 'static {
        let id = {
            let mut state = self.shared.lock();
            let id = state.next_job_id();
            state.push(Pending { id,
                                 spec,
                                 job: Box::new(job),
                                 cancel: Arc::new(AtomicBool::new(false)) });
            id
        };
        self.shared.dispatch();
        id
    }

    /// Cancels a job.
    ///
    /// A pending job is removed and never starts; it produces a single
    /// `Finished` event with [`Outcome::Cancelled`]. A running job has its
    /// cancellation flag set and ends when its closure next returns. An
    /// unknown or finished job is ignored.
    pub fn cancel(&self, id: JobId) {
        let was_pending = {
            let mut state = self.shared.lock();
            let was_pending = state.remove_pending(id);
            if !was_pending {
                state.signal_cancel(id);
            }
            was_pending
        };
        if was_pending {
            self.shared.emit(JobEvent::Finished { id,
                                                  outcome: Outcome::Cancelled });
        }
    }

    /// Changes how many jobs may run at once, starting waiting jobs if the new
    /// limit has room. Jobs already running are not interrupted by a lower
    /// limit. A `limit` below 1 is treated as 1.
    pub fn set_limit(&self, limit: usize) {
        self.shared.lock().set_limit(limit);
        self.shared.dispatch();
    }
}

impl Shared {
    /// Locks the state. A panic while holding the lock cannot leave the
    /// bookkeeping half-updated, so a poisoned lock is safe to keep using.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn emit(&self, event: JobEvent) {
        // A closed channel means nobody is watching; the queue carries on.
        let _ = self.events.send(event);
    }

    /// Starts every pending job that the limits currently allow.
    fn dispatch(self: &Arc<Self>) {
        loop {
            let next = self.lock().take_runnable();
            let Some(pending) = next else {
                break;
            };
            self.start(pending);
        }
    }

    fn start(self: &Arc<Self>, pending: Pending) {
        let Pending { id, job, cancel, .. } = pending;
        let context = JobContext::new(id, Arc::clone(&cancel), self.events.clone());
        self.emit(JobEvent::Started { id });

        let shared = Arc::clone(self);
        // The join handle is dropped on purpose: a blocking task cannot be
        // aborted, and the queue tracks every job to its `Finished` event.
        drop(self.runtime.spawn_blocking(move || {
                             // The closure is consumed by the call, so nothing
                             // it touched is observed in a broken state afterwards.
                             let result = panic::catch_unwind(AssertUnwindSafe(|| job(&context)));
                             shared.finish(id, &cancel, result);
                         }));
    }

    /// Frees the job's slot, reports how it ended, and starts the next job.
    fn finish(self: &Arc<Self>, id: JobId, cancel: &AtomicBool, result: thread::Result<JobResult>) {
        let cancelled = cancel.load(std::sync::atomic::Ordering::SeqCst);
        self.lock().finish(id);
        self.emit(JobEvent::Finished { id,
                                       outcome: outcome_of(id, cancelled, result) });
        self.dispatch();
    }
}

fn outcome_of(id: JobId, cancelled: bool, result: thread::Result<JobResult>) -> Outcome {
    if cancelled {
        return Outcome::Cancelled;
    }
    match result {
        Ok(Ok(())) => Outcome::Succeeded,
        Ok(Err(message)) => Outcome::Failed(message),
        Err(_) => {
            tracing::error!(%id, "work queue job panicked");
            Outcome::Failed("the job panicked".to_owned())
        }
    }
}
