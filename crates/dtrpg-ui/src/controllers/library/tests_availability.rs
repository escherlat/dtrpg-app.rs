//! End-to-end tests for availability checks: a real controller, the real work
//! queue and event loop, and a scripted library service.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use gpui::TestAppContext;

use super::{tests_support::*, *};

fn check(fixture: &Fixture, cx: &mut TestAppContext, id: &str) {
    let id: Arc<str> = Arc::from(id);
    fixture.controller
           .update(cx, |ctrl, cx| ctrl.maybe_check_item(id, cx));
}

fn queue_in_background(fixture: &Fixture, cx: &mut TestAppContext, ids: &[&str]) {
    let ids: Vec<Arc<str>> = ids.iter().map(|id| Arc::from(*id)).collect();
    fixture.controller.update(cx, |ctrl, _| {
                          ctrl.enqueue_checks(ids.into_iter(), Priority::Background)
                      });
}

fn is_checking(fixture: &Fixture, cx: &mut TestAppContext, id: &str) -> bool {
    fixture.controller
           .read_with(cx, |ctrl, _| ctrl.is_checking(id))
}

fn item<T>(fixture: &Fixture, cx: &mut TestAppContext, id: &str,
           read: impl Fn(&LibraryItem) -> T)
           -> T {
    fixture.controller
           .read_with(cx, |ctrl, _| read(&ctrl.item_by_id(id).unwrap()))
}

fn settle(cx: &mut TestAppContext) {
    cx.run_until_parked();
    std::thread::sleep(std::time::Duration::from_millis(50));
    cx.run_until_parked();
}

fn catalog(numbers: &[(&str, u64)]) -> Vec<LibraryItem> {
    numbers.iter()
           .map(|(id, number)| item_numbered(id, *number))
           .collect()
}

#[gpui::test]
fn a_check_refreshes_the_item_and_shows_a_spinner_while_it_runs(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::checking(0, &[], &[]);
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               library,
                               3,
                               catalog(&[("a", 1)]));

    check(&fixture, cx, "a");
    wait_for(cx, |cx| is_checking(&fixture, cx, "a"));
    fixture.library.release_checks(1);
    wait_for(cx, |cx| !is_checking(&fixture, cx, "a"));

    assert_eq!(&*item(&fixture, cx, "a", |item| item.title.clone()),
               REFRESHED_TITLE);
    assert!(item(&fixture, cx, "a", |item| item.is_available));
    assert!(item(&fixture, cx, "a", |item| item.availability_last_checked
                                               .is_some()));
}

#[gpui::test]
fn an_item_the_server_no_longer_has_is_marked_unavailable(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::checking(usize::MAX / 2, &[1], &[]);
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               library,
                               3,
                               catalog(&[("a", 1)]));

    check(&fixture, cx, "a");
    wait_for(cx, |_| fixture.library.checked() == [1]);
    wait_for(cx, |cx| !item(&fixture, cx, "a", |item| item.is_available));

    assert!(item(&fixture, cx, "a", |item| item.availability_last_checked
                                               .is_some()));
    assert!(!is_checking(&fixture, cx, "a"));
}

#[gpui::test]
fn a_failed_check_leaves_the_item_alone(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::checking(usize::MAX / 2, &[], &[1]);
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               library,
                               3,
                               catalog(&[("a", 1)]));

    check(&fixture, cx, "a");
    wait_for(cx, |_| fixture.library.checked() == [1]);
    wait_for(cx, |cx| !is_checking(&fixture, cx, "a"));

    assert!(item(&fixture, cx, "a", |item| item.is_available),
            "a network error is not proof of removal");
    assert!(item(&fixture, cx, "a", |item| item.availability_last_checked
                                               .is_none()));
    assert_eq!(&*item(&fixture, cx, "a", |item| item.title.clone()), "a");
}

#[gpui::test]
fn checks_run_one_at_a_time(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::checking(0, &[], &[]);
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               library,
                               3,
                               catalog(&[("a", 1), ("b", 2), ("c", 3)]));

    queue_in_background(&fixture, cx, &["a", "b", "c"]);
    wait_for(cx, |_| fixture.library.checked() == [1]);
    settle(cx);

    assert_eq!(fixture.library.checked(),
               [1],
               "the others wait for the first to finish");
    fixture.library.release_checks(1);
    wait_for(cx, |_| fixture.library.checked() == [1, 2]);
    settle(cx);
    assert_eq!(fixture.library.checked(), [1, 2]);
    fixture.library.release_checks(2);
    wait_for(cx, |cx| {
        !is_checking(&fixture, cx, "c") && fixture.library.checked() == [1, 2, 3]
    });
}

