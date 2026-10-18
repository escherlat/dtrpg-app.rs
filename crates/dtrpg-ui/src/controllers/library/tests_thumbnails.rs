//! End-to-end tests for thumbnail fetching: a real controller, the real work
//! queue and event loop, and a scripted transport standing in for the network.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{
    collections::HashSet,
    sync::{
        Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use dtrpg_core::{
    data::collection::CollectionEntry,
    services::{
        collections::CollectionsServiceError,
        covers::{CoverFetchError, CoverTransport},
        stub::{StubLibraryService, StubMode},
    },
};
use gpui::TestAppContext;

use super::*;

const WAIT: Duration = Duration::from_secs(5);

/// Bytes that sniff as a PNG.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-cover";

/// A transport that hands out one permit per request, so a test decides when
/// each fetch may finish, and fails the URLs it is told to.
struct ScriptedTransport {
    permits: Mutex<usize>,
    changed: Condvar,
    failing: HashSet<String>,
    requests: AtomicUsize,
}

impl ScriptedTransport {
    /// A transport that answers every request immediately.
    fn open() -> Arc<Self> {
        Self::with(usize::MAX / 2, &[])
    }

    /// A transport that holds each request until a permit is released.
    fn gated() -> Arc<Self> {
        Self::with(0, &[])
    }

    fn with(permits: usize, failing: &[&str]) -> Arc<Self> {
        Arc::new(Self { permits: Mutex::new(permits),
                        changed: Condvar::new(),
                        failing: failing.iter().map(|url| (*url).to_owned()).collect(),
                        requests: AtomicUsize::new(0) })
    }

    fn release(&self, count: usize) {
        *self.permits.lock().unwrap() += count;
        self.changed.notify_all();
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl CoverTransport for ScriptedTransport {
    fn is_reachable(&self) -> bool {
        true
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, CoverFetchError> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let permits = self.permits.lock().unwrap();
        let (mut permits, result) =
            self.changed.wait_timeout_while(permits, WAIT, |available| *available == 0).unwrap();
        assert!(!result.timed_out(), "no permit was released for {url}");
        *permits -= 1;
        if self.failing.contains(url) {
            // No wait between attempts, so a failing fetch is quick.
            return Err(CoverFetchError { message:     "boom".to_owned(),
                                         retry_after: Some(Duration::ZERO), });
        }
        Ok(PNG.to_vec())
    }
}

/// Never called: these tests do not touch collections.
struct UnusedCollections;

impl CollectionsService for UnusedCollections {
    fn list_collections(&self) -> Result<Vec<CollectionEntry>, CollectionsServiceError> {
        Err(unused())
    }

    fn create_collection(&self, _name: &str) -> Result<CollectionEntry, CollectionsServiceError> {
        Err(unused())
    }

    fn delete_collection(&self, _id: u64) -> Result<(), CollectionsServiceError> {
        Err(unused())
    }

    fn add_member(&self, _collection_id: u64, _item_id: u64) -> Result<(), CollectionsServiceError> {
        Err(unused())
    }

    fn remove_member(&self, _collection_id: u64, _item_id: u64)
                     -> Result<(), CollectionsServiceError> {
        Err(unused())
    }
}

fn unused() -> CollectionsServiceError {
    CollectionsServiceError::new(CollectionsServiceErrorKind::Network, "unused in this test")
}

fn url_for(id: &str) -> Arc<str> {
    Arc::from(format!("https://example.com/{id}.png"))
}

fn item_with_cover(id: &str) -> LibraryItem {
    let mut item = LibraryItem::new(id, id, "Publisher", "Line", "Core", "PDF", 0, 0.0, 2020, 0,
                                    ItemStatus::Cloud, "#000000", "", None);
    item.cover_url = Some(url_for(id));
    item
}

struct Fixture {
    controller: Entity<LibraryController>,
    activity: Entity<ActivityController>,
    transport: Arc<ScriptedTransport>,
    /// Keeps the cover cache directory alive for the length of the test.
    _covers: tempfile::TempDir,
}

/// Builds a controller holding `ids` as catalog items with covers, fetching
/// through `transport` and running at most `limit` fetches at once.
fn fixture(cx: &mut TestAppContext, transport: Arc<ScriptedTransport>, limit: usize, ids: &[&str])
           -> Fixture {
    cx.update(|cx| cx.set_global(CoverCache::new()));
    let covers = tempfile::tempdir().unwrap();
    let activity = cx.new(|_| ActivityController::new());
    let controller = cx.new(|cx| {
                           let (mut ctrl, events) =
                               LibraryController::build(Box::new(StubLibraryService::new(StubMode::Empty)),
                                                        Box::new(UnusedCollections),
                                                        activity.clone());
                           ctrl.cover_fetcher =
                               Arc::new(CoverFetcher::new(Arc::clone(&transport) as Arc<dyn CoverTransport>,
                                                          covers.path().to_path_buf()));
                           ctrl.work.set_limit(limit);
                           ctrl.catalog = ids.iter().map(|id| item_with_cover(id)).collect();
                           ctrl.start_work_event_pump(events, cx);
                           ctrl
                       });
    Fixture { controller, activity, transport, _covers: covers }
}

/// Lets the event loop run until `done` holds. Fetches run on other threads,
/// so this polls rather than waiting on the test scheduler alone.
fn wait_for(cx: &mut TestAppContext, mut done: impl FnMut(&mut TestAppContext) -> bool) {
    cx.executor().allow_parking();
    let deadline = Instant::now() + WAIT;
    loop {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(Instant::now() < deadline, "condition not met within {WAIT:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn cached(cx: &mut TestAppContext, id: &str) -> bool {
    cx.update(|cx| cx.global::<CoverCache>().get(id).is_some())
}

fn in_flight(cx: &mut TestAppContext, id: &str) -> bool {
    cx.update(|cx| cx.global::<CoverCache>().is_in_flight(id))
}

fn requested(fixture: &Fixture, cx: &mut TestAppContext, id: &str) {
    let url = url_for(id);
    fixture.controller.update(cx, |ctrl, cx| ctrl.load_thumbnail(url, cx));
}

fn all_attempted(fixture: &Fixture, cx: &mut TestAppContext, ids: &[&str]) -> bool {
    fixture.controller.read_with(cx, |ctrl, _| {
                          ids.iter().all(|id| {
                                        ctrl.catalog
                                            .iter()
                                            .any(|item| &*item.id == *id
                                                        && item.thumbnail_last_attempted.is_some())
                                    })
                      })
}

#[gpui::test]
fn a_fetched_cover_is_cached_and_its_activity_completes(cx: &mut TestAppContext) {
    let fixture = fixture(cx, ScriptedTransport::open(), 3, &["a"]);

    requested(&fixture, cx, "a");
    assert!(in_flight(cx, "a"), "the cover is marked in flight as soon as it is requested");
    wait_for(cx, |cx| cached(cx, "a"));
    wait_for(cx, |cx| fixture.activity.read_with(cx, |a, _| a.snapshot().recent_count == 1));

    assert!(!in_flight(cx, "a"));
    assert!(all_attempted(&fixture, cx, &["a"]));
    let snapshot = fixture.activity.read_with(cx, |a, _| a.snapshot());
    assert_eq!(snapshot.in_progress_count, 0);
    fixture.controller.read_with(cx, |ctrl, _| assert!(ctrl.thumbnail_jobs.is_empty()));
}

#[gpui::test]
fn a_failed_fetch_clears_the_marker_and_caches_nothing(cx: &mut TestAppContext) {
    let transport = ScriptedTransport::with(usize::MAX / 2, &["https://example.com/a.png"]);
    let fixture = fixture(cx, transport, 3, &["a"]);

    requested(&fixture, cx, "a");
    wait_for(cx, |cx| !in_flight(cx, "a") && all_attempted(&fixture, cx, &["a"]));
    wait_for(cx, |cx| fixture.activity.read_with(cx, |a, _| a.snapshot().in_progress_count == 0));

    assert!(!cached(cx, "a"));
    assert!(fixture.transport.requests() > 1, "a failed fetch is retried before giving up");
}

#[gpui::test]
fn refresh_all_reports_started_and_completed_totals(cx: &mut TestAppContext) {
    let transport = ScriptedTransport::with(usize::MAX / 2, &["https://example.com/b.png"]);
    let fixture = fixture(cx, transport, 3, &["a", "b", "c"]);
    let started = Arc::new(Mutex::new(None));
    let completed = Arc::new(Mutex::new(None));
    let (started_in, completed_in) = (Arc::clone(&started), Arc::clone(&completed));
    cx.update(|cx| {
          cx.subscribe(&fixture.controller, move |_ctrl, event: &ThumbnailRefreshStarted, _cx| {
              *started_in.lock().unwrap() = Some(event.count);
          })
          .detach();
          cx.subscribe(&fixture.controller, move |_ctrl, event: &ThumbnailRefreshCompleted, _cx| {
              *completed_in.lock().unwrap() = Some((event.succeeded, event.failed));
          })
          .detach();
      });

    fixture.controller.update(cx, |ctrl, cx| ctrl.refresh_all_thumbnails(cx));
    wait_for(cx, |_| completed.lock().unwrap().is_some());

    assert_eq!(*started.lock().unwrap(), Some(3));
    assert_eq!(*completed.lock().unwrap(), Some((2, 1)));
    assert!(cached(cx, "a") && cached(cx, "c") && !cached(cx, "b"));
}

#[gpui::test]
fn cancelling_waiting_fetches_clears_their_markers_and_leaves_the_running_one(cx: &mut TestAppContext) {
    let fixture = fixture(cx, ScriptedTransport::gated(), 1, &["a", "b", "c"]);
    requested(&fixture, cx, "a");
    wait_for(cx, |_| fixture.transport.requests() == 1);
    requested(&fixture, cx, "b");
    requested(&fixture, cx, "c");
    fixture.controller.read_with(cx, |ctrl, _| assert_eq!(ctrl.thumbnail_jobs.pending_count(), 2));

    fixture.controller.update(cx, |ctrl, cx| ctrl.cancel_pending_thumbnails(cx));

    assert!(!in_flight(cx, "b") && !in_flight(cx, "c"));
    assert!(in_flight(cx, "a"), "the running fetch is left alone");
    fixture.controller.read_with(cx, |ctrl, _| assert_eq!(ctrl.thumbnail_jobs.pending_count(), 0));
    fixture.transport.release(1);
    wait_for(cx, |cx| cached(cx, "a"));
    wait_for(cx, |cx| fixture.activity.read_with(cx, |a, _| a.snapshot().recent_count == 1));
    assert!(!cached(cx, "b") && !cached(cx, "c"));
    assert_eq!(fixture.transport.requests(), 1, "cancelled fetches never reached the network");
    fixture.controller.read_with(cx, |ctrl, _| assert!(ctrl.thumbnail_jobs.is_empty()));
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
    assert!((progress - 1.0 / 3.0).abs() < 0.001, "unexpected progress {progress}");

    fixture.transport.release(2);
    wait_for(cx, |cx| cached(cx, "b") && cached(cx, "c"));
    wait_for(cx, |cx| fixture.activity.read_with(cx, |a, _| a.snapshot().recent_count == 1));
}

#[gpui::test]
fn requesting_a_cover_again_replaces_its_waiting_fetch(cx: &mut TestAppContext) {
    let fixture = fixture(cx, ScriptedTransport::gated(), 1, &["a", "b"]);
    requested(&fixture, cx, "a");
    wait_for(cx, |_| fixture.transport.requests() == 1);

    requested(&fixture, cx, "b");
    requested(&fixture, cx, "b");

    fixture.controller.read_with(cx, |ctrl, _| assert_eq!(ctrl.thumbnail_jobs.pending_count(), 1));
    fixture.transport.release(2);
    wait_for(cx, |cx| cached(cx, "a") && cached(cx, "b"));
    wait_for(cx, |cx| fixture.activity.read_with(cx, |a, _| a.snapshot().recent_count == 1));
    assert_eq!(fixture.transport.requests(), 2, "b was fetched once, not twice");
}
