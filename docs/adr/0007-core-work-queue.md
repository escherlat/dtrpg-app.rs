## Status

Proposed

## Context

Background work is scheduled by `LibraryController`, a `gpui::Entity` in
`dtrpg-ui`, using three separate mechanisms:

| Work | Queue | Limit | Order |
|---|---|---|---|
| Cover thumbnails | `thumbnail_queue` (`VecDeque`) | Shared with downloads: `max_concurrent_downloads` (default 3) | FIFO |
| File downloads | `download_queue` (`VecDeque`) | Shared with thumbnails | FIFO |
| Availability checks | `check_queue` (`VecDeque`) | One at a time, outside the shared limit | FIFO |
| Disk verification of one item | None; spawned directly | Unbounded | n/a |

The shared limit is tracked by hand: `active_thumbnail_fetches` and
`active_downloads` are incremented when a job is dispatched, decremented in
each job's completion handler, and `available_slots()` subtracts both from
the limit. Cancellation is a per-download `AtomicBool` polled by the
transfer, stored in `download_cancel_flags`. Progress and cancellation are
wired straight into the activity panel entity.

Four things follow from where this lives:

- **Nothing headless can schedule work.** The queue state is inside a GPUI
  entity, so the MCP server (`dtrpg-mcp`) cannot download or fetch anything
  without linking GPUI. This is the same forcing function as ADR 0002.
- **A user-initiated download waits behind a backlog.** Order is strictly
  FIFO, so a download the user just asked for queues behind every
  thumbnail already waiting. `libri-work-queue` requires the opposite.
- **The bookkeeping is duplicated.** Each queue has its own drain function
  and its own slot accounting; the checks queue has a third variant of it.
- **The work itself is blocking, not async.** Thumbnails use
  `reqwest::blocking`, downloads use `reqwest::blocking` inside the SDK
  service, and availability checks call a synchronous `LibraryService` that
  runs the SDK with `Runtime::block_on`. All of it runs on GPUI's
  background executor threads today. With the shared runtime in
  `dtrpg_core::runtime`, the same blocking calls can run on that runtime's
  blocking pool instead, with no change to the calls.

Requirements are in `libri-work-queue` (OpenSpec change
`libri-v2-architecture`).

## Decision

Add a work queue to `dtrpg-core` (`dtrpg_core::work`) that owns scheduling,
the concurrency limit, priority, cancellation, and progress for background
jobs. The UI and the MCP server submit jobs to it and observe it through
events; neither owns queue state.

### Shape

```rust
pub struct WorkQueue { /* shared state behind an Arc<Mutex<_>> */ }

pub enum JobKind { Thumbnail, Download, Check }

pub enum Priority { Interactive, Background }   // Interactive runs first

pub struct JobSpec {
    pub kind: JobKind,
    pub priority: Priority,
    pub label: String,                           // for the activity panel
}

impl WorkQueue {
    pub fn new(runtime: Handle, limit: usize) -> (Self, Receiver<JobEvent>);
    pub fn submit<F>(&self, spec: JobSpec, job: F) -> JobId
        where F: FnOnce(&JobContext) -> JobResult + Send + 'static;
    pub fn cancel(&self, id: JobId);
    pub fn set_limit(&self, limit: usize);
}

pub struct JobContext { /* cancellation flag + progress sender */ }
impl JobContext {
    pub fn is_cancelled(&self) -> bool;
    pub fn report_progress(&self, done: u64, total: Option<u64>);
}

pub enum JobEvent {
    Started   { id: JobId },
    Progress  { id: JobId, done: u64, total: Option<u64> },
    Finished  { id: JobId, outcome: Outcome },   // Succeeded | Failed(String) | Cancelled
}
```

This is a sketch of the contract, not final signatures.

### Rules

1. **Jobs are blocking closures.** They match the code that exists, and run
   on the shared runtime through `spawn_blocking`. Making jobs async is a
   later, separate change; the queue's public contract does not need to
   change for it.
2. **The queue reports lifecycle, and jobs carry their own results.** The
   queue does not know what a thumbnail or an item is. A job's closure
   captures whatever it needs to deliver its result (a channel, a callback,
   a write to the index). The event stream tells a consumer that job `id`
   started, progressed, or finished, and the consumer keeps its own map from
   `JobId` to meaning. This keeps the queue free of generics over result
   types and free of any UI type.
3. **One shared limit, plus an optional cap per kind.** All kinds count
   against `limit`, which carries the existing
   `max_concurrent_downloads` setting. `Check` additionally has a cap of 1,
   which preserves today's one-at-a-time behavior for availability checks
   (a batch of up to 50 would otherwise hit the API in parallel). One set of
   accounting still serves every kind, as the requirement states.
4. **Priority orders the pending list; it does not preempt.** `Interactive`
   work is dispatched before `Background` work; order is FIFO within a
   priority. A running job is never interrupted to make room. User-initiated
   downloads and manual "Load/Refresh Thumbnail" are `Interactive`;
   automatic thumbnail loading and periodic checks are `Background`.
