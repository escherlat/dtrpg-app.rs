//! End-to-end tests for file downloads: a real controller, the real work queue
//! and event loop, and a scripted library service standing in for the network.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use dtrpg_core::data::activity::ActivityStatus;
use gpui::TestAppContext;

use super::{tests_support::*, *};

/// Order products, one per test entry, so the service can tell downloads apart.
const A: u64 = 11;
const B: u64 = 22;

fn download(fixture: &Fixture, cx: &mut TestAppContext, id: &str) {
    fixture.controller
           .update(cx, |ctrl, cx| ctrl.enqueue_item_download(id, 0, "Title", cx));
}

fn file_downloaded(fixture: &Fixture, cx: &mut TestAppContext, id: &str) -> bool {
    fixture.controller.read_with(cx, |ctrl, _| {
                          ctrl.catalog
                              .iter()
                              .find(|item| &*item.id == id)
                              .is_some_and(|item| item.files[0].downloaded)
                      })
}

fn is_downloading(fixture: &Fixture, cx: &mut TestAppContext, id: &str) -> bool {
    fixture.controller
           .read_with(cx, |ctrl, _| ctrl.is_downloading(id))
}

fn entries(ids: &[(&str, u64)]) -> Vec<LibraryItem> {
    ids.iter()
       .map(|(id, order_product)| item_with_files(id, *order_product, 1))
       .collect()
}

#[gpui::test]
fn a_download_marks_the_file_downloaded_and_completes_its_activity(cx: &mut TestAppContext) {
    let fixture = fixture_with(cx, ScriptedTransport::open(), ScriptedLibrary::open(), 3,
                               entries(&[("a", A)]));

    download(&fixture, cx, "a");
    assert!(is_downloading(&fixture, cx, "a"), "queued downloads count as downloading");
    wait_for(cx, |cx| file_downloaded(&fixture, cx, "a"));
    wait_for(cx, |cx| activity_counts(&fixture, cx) == (0, 1));

    assert!(!is_downloading(&fixture, cx, "a"));
    let status = fixture.controller.read_with(cx, |ctrl, _| ctrl.catalog[0].status);
    assert_eq!(status, ItemStatus::Downloaded);
    let items = fixture.activity.read_with(cx, |a, _| a.snapshot().items);
    assert!(matches!(items[0].status, ActivityStatus::Complete));
    fixture.controller.read_with(cx, |ctrl, _| assert!(ctrl.download_jobs.item_ids().next().is_none()));
}

#[gpui::test]
fn a_finished_download_is_saved_to_the_catalog_cache(cx: &mut TestAppContext) {
    let fixture = fixture_with(cx, ScriptedTransport::open(), ScriptedLibrary::open(), 3,
                               entries(&[("a", A)]));

    download(&fixture, cx, "a");

    wait_for(cx, |_| std::fs::read_dir(&fixture.cache).is_ok_and(|mut dir| dir.next().is_some()));
}

#[gpui::test]
fn a_failed_download_leaves_the_file_and_fails_its_activity(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::with(usize::MAX / 2, &[A], &[]);
    let fixture = fixture_with(cx, ScriptedTransport::open(), library, 3, entries(&[("a", A)]));

    download(&fixture, cx, "a");
    wait_for(cx, |cx| activity_counts(&fixture, cx) == (0, 1));

    assert!(!file_downloaded(&fixture, cx, "a"));
    assert!(!is_downloading(&fixture, cx, "a"));
    let items = fixture.activity.read_with(cx, |a, _| a.snapshot().items);
    assert!(matches!(&items[0].status, ActivityStatus::Error(message) if message.contains("boom")));
    let cache_written = std::fs::read_dir(&fixture.cache).is_ok_and(|mut dir| dir.next().is_some());
    assert!(!cache_written, "a failed download does not rewrite the catalog cache");
}

#[gpui::test]
fn cancelling_a_waiting_download_means_it_never_starts(cx: &mut TestAppContext) {
    let fixture = fixture_with(cx, ScriptedTransport::open(), ScriptedLibrary::gated(), 1,
                               entries(&[("a", A), ("b", B)]));
    download(&fixture, cx, "a");
    wait_for(cx, |_| fixture.library.started() == [A]);
    download(&fixture, cx, "b");
    assert!(is_downloading(&fixture, cx, "b"));

    fixture.controller.update(cx, |ctrl, cx| ctrl.cancel_download("b", 0, cx));

    assert!(!is_downloading(&fixture, cx, "b"));
    fixture.library.release(1);
    wait_for(cx, |cx| file_downloaded(&fixture, cx, "a"));
    wait_for(cx, |cx| activity_counts(&fixture, cx) == (0, 1));
    assert_eq!(fixture.library.started(), [A], "the cancelled download never began");
    assert!(!file_downloaded(&fixture, cx, "b"));
}

