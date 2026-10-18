//! Tests for tag colors, tag change times, replacing an item's tags, and the
//! record of what a file's tags last agreed on.

#![allow(clippy::unwrap_used)]

use std::collections::HashSet;

use crate::data::enums::ItemStatus;
use crate::data::index::LibraryIndex;
use crate::data::library::LibraryItem;
use crate::data::tag_color::TagColor;
use crate::util::datetime::now_secs;

fn item(id: &str) -> LibraryItem {
    LibraryItem::new(id,
                     &format!("Title {id}"),
                     "Free League",
                     "",
                     "Core",
                     "PDF",
                     100,
                     10.0,
                     2024,
                     0,
                     ItemStatus::Cloud,
                     "#1C2A44",
                     "A game.",
                     Some(1_700_000_000))
}

fn index_with(ids: &[&str]) -> LibraryIndex {
    let mut index = LibraryIndex::open_in_memory().unwrap();
    let items: Vec<_> = ids.iter().map(|id| item(id)).collect();
    index.replace_all_items(&items).unwrap();
    index
}

fn tags(entries: &[(&str, Option<TagColor>)]) -> Vec<(String, Option<TagColor>)> {
    entries.iter()
           .map(|(name, color)| ((*name).to_owned(), *color))
           .collect()
}

fn names(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|name| (*name).to_owned()).collect()
}

// ── Colors ───────────────────────────────────────────────────────────────────

#[test]
fn a_new_tag_has_no_color() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();

    assert!(index.tag_colors().unwrap().is_empty());
}

#[test]
fn a_tags_color_can_be_set_and_cleared_ignoring_case() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "Solo").unwrap();

    assert!(index.set_tag_color("solo", Some(TagColor::Red)).unwrap());
    assert_eq!(index.tag_colors().unwrap()["solo"], TagColor::Red);

    assert!(index.set_tag_color("SOLO", None).unwrap());
    assert!(index.tag_colors().unwrap().is_empty());
}

#[test]
fn coloring_a_tag_that_does_not_exist_changes_nothing() {
    let index = index_with(&["a"]);

    assert!(!index.set_tag_color("solo", Some(TagColor::Red)).unwrap());
    assert!(index.tag_colors().unwrap().is_empty());
}

#[test]
fn a_color_stays_while_any_item_has_the_tag_and_goes_with_the_last() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("b", "solo").unwrap();
    index.set_tag_color("solo", Some(TagColor::Blue)).unwrap();

    index.remove_tag("a", "solo").unwrap();
    assert_eq!(index.tag_colors().unwrap()["solo"], TagColor::Blue);

    index.remove_tag("b", "solo").unwrap();
    index.add_tag("a", "solo").unwrap();
    assert!(index.tag_colors().unwrap().is_empty());
}

#[test]
fn renaming_a_tag_keeps_its_color() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();
    index.set_tag_color("solo", Some(TagColor::Green)).unwrap();

    index.rename_tag("solo", "Solo play").unwrap();

    assert_eq!(index.tag_colors().unwrap()["solo play"], TagColor::Green);
}

#[test]
fn merging_a_tag_into_another_keeps_the_color_of_the_one_it_joins() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("b", "alone").unwrap();
    index.set_tag_color("solo", Some(TagColor::Red)).unwrap();
    index.set_tag_color("alone", Some(TagColor::Blue)).unwrap();

    index.rename_tag("solo", "alone").unwrap();

    assert_eq!(index.tag_colors().unwrap().len(), 1);
    assert_eq!(index.tag_colors().unwrap()["alone"], TagColor::Blue);
}

// ── When an item's tags changed ──────────────────────────────────────────────

#[test]
fn an_item_whose_tags_never_changed_has_no_change_time() {
    let index = index_with(&["a"]);

    assert_eq!(index.tags_changed_at("a").unwrap(), None);
}

