//! Tests for tags in the library controller: loading them from the index,
//! editing them, and filtering the catalog by a tag. The index is real, in the
//! fixture's temporary directory; the work runs on the real background
//! executor.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use dtrpg_core::data::activity::ActivityStatus;
use dtrpg_core::data::index::LibraryIndex;
use dtrpg_core::data::tag_color::TagColor;
use gpui::TestAppContext;

use super::{tests_support::*, *};

/// A controller whose catalog is items `a`, `b` and `c`, all of them in the
/// library index too.
fn fixture_with_index(cx: &mut TestAppContext) -> Fixture {
    let items = vec![item_numbered("a", 1),
                     item_numbered("b", 2),
                     item_numbered("c", 3)];
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               ScriptedLibrary::open(),
                               3,
                               items.clone());
    LibraryIndex::open_and_migrate(&fixture.cache).unwrap()
                                                  .replace_all_items(&items)
                                                  .unwrap();
    fixture
}

fn index(fixture: &Fixture) -> LibraryIndex {
    LibraryIndex::open_and_migrate(&fixture.cache).unwrap()
}

fn names_of(fixture: &Fixture, cx: &mut TestAppContext, id: &str) -> Vec<String> {
    fixture.controller.read_with(cx, |ctrl, _| {
                          ctrl.tags_of(id)
                              .iter()
                              .map(|tag| tag.name.to_string())
                              .collect()
                      })
}

fn add(fixture: &Fixture, cx: &mut TestAppContext, id: &str, tag: &str) {
    fixture.controller
           .update(cx, |ctrl, cx| ctrl.add_tag(id, tag, cx));
}

fn visible_ids(fixture: &Fixture, cx: &mut TestAppContext) -> Vec<String> {
    fixture.controller.read_with(cx, |ctrl, _| {
                          let mut ids: Vec<String> = ctrl.visible_items()
                                                         .iter()
                                                         .map(|i| i.id.to_string())
                                                         .collect();
                          ids.sort();
                          ids
                      })
}

#[gpui::test]
fn tags_are_read_from_the_index_when_asked(cx: &mut TestAppContext) {
    let fixture = fixture_with_index(cx);
    index(&fixture).add_tag("a", "solo").unwrap();

    fixture.controller.update(cx, |ctrl, cx| ctrl.load_tags(cx));
    wait_for(cx, |cx| !names_of(&fixture, cx, "a").is_empty());

    assert_eq!(names_of(&fixture, cx, "a"), ["solo"]);
    let entries = fixture.controller
                         .read_with(cx, |ctrl, _| ctrl.tag_entries());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].count, 1);
}

#[gpui::test]
fn adding_a_tag_saves_it_and_shows_it(cx: &mut TestAppContext) {
    let fixture = fixture_with_index(cx);

    add(&fixture, cx, "a", "solo");
    wait_for(cx, |cx| !names_of(&fixture, cx, "a").is_empty());

    assert_eq!(names_of(&fixture, cx, "a"), ["solo"]);
    assert_eq!(index(&fixture).tags_for_item("a").unwrap(), ["solo"]);
}

#[gpui::test]
fn removing_a_tag_saves_it_and_shows_it(cx: &mut TestAppContext) {
    let fixture = fixture_with_index(cx);
    add(&fixture, cx, "a", "solo");
    add(&fixture, cx, "a", "to run");
    wait_for(cx, |cx| names_of(&fixture, cx, "a").len() == 2);

    fixture.controller
           .update(cx, |ctrl, cx| ctrl.remove_tag("a", "solo", cx));
    wait_for(cx, |cx| names_of(&fixture, cx, "a").len() == 1);

    assert_eq!(names_of(&fixture, cx, "a"), ["to run"]);
    assert_eq!(index(&fixture).tags_for_item("a").unwrap(), ["to run"]);
}

#[gpui::test]
fn a_tag_gets_a_color(cx: &mut TestAppContext) {
    let fixture = fixture_with_index(cx);
    add(&fixture, cx, "a", "solo");
    wait_for(cx, |cx| !names_of(&fixture, cx, "a").is_empty());

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.set_tag_color("solo", Some(TagColor::Red), cx)
                      });
    wait_for(cx, |cx| {
        fixture.controller.read_with(cx, |ctrl, _| {
                              ctrl.tags_of("a")[0].color == Some(TagColor::Red)
                          })
    });

    assert_eq!(index(&fixture).tag_colors().unwrap()["solo"], TagColor::Red);
}