#[gpui::test]
fn viewing_an_item_upgrades_its_waiting_background_check(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::checking(0, &[], &[]);
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               library,
                               3,
                               catalog(&[("a", 1), ("b", 2), ("c", 3)]));
    queue_in_background(&fixture, cx, &["a", "c", "b"]);
    wait_for(cx, |_| fixture.library.checked() == [1]);

    check(&fixture, cx, "b");
    fixture.library.release_checks(3);

    wait_for(cx, |_| fixture.library.checked().len() == 3);
    settle(cx);
    assert_eq!(fixture.library.checked(),
               [1, 2, 3],
               "b was checked ahead of c, and only once");
}

#[gpui::test]
fn a_background_request_does_not_displace_a_waiting_check(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::checking(0, &[], &[]);
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               library,
                               3,
                               catalog(&[("a", 1), ("b", 2), ("c", 3)]));
    queue_in_background(&fixture, cx, &["a", "b", "c"]);
    wait_for(cx, |_| fixture.library.checked() == [1]);

    queue_in_background(&fixture, cx, &["a", "b", "c"]);
    fixture.library.release_checks(3);

    wait_for(cx, |_| fixture.library.checked().len() == 3);
    settle(cx);
    assert_eq!(fixture.library.checked(),
               [1, 2, 3],
               "each item is checked once, in order");
}

#[gpui::test]
fn the_users_batch_goes_ahead_of_background_checks(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::checking(0, &[], &[]);
    let mut items = catalog(&[("a", 1), ("b", 2), ("c", 3), ("d", 4)]);
    // Checked a moment ago, so the batch leaves it out.
    items[3].availability_last_checked = Some(std::time::SystemTime::now());
    let fixture = fixture_with(cx, ScriptedTransport::open(), library, 3, items);
    queue_in_background(&fixture, cx, &["a", "d"]);
    wait_for(cx, |_| fixture.library.checked() == [1]);

    fixture.controller
           .update(cx, |ctrl, cx| ctrl.request_manual_check_batch(cx));
    fixture.library.release_checks(4);

    wait_for(cx, |_| fixture.library.checked().len() == 4);
    settle(cx);
    assert_eq!(fixture.library.checked(),
               [1, 2, 3, 4],
               "b and c, asked for by the user, ran before d");
}

#[gpui::test]
fn the_timers_batch_runs_behind_the_users_checks(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::checking(0, &[], &[]);
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               library,
                               3,
                               catalog(&[("a", 1), ("b", 2), ("c", 3)]));
    check(&fixture, cx, "a");
    wait_for(cx, |_| fixture.library.checked() == [1]);

    fixture.controller
           .update(cx, |ctrl, cx| ctrl.request_check_batch(cx));
    check(&fixture, cx, "c");
    fixture.library.release_checks(3);

    wait_for(cx, |_| fixture.library.checked().len() == 3);
    settle(cx);
    assert_eq!(fixture.library.checked(),
               [1, 3, 2],
               "c, which the user opened, went ahead of b");
}

#[gpui::test]
fn cancelling_waiting_checks_leaves_the_running_one(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::checking(0, &[], &[]);
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               library,
                               3,
                               catalog(&[("a", 1), ("b", 2)]));
    queue_in_background(&fixture, cx, &["a", "b"]);
    wait_for(cx, |cx| is_checking(&fixture, cx, "a"));

    fixture.controller
           .update(cx, |ctrl, _| ctrl.cancel_pending_checks());
    fixture.library.release_checks(2);

    wait_for(cx, |cx| !is_checking(&fixture, cx, "a"));
    settle(cx);
    assert_eq!(fixture.library.checked(),
               [1],
               "the waiting check never ran");
    assert_eq!(&*item(&fixture, cx, "a", |item| item.title.clone()),
               REFRESHED_TITLE);
}

#[gpui::test]
fn a_check_is_not_held_up_by_a_full_set_of_downloads(cx: &mut TestAppContext) {
    let library = ScriptedLibrary::gated();
    let mut items = catalog(&[("a", 1)]);
    items.push(item_with_files("d", 77, 1));
    let fixture = fixture_with(cx, ScriptedTransport::open(), library, 1, items);
    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.enqueue_item_download("d", 0, "Title", cx)
                      });
    wait_for(cx, |_| fixture.library.started() == [77]);

    check(&fixture, cx, "a");

    wait_for(cx, |cx| {
        item(&fixture, cx, "a", |item| {
            item.availability_last_checked.is_some()
        })
    });
    fixture.library.release(1);
}