#[test]
fn adding_and_removing_a_tag_record_when_the_items_tags_changed() {
    let mut index = index_with(&["a"]);
    let before = now_secs();

    index.add_tag("a", "solo").unwrap();
    let added = index.tags_changed_at("a").unwrap().unwrap();
    assert!(added >= before && added <= now_secs());

    index.set_item_tags("a", &tags(&[("solo", None)]), 5)
         .unwrap();
    index.remove_tag("a", "solo").unwrap();
    assert!(index.tags_changed_at("a").unwrap().unwrap() >= before);
}

#[test]
fn a_tag_that_was_not_added_does_not_move_the_change_time() {
    let mut index = index_with(&["a"]);
    index.set_item_tags("a", &tags(&[("solo", None)]), 5)
         .unwrap();

    assert!(!index.add_tag("a", "SOLO").unwrap());
    assert!(!index.remove_tag("a", "other").unwrap());

    assert_eq!(index.tags_changed_at("a").unwrap(), Some(5));
}

#[test]
fn renaming_and_deleting_a_tag_record_a_change_for_each_item_that_had_it() {
    let mut index = index_with(&["a", "b", "c"]);
    for id in ["a", "b"] {
        index.set_item_tags(id, &tags(&[("solo", None)]), 5)
             .unwrap();
    }
    index.set_item_tags("c", &tags(&[("other", None)]), 5)
         .unwrap();

    index.rename_tag("solo", "alone").unwrap();
    assert!(index.tags_changed_at("a").unwrap().unwrap() > 5);
    assert!(index.tags_changed_at("b").unwrap().unwrap() > 5);
    assert_eq!(index.tags_changed_at("c").unwrap(), Some(5));

    index.set_item_tags("a", &tags(&[("alone", None)]), 5)
         .unwrap();
    index.delete_tag("alone").unwrap();
    assert!(index.tags_changed_at("a").unwrap().unwrap() > 5);
}

// ── Replacing an item's tags ─────────────────────────────────────────────────

#[test]
fn setting_an_items_tags_replaces_them_and_records_when() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "old").unwrap();

    index.set_item_tags("a", &tags(&[("solo", None), ("to run", None)]), 42)
         .unwrap();

    assert_eq!(index.tags_for_item("a").unwrap(), ["solo", "to run"]);
    assert_eq!(index.tags_changed_at("a").unwrap(), Some(42));
    assert_eq!(index.list_tags().unwrap(), ["solo", "to run"]);
}

#[test]
fn setting_no_tags_clears_the_item_and_the_tags_nothing_else_has() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("a", "shared").unwrap();
    index.add_tag("b", "shared").unwrap();

    index.set_item_tags("a", &[], 7).unwrap();

    assert!(index.tags_for_item("a").unwrap().is_empty());
    assert_eq!(index.list_tags().unwrap(), ["shared"]);
}

#[test]
fn setting_an_items_tags_keeps_the_spelling_the_index_has() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("b", "Solo").unwrap();

    index.set_item_tags("a", &tags(&[("solo", None)]), 7)
         .unwrap();

    assert_eq!(index.tags_for_item("a").unwrap(), ["Solo"]);
}

#[test]
fn a_color_from_a_file_fills_in_a_tag_with_none_and_never_replaces_one() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("b", "plain").unwrap();
    index.add_tag("b", "painted").unwrap();
    index.set_tag_color("painted", Some(TagColor::Red)).unwrap();

    index.set_item_tags("a",
                        &tags(&[("plain", Some(TagColor::Green)),
                                ("painted", Some(TagColor::Blue)),
                                ("fresh", Some(TagColor::Purple))]),
                        7)
         .unwrap();

    let colors = index.tag_colors().unwrap();
    assert_eq!(colors["plain"], TagColor::Green);
    assert_eq!(colors["painted"], TagColor::Red);
    assert_eq!(colors["fresh"], TagColor::Purple);
}

#[test]
fn setting_an_items_tags_refreshes_search() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "old").unwrap();

    index.set_item_tags("a", &tags(&[("solo", None)]), 7)
         .unwrap();

    assert_eq!(index.search("solo").unwrap().len(), 1);
    assert!(index.search("old").unwrap().is_empty());
}