#[gpui::test]
fn a_tag_that_cannot_be_stored_fails_in_the_activity_panel_and_changes_nothing(cx: &mut TestAppContext)
{
    let fixture = fixture_with_index(cx);

    add(&fixture, cx, "a", "no, commas");
    wait_for(cx, |cx| activity_counts(&fixture, cx) == (0, 1));

    let items = fixture.activity.read_with(cx, |a, _| a.snapshot().items);
    assert!(matches!(items[0].status, ActivityStatus::Error(_)));
    assert!(names_of(&fixture, cx, "a").is_empty());
    assert!(index(&fixture).list_tags().unwrap().is_empty());
}

#[gpui::test]
fn the_tags_to_offer_are_the_ones_the_item_does_not_have(cx: &mut TestAppContext) {
    let fixture = fixture_with_index(cx);
    add(&fixture, cx, "a", "solo");
    add(&fixture, cx, "b", "co-op");
    wait_for(cx, |cx| {
        fixture.controller
               .read_with(cx, |ctrl, _| ctrl.tag_entries().len() == 2)
    });

    let offered: Vec<String> = fixture.controller.read_with(cx, |ctrl, _| {
                                                     ctrl.tags_to_offer("a")
                                                         .iter()
                                                         .map(|t| t.name.to_string())
                                                         .collect()
                                                 });

    assert_eq!(offered, ["co-op"]);
}

// ── Filtering ────────────────────────────────────────────────────────────────

#[gpui::test]
fn filtering_by_a_tag_shows_only_the_items_that_have_it(cx: &mut TestAppContext) {
    let fixture = fixture_with_index(cx);
    add(&fixture, cx, "a", "solo");
    add(&fixture, cx, "b", "solo");
    wait_for(cx, |cx| {
        fixture.controller.read_with(cx, |ctrl, _| {
                              ctrl.tag_entries().first().is_some_and(|t| t.count == 2)
                          })
    });

    fixture.controller
           .update(cx, |ctrl, cx| ctrl.filter_by_tag("SOLO", cx));

    assert_eq!(visible_ids(&fixture, cx), ["a", "b"]);
}

#[gpui::test]
fn a_tag_filter_follows_the_tags_when_they_change(cx: &mut TestAppContext) {
    let fixture = fixture_with_index(cx);
    add(&fixture, cx, "a", "solo");
    wait_for(cx, |cx| !names_of(&fixture, cx, "a").is_empty());
    fixture.controller
           .update(cx, |ctrl, cx| ctrl.filter_by_tag("solo", cx));
    assert_eq!(visible_ids(&fixture, cx), ["a"]);

    add(&fixture, cx, "c", "solo");
    wait_for(cx, |cx| !names_of(&fixture, cx, "c").is_empty());

    assert_eq!(visible_ids(&fixture, cx), ["a", "c"]);
}

#[gpui::test]
fn a_tag_filter_ends_when_no_item_has_the_tag_any_more(cx: &mut TestAppContext) {
    let fixture = fixture_with_index(cx);
    add(&fixture, cx, "a", "solo");
    wait_for(cx, |cx| !names_of(&fixture, cx, "a").is_empty());
    fixture.controller
           .update(cx, |ctrl, cx| ctrl.filter_by_tag("solo", cx));

    fixture.controller
           .update(cx, |ctrl, cx| ctrl.remove_tag("a", "solo", cx));
    wait_for(cx, |cx| names_of(&fixture, cx, "a").is_empty());

    let filter = fixture.controller
                        .read_with(cx, |ctrl, _| ctrl.filter.clone());
    assert_eq!(filter, SidebarFilter::AllTitles);
    assert_eq!(visible_ids(&fixture, cx), ["a", "b", "c"]);
}

#[gpui::test]
fn filtering_by_a_tag_nothing_has_does_nothing(cx: &mut TestAppContext) {
    let fixture = fixture_with_index(cx);

    fixture.controller
           .update(cx, |ctrl, cx| ctrl.filter_by_tag("nothing", cx));

    let filter = fixture.controller
                        .read_with(cx, |ctrl, _| ctrl.filter.clone());
    assert_eq!(filter, SidebarFilter::AllTitles);
}

#[gpui::test]
fn tags_that_arrive_after_newer_ones_are_ignored(cx: &mut TestAppContext) {
    let fixture = fixture_with_index(cx);
    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.tag_generation = 5;
                          let stale =
                              TagLibrary::new([("a".to_owned(), vec!["old".to_owned()])].into(),
                                              Default::default());
                          ctrl.show_tags(stale, 4, cx);
                      });

    assert!(names_of(&fixture, cx, "a").is_empty());
}
