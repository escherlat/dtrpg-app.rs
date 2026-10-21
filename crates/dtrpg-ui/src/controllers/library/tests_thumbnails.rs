//! End-to-end tests for thumbnail fetching: a real controller, the real work
//! queue and event loop, and a scripted transport standing in for the network.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Mutex;

use gpui::TestAppContext;

use super::{tests_support::*, *};

fn requested(fixture: &Fixture, cx: &mut TestAppContext, id: &str) {
    let url = url_for(id);
    fixture.controller
           .update(cx, |ctrl, cx| ctrl.load_thumbnail(url, cx));
}

fn all_attempted(fixture: &Fixture, cx: &mut TestAppContext, ids: &[&str]) -> bool {
    fixture.controller.read_with(cx, |ctrl, _| {
                          ids.iter().all(|id| {
                                        ctrl.item_by_id(id)
                                            .is_some_and(|item| {
                                                item.thumbnail_last_attempted.is_some()
                                            })
                                    })
                      })
}

#[gpui::test]
fn a_fetched_cover_is_cached_and_its_activity_completes(cx: &mut TestAppContext) {
    let fixture = fixture(cx, ScriptedTransport::open(), 3, &["a"]);

    requested(&fixture, cx, "a");
    assert!(in_flight(cx, "a"),
            "the cover is marked in flight as soon as it is requested");
    wait_for(cx, |cx| cached(cx, "a"));
    wait_for(cx, |cx| {
        fixture.activity
               .read_with(cx, |a, _| a.snapshot().recent_count == 1)
    });

    assert!(!in_flight(cx, "a"));
    assert!(all_attempted(&fixture, cx, &["a"]));
    let snapshot = fixture.activity.read_with(cx, |a, _| a.snapshot());
    assert_eq!(snapshot.in_progress_count, 0);
    fixture.controller.read_with(cx, |ctrl, _| {
                          assert!(ctrl.library.thumbnail_jobs_is_empty())
                      });
}

#[gpui::test]
fn a_failed_fetch_clears_the_marker_and_caches_nothing(cx: &mut TestAppContext) {
    let transport = ScriptedTransport::with(usize::MAX / 2, &["https://example.com/a.png"]);
    let fixture = fixture(cx, transport, 3, &["a"]);

    requested(&fixture, cx, "a");
    wait_for(cx, |cx| {
        !in_flight(cx, "a") && all_attempted(&fixture, cx, &["a"])
    });
    wait_for(cx, |cx| {
        fixture.activity
               .read_with(cx, |a, _| a.snapshot().in_progress_count == 0)
    });

    assert!(!cached(cx, "a"));
    assert!(fixture.transport.requests() > 1,
            "a failed fetch is retried before giving up");
}

#[gpui::test]
fn refresh_all_reports_started_and_completed_totals(cx: &mut TestAppContext) {
    let transport = ScriptedTransport::with(usize::MAX / 2, &["https://example.com/b.png"]);
    let fixture = fixture(cx, transport, 3, &["a", "b", "c"]);
    let started = Arc::new(Mutex::new(None));
    let completed = Arc::new(Mutex::new(None));
    let (started_in, completed_in) = (Arc::clone(&started), Arc::clone(&completed));
    cx.update(|cx| {
          cx.subscribe(&fixture.controller,
                       move |_ctrl, event: &ThumbnailRefreshStarted, _cx| {
                           *started_in.lock().unwrap() = Some(event.count);
                       })
            .detach();
          cx.subscribe(&fixture.controller,
                       move |_ctrl, event: &ThumbnailRefreshCompleted, _cx| {
                           *completed_in.lock().unwrap() = Some((event.succeeded, event.failed));
                       })
            .detach();
      });

    fixture.controller
           .update(cx, |ctrl, cx| ctrl.refresh_all_thumbnails(cx));
    wait_for(cx, |_| completed.lock().unwrap().is_some());

    assert_eq!(*started.lock().unwrap(), Some(3));
    assert_eq!(*completed.lock().unwrap(), Some((2, 1)));
    assert!(cached(cx, "a") && cached(cx, "c") && !cached(cx, "b"));
}

