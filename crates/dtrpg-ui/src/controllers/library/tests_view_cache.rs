//! Tests that drawing the library does not cost a copy of the library.
//!
//! The list asks for its items once for every cell it draws, and the catalog is
//! drawn on every change, so these accessors must not copy the result set.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::SystemTime;

use gpui::TestAppContext;

use super::tests_support::*;

fn fixture_with_numbered_items(cx: &mut TestAppContext, count: u64) -> Fixture {
    let items = (1..=count).map(|n| item_numbered(&format!("id{n}"), n))
                           .collect();
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               ScriptedLibrary::open(),
                               3,
                               items);
    fixture.controller
           .update(cx, |ctrl, _| ctrl.invalidate_cache());
    fixture
}

#[gpui::test]
fn the_count_and_a_slice_come_from_the_result_set_without_copying_it(cx: &mut TestAppContext) {
    let fixture = fixture_with_numbered_items(cx, 5);

    fixture.controller.read_with(cx, |ctrl, _| {
                          assert_eq!(ctrl.visible_items_count(), 5);
                          assert_eq!(ctrl.visible_items_slice(1..3).len(), 2);
                          assert_eq!(ctrl.visible_items_slice(4..9).len(), 0, "past the end");
                          assert_eq!(ctrl.visible_items_slice(2..2).len(), 0);
                      });
}

#[gpui::test]
fn a_snapshot_shares_the_result_set_instead_of_copying_it(cx: &mut TestAppContext) {
    let fixture = fixture_with_numbered_items(cx, 5);

    fixture.controller.read_with(cx, |ctrl, _| {
                          let snapshot = ctrl.snapshot();

                          assert!(Arc::ptr_eq(&snapshot.items, &ctrl.visible_items_shared()));
                          assert_eq!(snapshot.matched_count, 5);
                          assert_eq!(snapshot.filter_count, 5);
                      });
}

#[gpui::test]
fn the_snapshot_knows_every_product_id_in_the_catalog(cx: &mut TestAppContext) {
    let items = vec![item_with_files("a", 11, 1), item_with_files("b", 22, 1)];
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               ScriptedLibrary::open(),
                               3,
                               items);
    fixture.controller
           .update(cx, |ctrl, _| ctrl.invalidate_cache());

    let ids = fixture.controller
                     .read_with(cx, |ctrl, _| ctrl.snapshot().catalog_ids);

    assert!(ids.contains(&11) && ids.contains(&22));
    assert!(!ids.contains(&0), "an item without an id adds none");
}

#[gpui::test]
fn a_thumbnail_attempt_is_recorded_without_rebuilding_the_result_set(cx: &mut TestAppContext) {
    let fixture = fixture_with_numbered_items(cx, 5);
    let at = SystemTime::now();

    let (rebuilt, in_cache, in_catalog) =
        fixture.controller.update(cx, |ctrl, _| {
                              // Only the address is kept, so no reference to
                              // the set outlives this
                              // line and the set is not copied to write to it.
                              let before = Arc::as_ptr(&ctrl.visible_items_shared());
                              ctrl.note_thumbnail_attempt(&Arc::from("id3"), at);
                              let after = Arc::as_ptr(&ctrl.visible_items_shared());
                              let in_cache = ctrl.visible_items()
                                                 .iter()
                                                 .find(|item| &*item.id == "id3")
                                                 .and_then(|item| item.thumbnail_last_attempted);
                              let in_catalog =
                                  ctrl.thumbnail_attempts.get(&Arc::from("id3")).copied();
                              (before != after, in_cache, in_catalog)
                          });

    assert!(!rebuilt, "the result set was rebuilt");
    assert_eq!(in_cache, Some(at));
    assert_eq!(in_catalog, Some(at));
}

#[gpui::test]
fn a_thumbnail_attempt_for_an_item_nobody_has_changes_nothing(cx: &mut TestAppContext) {
    let fixture = fixture_with_numbered_items(cx, 2);

    fixture.controller.update(cx, |ctrl, _| {
                          ctrl.note_thumbnail_attempt(&Arc::from("missing"), SystemTime::now());

                          assert!(ctrl.visible_items()
                                      .iter()
                                      .all(|item| item.thumbnail_last_attempted.is_none()));
                      });
}
