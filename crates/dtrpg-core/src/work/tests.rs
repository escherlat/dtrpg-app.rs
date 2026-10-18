//! Behavior tests for the work queue, run against the real shared runtime.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

fn new_queue(limit: usize) -> (WorkQueue, JobEvents) {
    WorkQueue::new(limit)
}

fn spec(kind: JobKind, priority: Priority) -> JobSpec {
    JobSpec { kind,
              priority,
              label: format!("{kind:?}") }
}

fn background(kind: JobKind) -> JobSpec {
    spec(kind, Priority::Background)
}

/// Waits for the next event, failing the test if none arrives in time.
fn next_event(events: &mut JobEvents) -> JobEvent {
    let deadline = Instant::now() + WAIT;
    loop {
        match events.try_recv() {
            Ok(event) => return event,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
            Err(error) => panic!("no event within {WAIT:?}: {error}"),
        }
    }
}

/// Reads events until `id` finishes, returning its outcome.
fn outcome_of(events: &mut JobEvents, id: JobId) -> Outcome {
    loop {
        if let JobEvent::Finished { id: finished, outcome } = next_event(events)
           && finished == id
        {
            return outcome;
        }
    }
}

/// Reads events until `count` jobs have finished, whatever order they end in.
fn finished_outcomes(events: &mut JobEvents, count: usize)
                     -> Vec<(JobId, Outcome)> {
    let mut finished = Vec::new();
    while finished.len() < count {
        if let JobEvent::Finished { id, outcome } = next_event(events) {
            finished.push((id, outcome));
        }
    }
    finished
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "condition not met within {WAIT:?}");
        thread::sleep(Duration::from_millis(2));
    }
}

/// Holds jobs until the test opens it.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }

    fn wait(&self) {
        let guard = self.open.lock().unwrap();
        let (_guard, result) = self.changed
                                   .wait_timeout_while(guard, WAIT, |open| !*open)
                                   .unwrap();
        assert!(!result.timed_out(), "gate never opened");
    }
}

/// Submits a job that counts itself as entered, then waits at the gate.
fn submit_gated(queue: &WorkQueue, spec: JobSpec, gate: &Arc<Gate>, entered: &Arc<AtomicUsize>)
                -> JobId {
    let gate = Arc::clone(gate);
    let entered = Arc::clone(entered);
    queue.submit(spec, move |_context| {
             entered.fetch_add(1, Ordering::SeqCst);
             gate.wait();
             Ok(())
         })
}

#[test]
fn a_job_reports_started_then_finished() {
    let (queue, mut events) = new_queue(3);
    let id = queue.submit(background(JobKind::Download), |_context| Ok(()));

    assert_eq!(next_event(&mut events), JobEvent::Started { id });
    assert_eq!(next_event(&mut events),
               JobEvent::Finished { id,
                                    outcome: Outcome::Succeeded });
}

#[test]
fn a_failing_job_reports_its_message() {
    let (queue, mut events) = new_queue(3);
    let id = queue.submit(background(JobKind::Download), |_context| Err("disk full".to_owned()));

    assert_eq!(outcome_of(&mut events, id), Outcome::Failed("disk full".to_owned()));
}

#[test]
fn a_panicking_job_fails_and_frees_its_slot() {
    let (queue, mut events) = new_queue(1);
    let panicking = queue.submit(background(JobKind::Download), |_context| panic!("boom"));
    let after = queue.submit(background(JobKind::Download), |_context| Ok(()));

    assert!(matches!(outcome_of(&mut events, panicking), Outcome::Failed(_)));
    assert_eq!(outcome_of(&mut events, after), Outcome::Succeeded);
}

#[test]
fn progress_arrives_between_started_and_finished() {
    let (queue, mut events) = new_queue(3);
    let id = queue.submit(background(JobKind::Download), |context| {
                      context.report_progress(10, Some(100));
                      context.report_progress(100, Some(100));
                      Ok(())
                  });

    assert_eq!(next_event(&mut events), JobEvent::Started { id });
    assert_eq!(next_event(&mut events),
               JobEvent::Progress { id,
                                    done: 10,
                                    total: Some(100) });
    assert_eq!(next_event(&mut events),
               JobEvent::Progress { id,
                                    done: 100,
                                    total: Some(100) });
    assert_eq!(next_event(&mut events),
               JobEvent::Finished { id,
                                    outcome: Outcome::Succeeded });
}