#[gpui::test]
fn cancelling_a_running_download_removes_its_activity_and_frees_the_slot(cx: &mut TestAppContext) {
    let fixture = fixture_with(cx, ScriptedTransport::open(), ScriptedLibrary::gated(), 1,
                               entries(&[("a", A), ("b", B)]));
    download(&fixture, cx, "a");
    wait_for(cx, |cx| activity_counts(&fixture, cx).0 == 1);
    download(&fixture, cx, "b");

    fixture.controller.update(cx, |ctrl, cx| ctrl.cancel_download("a", 0, cx));

    assert!(!is_downloading(&fixture, cx, "a"));
    wait_for(cx, |_| fixture.library.started() == [A, B]);
    assert_eq!(fixture.library.cancelled(), [A], "the running download saw the cancellation");
    fixture.library.release(1);
    wait_for(cx, |cx| file_downloaded(&fixture, cx, "b"));
    wait_for(cx, |cx| activity_counts(&fixture, cx) == (0, 1));
    assert!(!file_downloaded(&fixture, cx, "a"), "a cancelled download is not marked downloaded");
}

#[gpui::test]
fn a_cancelled_download_changes_nothing_when_it_finishes(cx: &mut TestAppContext) {
    let fixture = fixture_with(cx, ScriptedTransport::open(), ScriptedLibrary::gated(), 1,
                               entries(&[("a", A)]));
    let changes = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&changes);
    cx.update(|cx| {
          cx.subscribe(&fixture.controller, move |_ctrl, _event: &LibraryChanged, _cx| {
              counted.fetch_add(1, Ordering::SeqCst);
          })
          .detach();
      });
    download(&fixture, cx, "a");
    wait_for(cx, |cx| activity_counts(&fixture, cx).0 == 1);
    let before = changes.load(Ordering::SeqCst);

    fixture.controller.update(cx, |ctrl, cx| ctrl.cancel_download("a", 0, cx));
    wait_for(cx, |_| fixture.library.cancelled() == [A]);
    wait_for(cx, |cx| fixture.controller.read_with(cx, |ctrl, _| ctrl.download_jobs.is_idle()));

    assert_eq!(changes.load(Ordering::SeqCst), before + 1,
               "cancelling announces one change, and the cancelled job finishing adds none");
}

#[gpui::test]
fn cancelling_from_the_activity_panel_cancels_the_download(cx: &mut TestAppContext) {
    let fixture = fixture_with(cx, ScriptedTransport::open(), ScriptedLibrary::gated(), 1,
                               entries(&[("a", A)]));
    download(&fixture, cx, "a");
    wait_for(cx, |cx| activity_counts(&fixture, cx).0 == 1);
    let cancel = fixture.activity
                        .read_with(cx, |a, _| a.snapshot().items[0].cancel_fn.clone())
                        .expect("a running download offers a cancel action");

    cancel();

    wait_for(cx, |_| fixture.library.cancelled() == [A]);
    assert!(!file_downloaded(&fixture, cx, "a"));
}

#[gpui::test]
fn a_retrying_download_says_so_in_its_activity(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::with(0, &[], &[A]);
    let fixture = fixture_with(cx, ScriptedTransport::open(), library, 3, entries(&[("a", A)]));
    // The service reports "attempt 2, in 3 seconds" as soon as the download begins.
    let expected = t!("activity.downloading_retry", title = "Title", attempt = 2, delay_secs = 3).to_string();

    download(&fixture, cx, "a");

    wait_for(cx, |cx| {
        let items = fixture.activity.read_with(cx, |a, _| a.snapshot().items);
        items.first().is_some_and(|item| *item.label == *expected)
    });
    fixture.library.release(1);
    wait_for(cx, |cx| file_downloaded(&fixture, cx, "a"));
}

#[gpui::test]
fn enqueuing_the_same_file_twice_downloads_it_once(cx: &mut TestAppContext) {
    let fixture = fixture_with(cx, ScriptedTransport::open(), ScriptedLibrary::gated(), 3,
                               entries(&[("a", A)]));

    download(&fixture, cx, "a");
    download(&fixture, cx, "a");
    wait_for(cx, |_| fixture.library.started() == [A]);
    fixture.library.release(2);
    wait_for(cx, |cx| file_downloaded(&fixture, cx, "a"));
    wait_for(cx, |cx| activity_counts(&fixture, cx) == (0, 1));

    assert_eq!(fixture.library.started(), [A]);
}

#[gpui::test]
fn downloads_and_thumbnails_share_one_limit_and_downloads_go_first(cx: &mut TestAppContext) {
    let mut items = entries(&[("d", A)]);
    items.extend(["x", "y"].iter().map(|id| item_with_cover(id)));
    let fixture = fixture_with(cx, ScriptedTransport::gated(), ScriptedLibrary::gated(), 1, items);
    // One cover is fetching, the other is waiting behind it.
    fixture.controller.update(cx, |ctrl, cx| ctrl.refresh_all_thumbnails(cx));
    wait_for(cx, |_| fixture.transport.requests() == 1);

    download(&fixture, cx, "d");

    // The only slot is taken, so the download waits.
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(fixture.library.started().is_empty(), "downloads share the thumbnail limit");
    // When the running cover finishes, the download goes before the waiting one,
    // and holds the slot while the other cover keeps waiting.
    fixture.transport.release(1);
    wait_for(cx, |_| fixture.library.started() == [A]);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(fixture.transport.requests(), 1, "the waiting cover stays behind the download");
    // Once the download is done, the other cover gets its turn.
    fixture.library.release(1);
    wait_for(cx, |cx| file_downloaded(&fixture, cx, "d"));
    wait_for(cx, |_| fixture.transport.requests() == 2);
    fixture.transport.release(1);
    wait_for(cx, |cx| cached(cx, "x") && cached(cx, "y"));
}