#[test]
fn setting_the_tags_of_an_item_that_is_not_in_the_catalog_is_an_error() {
    let mut index = index_with(&["a"]);

    assert!(index.set_item_tags("missing", &tags(&[("solo", None)]), 7)
                 .is_err());
    assert!(index.list_tags().unwrap().is_empty());
}

#[test]
fn an_invalid_tag_leaves_the_items_tags_as_they_were() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();

    let result = index.set_item_tags("a", &tags(&[("fine", None), ("no, commas", None)]), 7);

    assert!(result.is_err());
    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);
}

// ── What a file's tags last agreed on ────────────────────────────────────────

#[test]
fn a_file_that_was_never_synced_has_no_snapshot() {
    let index = index_with(&["a"]);

    assert_eq!(index.file_tag_snapshot("/a/one.pdf").unwrap(), None);
}

#[test]
fn a_snapshot_is_read_back_as_recorded_and_can_be_replaced() {
    let index = index_with(&["a"]);

    index.record_file_tag_sync("/a/one.pdf", "a", &names(&["solo", "to run"]), 1)
         .unwrap();
    assert_eq!(index.file_tag_snapshot("/a/one.pdf").unwrap(),
               Some(names(&["solo", "to run"])));

    index.record_file_tag_sync("/a/one.pdf", "a", &[], 2)
         .unwrap();
    assert_eq!(index.file_tag_snapshot("/a/one.pdf").unwrap(),
               Some(Vec::new()));
}

#[test]
fn a_snapshot_with_no_tags_is_different_from_no_snapshot() {
    let index = index_with(&["a"]);

    index.record_file_tag_sync("/a/one.pdf", "a", &[], 1)
         .unwrap();

    assert!(index.file_tag_snapshot("/a/one.pdf").unwrap().is_some());
}

#[test]
fn one_files_snapshot_can_be_forgotten_alone() {
    let index = index_with(&["a"]);
    index.record_file_tag_sync("/a/one.pdf", "a", &names(&["solo"]), 1)
         .unwrap();
    index.record_file_tag_sync("/a/two.pdf", "a", &names(&["solo"]), 1)
         .unwrap();

    index.forget_file_tag_sync("/a/one.pdf").unwrap();

    assert_eq!(index.file_tag_snapshot("/a/one.pdf").unwrap(), None);
    assert!(index.file_tag_snapshot("/a/two.pdf").unwrap().is_some());
}

#[test]
fn snapshots_of_files_that_are_gone_are_forgotten() {
    let mut index = index_with(&["a"]);
    index.record_file_tag_sync("/a/one.pdf", "a", &names(&["solo"]), 1)
         .unwrap();
    index.record_file_tag_sync("/a/two.pdf", "a", &names(&["solo"]), 1)
         .unwrap();
    let present: HashSet<String> = ["/a/one.pdf".to_owned()].into();

    let forgotten = index.forget_file_tag_syncs_except(&present).unwrap();

    assert_eq!(forgotten, 1);
    assert!(index.file_tag_snapshot("/a/one.pdf").unwrap().is_some());
    assert_eq!(index.file_tag_snapshot("/a/two.pdf").unwrap(), None);
}

// ── Upgrading ────────────────────────────────────────────────────────────────

#[test]
fn upgrading_a_version_three_index_adds_colors_and_keeps_its_tags() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();
    // Put the tables back as version 3 had them.
    index.conn
         .execute_batch("ALTER TABLE tags DROP COLUMN color;
                         DROP TABLE item_tags_changed;
                         DROP TABLE file_tag_sync;
                         PRAGMA user_version = 3;")
         .unwrap();

    crate::data::index::schema::ensure_schema(&index.conn).unwrap();

    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);
    assert!(index.set_tag_color("solo", Some(TagColor::Red)).unwrap());
    assert_eq!(index.tags_changed_at("a").unwrap(), None);
    assert_eq!(index.file_tag_snapshot("/a/one.pdf").unwrap(), None);
}