#[test]
fn the_limit_bounds_how_many_jobs_run_at_once() {
    let (queue, mut events) = new_queue(2);
    let gate = Arc::new(Gate::default());
    let current = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    for _ in 0..4 {
        let (gate, current, peak) = (Arc::clone(&gate), Arc::clone(&current), Arc::clone(&peak));
        queue.submit(background(JobKind::Download), move |_context| {
                 let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                 peak.fetch_max(now, Ordering::SeqCst);
                 gate.wait();
                 current.fetch_sub(1, Ordering::SeqCst);
                 Ok(())
             });
    }

    wait_until(|| current.load(Ordering::SeqCst) == 2);
    gate.open();
    let finished = finished_outcomes(&mut events, 4);
    assert!(finished.iter().all(|(_, outcome)| *outcome == Outcome::Succeeded));
    assert_eq!(peak.load(Ordering::SeqCst), 2);
}

#[test]
fn interactive_jobs_start_before_background_jobs() {
    let (queue, mut events) = new_queue(1);
    let gate = Arc::new(Gate::default());
    let entered = Arc::new(AtomicUsize::new(0));
    let blocker = submit_gated(&queue, background(JobKind::Download), &gate, &entered);
    wait_until(|| entered.load(Ordering::SeqCst) == 1);

    let background_job = queue.submit(background(JobKind::Thumbnail), |_context| Ok(()));
    let interactive_job = queue.submit(spec(JobKind::Download, Priority::Interactive),
                                       |_context| Ok(()));
    gate.open();

    let mut started = Vec::new();
    while started.len() < 3 {
        if let JobEvent::Started { id } = next_event(&mut events) {
            started.push(id);
        }
    }
    assert_eq!(started, [blocker, interactive_job, background_job]);
}

#[test]
fn jobs_of_one_priority_start_in_submission_order() {
    let (queue, mut events) = new_queue(1);
    let gate = Arc::new(Gate::default());
    let entered = Arc::new(AtomicUsize::new(0));
    let blocker = submit_gated(&queue, background(JobKind::Download), &gate, &entered);
    wait_until(|| entered.load(Ordering::SeqCst) == 1);

    let second = queue.submit(background(JobKind::Download), |_context| Ok(()));
    let third = queue.submit(background(JobKind::Thumbnail), |_context| Ok(()));
    gate.open();

    let mut started = Vec::new();
    while started.len() < 3 {
        if let JobEvent::Started { id } = next_event(&mut events) {
            started.push(id);
        }
    }
    assert_eq!(started, [blocker, second, third]);
}

#[test]
fn cancelling_a_pending_job_means_it_never_starts() {
    let (queue, mut events) = new_queue(1);
    let gate = Arc::new(Gate::default());
    let entered = Arc::new(AtomicUsize::new(0));
    submit_gated(&queue, background(JobKind::Download), &gate, &entered);
    wait_until(|| entered.load(Ordering::SeqCst) == 1);

    let ran = Arc::new(AtomicBool::new(false));
    let ran_in_job = Arc::clone(&ran);
    let pending = queue.submit(background(JobKind::Download), move |_context| {
                           ran_in_job.store(true, Ordering::SeqCst);
                           Ok(())
                       });
    queue.cancel(pending);
    gate.open();

    assert_eq!(outcome_of(&mut events, pending), Outcome::Cancelled);
    // Let the queue go idle before checking, so a late start would be seen.
    let idle = queue.submit(background(JobKind::Download), |_context| Ok(()));
    outcome_of(&mut events, idle);
    assert!(!ran.load(Ordering::SeqCst));
}

#[test]
fn cancelling_a_running_job_sets_its_flag_and_frees_its_slot() {
    let (queue, mut events) = new_queue(1);
    let running = Arc::new(AtomicBool::new(false));
    let running_in_job = Arc::clone(&running);
    let id = queue.submit(background(JobKind::Download), move |context| {
                      running_in_job.store(true, Ordering::SeqCst);
                      let deadline = Instant::now() + WAIT;
                      while !context.is_cancelled() {
                          assert!(Instant::now() < deadline, "job was never cancelled");
                          thread::sleep(Duration::from_millis(2));
                      }
                      Ok(())
                  });
    let after = queue.submit(background(JobKind::Download), |_context| Ok(()));

    wait_until(|| running.load(Ordering::SeqCst));
    queue.cancel(id);

    assert_eq!(outcome_of(&mut events, id), Outcome::Cancelled);
    assert_eq!(outcome_of(&mut events, after), Outcome::Succeeded);
}