5. **Cancellation is cooperative.** Cancelling a pending job removes it and
   emits `Finished(Cancelled)` without it ever starting. Cancelling a running
   job sets the flag the job polls through `is_cancelled`; the slot is
   released when the closure returns, as with `download_cancel_flags` today.
6. **A slot is never leaked.** The slot is released when the closure
   returns, panics, or its blocking task fails to join. A panic is reported
   as `Failed`.
7. **The state lock is never held while a job runs.** The lock guards the
   pending lists and counters only. A poisoned lock is recovered, as
   `SdkConnection` already does.
8. **Events are ordered per job.** `Started` precedes `Progress`, which
   precedes `Finished`. Events from different jobs may interleave.
9. **Jobs are not durable.** The queue is in memory. Restart recovery of
   downloads belongs to a later change (the v2 review's "durable job
   model").

### How consumers use it

- **`dtrpg-ui`** keeps one GPUI task that receives `JobEvent`s and updates
  the controller and the activity panel on the foreground. The controller
  drops its three `VecDeque`s, its slot counters, and `available_slots`; it
  keeps view state (which items are checking or downloading, activity ids)
  keyed by `JobId`. The Storage settings page calls `set_limit`.
- **`dtrpg-mcp`** submits the same jobs through `LibraryApi` and reads the
  same events, with no window and no GPUI.
- **`LibraryApi`** is the only public path to the queue, consistent with
  ADR 0002.

### Migration, in slices

Each slice keeps behavior constant and lands separately on the stage 3
branch, with the full suite run at each step:

1. The `work` module and its tests, with no consumers.
2. Thumbnails. This also moves the direct `reqwest` calls in
   `thumbnails.rs` and `data/avatar.rs` behind a core service trait, so the
   job body can live in `dtrpg-core` and has a test seam.
3. Downloads, including cancel and the activity panel progress.
4. Availability checks.
5. Delete the old queues and counters from `LibraryController`.

## Alternatives considered

- **A `tokio::sync::Semaphore` around spawned tasks.** Rejected. A
  semaphore grants permits in arrival order, so it cannot run interactive
  work first, cannot remove a job that has not started, and cannot be
  resized without replacing it. It would also need a second structure to
  list what is waiting, which is the queue we are trying to write once.
- **Leave the queues in the UI and share a trait with the MCP server.**
  Rejected for the reason ADR 0002 rejected duplicating the catalog: two
  implementations of the same scheduling with nothing forcing them to agree,
  and the MCP server still could not run without GPUI.
- **Make jobs `async` now.** Deferred. It means replacing
  `reqwest::blocking` in three places and the SDK service's `block_on`
  wrappers in the same change that is already moving the queues. The shared
  runtime makes it possible later without changing this contract.
- **One generic queue with typed results (`submit<T>`).** Rejected for now.
  It adds a type parameter to the queue, the handle, and the events, to
  save consumers a map from `JobId` to meaning that they need anyway for
  their view state.

## Consequences

- Scheduling logic exists once, and the MCP server can schedule downloads
  and thumbnail work without GPUI.
- User-initiated downloads no longer wait behind thumbnail backlogs. This is
  a visible behavior change in its favor, and it should be called out in the
  changelog; it is the one deliberate deviation from "behavior held
  constant".
- The controller stops owning queue state, which removes the three queues,
  the slot counters, and the drain functions, and is a step toward the
  `LibraryController` slimming `design.md` leaves as optional follow-up. It
  does not move catalog ownership to the core; the render path is untouched.
- Completion now crosses a thread boundary through an event channel instead
  of a `this.update` closure on the entity. Consumers must tolerate the
  entity having been dropped by the time an event arrives, which the current
  `.ok()` calls already do.
- Blocking jobs run on Tokio's blocking pool, which is sized far above any
  realistic `limit`. The queue's own limit is what bounds them, so the limit
  must be enforced in the queue and never assumed from the pool.
- The `Check` cap is a special case in the scheduler. It is the smallest way
  to keep single-flight checks inside one accounting scheme; if more kinds
  need caps, a per-kind limit table replaces it.

## Resolved (2026-10-18)

1. **Disk verification stays inline and outside the queue.**
   `verify_selected_item_download` is a handful of `Path::exists` calls,
   triggered when the user selects an item. Putting it behind a download
   slot would delay a result the user is looking at. `libri-work-queue` is
   amended so that "verification" means availability checks.
2. **Manually triggered availability checks are `Interactive`.**
   `request_check_batch` serves both the automatic timer and a manual
   trigger. The timer submits its batch as `Background`; a manual trigger
   submits its batch as `Interactive`. The `Check` cap of 1 applies to both,
   so a manual batch jumps ahead of pending background work but still runs
   one check at a time.