#[gpui::test]
fn cancelling_waiting_fetches_clears_their_markers_and_leaves_the_running_one(cx: &mut TestAppContext)
{
    let fixture = fixture(cx, ScriptedTransport::gated(), 1, &["a", "b", "c"]);
    requested(&fixture, cx, "a");
    wait_for(cx, |_| fixture.transport.requests() == 1);
    requested(&fixture, cx, "b");
    requested(&fixture, cx, "c");
    fixture.controller.read_with(cx, |ctrl, _| {
                          assert_eq!(ctrl.library.thumbnail_jobs_pending_count(), 2)
                      });

    fixture.controller
           .update(cx, |ctrl, cx| ctrl.cancel_pending_thumbnails(cx));

    assert!(!in_flight(cx, "b") && !in_flight(cx, "c"));
    assert!(in_flight(cx, "a"), "the running fetch is left alone");
    fixture.controller.read_with(cx, |ctrl, _| {
                          assert_eq!(ctrl.library.thumbnail_jobs_pending_count(), 0)
                      });
    fixture.transport.release(1);
    wait_for(cx, |cx| cached(cx, "a"));
    wait_for(cx, |cx| {
        fixture.activity
               .read_with(cx, |a, _| a.snapshot().recent_count == 1)
    });
    assert!(!cached(cx, "b") && !cached(cx, "c"));
    assert_eq!(fixture.transport.requests(),
               1,
               "cancelled fetches never reached the network");
    fixture.controller.read_with(cx, |ctrl, _| {
                          assert!(ctrl.library.thumbnail_jobs_is_empty())
                      });
}

#[gpui::test]
fn progress_counts_finished_fetches_against_those_still_waiting(cx: &mut TestAppContext) {
    let fixture = fixture(cx, ScriptedTransport::gated(), 1, &["a", "b", "c"]);
    for id in ["a", "b", "c"] {
        requested(&fixture, cx, id);
    }
    wait_for(cx, |_| fixture.transport.requests() == 1);

    fixture.transport.release(1);
    wait_for(cx, |cx| cached(cx, "a"));
    wait_for(cx, |_| fixture.transport.requests() == 2);

    // One finished, two unfinished (one running, one waiting): 1 / (1 + 2).
    let snapshot = fixture.activity.read_with(cx, |a, _| a.snapshot());
    assert_eq!(snapshot.in_progress_count, 1);
    let progress = snapshot.items[0].progress.expect("progress is reported");
    assert!((progress - 1.0 / 3.0).abs() < 0.001,
            "unexpected progress {progress}");

    fixture.transport.release(2);
    wait_for(cx, |cx| cached(cx, "b") && cached(cx, "c"));
    wait_for(cx, |cx| {
        fixture.activity
               .read_with(cx, |a, _| a.snapshot().recent_count == 1)
    });
}

#[gpui::test]
fn requesting_a_cover_again_replaces_its_waiting_fetch(cx: &mut TestAppContext) {
    let fixture = fixture(cx, ScriptedTransport::gated(), 1, &["a", "b"]);
    requested(&fixture, cx, "a");
    wait_for(cx, |_| fixture.transport.requests() == 1);

    requested(&fixture, cx, "b");
    requested(&fixture, cx, "b");

    fixture.controller.read_with(cx, |ctrl, _| {
                          assert_eq!(ctrl.library.thumbnail_jobs_pending_count(), 1)
                      });
    fixture.transport.release(2);
    wait_for(cx, |cx| cached(cx, "a") && cached(cx, "b"));
    wait_for(cx, |cx| {
        fixture.activity
               .read_with(cx, |a, _| a.snapshot().recent_count == 1)
    });
    assert_eq!(fixture.transport.requests(),
               2,
               "b was fetched once, not twice");
}