#[test]
fn cancelling_an_unknown_or_finished_job_does_nothing() {
    let (queue, mut events) = new_queue(1);
    let id = queue.submit(background(JobKind::Download), |_context| Ok(()));
    assert_eq!(outcome_of(&mut events, id), Outcome::Succeeded);

    queue.cancel(id);

    let next = queue.submit(background(JobKind::Download), |_context| Ok(()));
    assert_eq!(next_event(&mut events), JobEvent::Started { id: next });
}

#[test]
fn checks_run_one_at_a_time_without_blocking_other_kinds() {
    let (queue, mut events) = new_queue(3);
    let gate = Arc::new(Gate::default());
    let entered_checks = Arc::new(AtomicUsize::new(0));
    let first = submit_gated(&queue, background(JobKind::Check), &gate, &entered_checks);
    let second = submit_gated(&queue, background(JobKind::Check), &gate, &entered_checks);
    let download = queue.submit(background(JobKind::Download), |_context| Ok(()));

    // The download runs even though a second check is waiting ahead of it.
    assert_eq!(outcome_of(&mut events, download), Outcome::Succeeded);
    assert_eq!(entered_checks.load(Ordering::SeqCst), 1);

    gate.open();
    assert_eq!(outcome_of(&mut events, first), Outcome::Succeeded);
    assert_eq!(outcome_of(&mut events, second), Outcome::Succeeded);
    assert_eq!(entered_checks.load(Ordering::SeqCst), 2);
}

#[test]
fn raising_the_limit_starts_waiting_jobs() {
    let (queue, _events) = new_queue(1);
    let gate = Arc::new(Gate::default());
    let entered = Arc::new(AtomicUsize::new(0));
    submit_gated(&queue, background(JobKind::Download), &gate, &entered);
    submit_gated(&queue, background(JobKind::Download), &gate, &entered);
    wait_until(|| entered.load(Ordering::SeqCst) == 1);

    queue.set_limit(2);

    wait_until(|| entered.load(Ordering::SeqCst) == 2);
    gate.open();
}

#[test]
fn a_limit_of_zero_is_treated_as_one() {
    let (queue, mut events) = new_queue(0);
    let id = queue.submit(background(JobKind::Download), |_context| Ok(()));
    assert_eq!(outcome_of(&mut events, id), Outcome::Succeeded);

    queue.set_limit(0);

    let next = queue.submit(background(JobKind::Download), |_context| Ok(()));
    assert_eq!(outcome_of(&mut events, next), Outcome::Succeeded);
}

#[test]
fn jobs_still_run_when_nobody_reads_events() {
    let (queue, events) = new_queue(1);
    drop(events);
    let ran = Arc::new(AtomicBool::new(false));
    let ran_in_job = Arc::clone(&ran);

    queue.submit(background(JobKind::Download), move |context| {
             context.report_progress(1, None);
             ran_in_job.store(true, Ordering::SeqCst);
             Ok(())
         });

    wait_until(|| ran.load(Ordering::SeqCst));
}

#[test]
fn a_job_can_hand_back_a_value_through_a_slot() {
    let (queue, mut events) = new_queue(1);
    let slot = ResultSlot::new();
    let slot_in_job = slot.clone();

    let id = queue.submit(background(JobKind::Thumbnail), move |_context| {
                      slot_in_job.put(vec![1_u8, 2, 3]);
                      Ok(())
                  });

    assert_eq!(outcome_of(&mut events, id), Outcome::Succeeded);
    assert_eq!(slot.take(), Some(vec![1, 2, 3]));
    assert_eq!(slot.take(), None);
}

#[test]
fn a_job_can_pass_its_cancel_flag_to_other_code() {
    let (queue, mut events) = new_queue(1);
    let seen = Arc::new(AtomicBool::new(true));
    let seen_in_job = Arc::clone(&seen);

    let id = queue.submit(background(JobKind::Download), move |context| {
                      seen_in_job.store(context.cancel_flag().load(Ordering::SeqCst), Ordering::SeqCst);
                      Ok(())
                  });

    assert_eq!(outcome_of(&mut events, id), Outcome::Succeeded);
    assert!(!seen.load(Ordering::SeqCst));
}
