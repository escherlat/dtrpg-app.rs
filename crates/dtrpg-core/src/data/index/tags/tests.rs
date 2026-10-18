//! Tests for tag storage in the library index.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::data::enums::ItemStatus;
use crate::data::index::LibraryIndex;
use crate::data::library::LibraryItem;

fn item(id: &str, title: &str) -> LibraryItem {
    LibraryItem::new(id,
                     title,
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
    let items: Vec<_> = ids.iter()
                           .map(|id| item(id, &format!("Title {id}")))
                           .collect();
    index.replace_all_items(&items).unwrap();
    index
}

fn found_ids(index: &LibraryIndex, query: &str) -> Vec<String> {
    index.search(query)
         .unwrap()
         .iter()
         .map(|item| item.id.to_string())
         .collect()
}

#[test]
fn an_added_tag_is_listed_for_the_item_and_overall() {
    let mut index = index_with(&["a", "b"]);

    assert!(index.add_tag("a", "solo").unwrap());

    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);
    assert!(index.tags_for_item("b").unwrap().is_empty());
    assert_eq!(index.list_tags().unwrap(), ["solo"]);
}

#[test]
fn tags_are_unique_ignoring_case_and_keep_the_first_spelling() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("a", "solo").unwrap();

    assert!(!index.add_tag("a", "Solo").unwrap());
    assert!(index.add_tag("b", "SOLO").unwrap());

    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);
    assert_eq!(index.tags_for_item("b").unwrap(), ["solo"]);
    assert_eq!(index.list_tags().unwrap(), ["solo"]);
}

#[test]
fn uniqueness_ignores_case_beyond_ascii() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "épique").unwrap();

    assert!(!index.add_tag("a", "ÉPIQUE").unwrap());
    assert_eq!(index.list_tags().unwrap(), ["épique"]);
}

#[test]
fn a_tag_is_trimmed() {
    let mut index = index_with(&["a"]);

    index.add_tag("a", "  to run  ").unwrap();

    assert_eq!(index.tags_for_item("a").unwrap(), ["to run"]);
}

#[test]
fn tags_are_listed_ignoring_case() {
    let mut index = index_with(&["a"]);
    for tag in ["beta", "Alpha", "gamma"] {
        index.add_tag("a", tag).unwrap();
    }

    assert_eq!(index.tags_for_item("a").unwrap(),
               ["Alpha", "beta", "gamma"]);
    assert_eq!(index.list_tags().unwrap(), ["Alpha", "beta", "gamma"]);
}

#[test]
fn tags_that_cannot_be_written_to_a_file_are_rejected() {
    let mut index = index_with(&["a"]);
    let too_long = "x".repeat(65);

    for bad in ["", "   ", "a,b", "line\nbreak", too_long.as_str()] {
        let error = index.add_tag("a", bad).unwrap_err();
        assert!(matches!(error, IndexError::InvalidTag { .. }), "{bad:?}");
    }
    assert!(index.tags_for_item("a").unwrap().is_empty());
}

#[test]
fn tagging_an_item_that_does_not_exist_fails() {
    let mut index = index_with(&["a"]);

    let error = index.add_tag("missing", "solo").unwrap_err();

    assert!(matches!(error, IndexError::NoSuchItem(id) if id == "missing"));
    assert!(index.list_tags().unwrap().is_empty());
}

#[test]
fn removing_a_tag_leaves_other_items_tagged() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("b", "solo").unwrap();

    assert!(index.remove_tag("a", "SOLO").unwrap());

    assert!(index.tags_for_item("a").unwrap().is_empty());
    assert_eq!(index.tags_for_item("b").unwrap(), ["solo"]);
    assert_eq!(index.list_tags().unwrap(), ["solo"]);
}

#[test]
fn removing_a_tag_the_item_does_not_have_changes_nothing() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("b", "solo").unwrap();

    assert!(!index.remove_tag("a", "solo").unwrap());
    assert!(!index.remove_tag("a", "never-used").unwrap());

    assert_eq!(index.tags_for_item("b").unwrap(), ["solo"]);
}

#[test]
fn a_tag_nobody_has_any_more_is_gone() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();

    index.remove_tag("a", "solo").unwrap();

    assert!(index.list_tags().unwrap().is_empty());
    // Adding it again starts fresh, with the new spelling.
    index.add_tag("a", "Solo").unwrap();
    assert_eq!(index.list_tags().unwrap(), ["Solo"]);
}

#[test]
fn renaming_a_tag_renames_it_on_every_item() {
    let mut index = index_with(&["a", "b", "c"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("b", "solo").unwrap();
    index.add_tag("c", "co-op").unwrap();

    assert_eq!(index.rename_tag("solo", "single player").unwrap(), 2);

    assert_eq!(index.tags_for_item("a").unwrap(), ["single player"]);
    assert_eq!(index.tags_for_item("b").unwrap(), ["single player"]);
    assert_eq!(index.tags_for_item("c").unwrap(), ["co-op"]);
    assert_eq!(index.list_tags().unwrap(), ["co-op", "single player"]);
}

#[test]
fn renaming_only_the_case_of_a_tag_is_allowed() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();

    assert_eq!(index.rename_tag("solo", "Solo").unwrap(), 1);

    assert_eq!(index.tags_for_item("a").unwrap(), ["Solo"]);
}

#[test]
fn renaming_to_an_existing_tag_merges_them() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("a", "single").unwrap();
    index.add_tag("b", "single").unwrap();

    assert_eq!(index.rename_tag("solo", "Single").unwrap(), 1);

    // "a" had both, so it ends with one tag, not a duplicate.
    assert_eq!(index.tags_for_item("a").unwrap(), ["single"]);
    assert_eq!(index.tags_for_item("b").unwrap(), ["single"]);
    assert_eq!(index.list_tags().unwrap(), ["single"]);
}

#[test]
fn renaming_a_missing_tag_changes_nothing() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();

    assert_eq!(index.rename_tag("nope", "other").unwrap(), 0);

    assert_eq!(index.list_tags().unwrap(), ["solo"]);
}

#[test]
fn renaming_to_an_invalid_tag_changes_nothing() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();

    let error = index.rename_tag("solo", "a,b").unwrap_err();

    assert!(matches!(error, IndexError::InvalidTag { .. }));
    assert_eq!(index.list_tags().unwrap(), ["solo"]);
}

#[test]
fn deleting_a_tag_removes_it_from_every_item() {
    let mut index = index_with(&["a", "b", "c"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("b", "solo").unwrap();
    index.add_tag("c", "co-op").unwrap();

    assert_eq!(index.delete_tag("Solo").unwrap(), 2);

    assert!(index.tags_for_item("a").unwrap().is_empty());
    assert!(index.tags_for_item("b").unwrap().is_empty());
    assert_eq!(index.list_tags().unwrap(), ["co-op"]);
    assert_eq!(index.delete_tag("Solo").unwrap(), 0);
}

#[test]
fn tags_survive_a_catalog_refresh() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("a", "solo").unwrap();

    // The catalog is rewritten in full on every refresh.
    index.replace_all_items(&[item("a", "Title a"), item("b", "Title b")])
         .unwrap();

    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);
    assert_eq!(found_ids(&index, "solo"), ["a"]);
}

#[test]
fn a_tag_on_an_item_missing_from_the_catalog_is_not_listed_but_comes_back() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("b", "solo").unwrap();

    index.replace_all_items(&[item("a", "Title a")]).unwrap();
    assert!(index.list_tags().unwrap().is_empty());

    // A partial catalog must not cost the user their tags.
    index.replace_all_items(&[item("a", "Title a"), item("b", "Title b")])
         .unwrap();
    assert_eq!(index.tags_for_item("b").unwrap(), ["solo"]);
    assert_eq!(index.list_tags().unwrap(), ["solo"]);
}

#[test]
fn search_finds_items_by_tag_and_follows_every_change() {
    let mut index = index_with(&["a", "b"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("b", "solo").unwrap();
    assert_eq!(found_ids(&index, "solo").len(), 2);

    index.remove_tag("a", "solo").unwrap();
    assert_eq!(found_ids(&index, "solo"), ["b"]);

    index.rename_tag("solo", "duet").unwrap();
    assert!(found_ids(&index, "solo").is_empty());
    assert_eq!(found_ids(&index, "duet"), ["b"]);

    index.delete_tag("duet").unwrap();
    assert!(found_ids(&index, "duet").is_empty());
}

#[test]
fn tags_remain_after_the_index_is_reopened() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut index = LibraryIndex::open_and_migrate(dir.path()).unwrap();
        index.replace_all_items(&[item("a", "Title a")]).unwrap();
        index.add_tag("a", "solo").unwrap();
    }

    let index = LibraryIndex::open_and_migrate(dir.path()).unwrap();

    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);
}

#[test]
fn upgrading_an_older_index_keeps_its_tags_when_the_catalog_is_rewritten() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();
    // Put the tables back as version 2 had them, with the foreign key.
    index.conn
         .execute_batch("DROP TABLE item_tags;
                         CREATE TABLE item_tags (
                             item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                             tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                             PRIMARY KEY (item_id, tag_id)
                         );
                         INSERT INTO item_tags (item_id, tag_id) SELECT 'a', id FROM tags;
                         PRAGMA user_version = 2;")
         .unwrap();

    crate::data::index::schema::ensure_schema(&index.conn).unwrap();
    index.replace_all_items(&[]).unwrap();
    index.replace_all_items(&[item("a", "Title a")]).unwrap();

    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);
}

#[test]
fn every_items_tags_can_be_read_at_once() {
    let mut index = index_with(&["a", "b", "c"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("a", "Beta").unwrap();
    index.add_tag("b", "solo").unwrap();

    let all = index.all_item_tags().unwrap();

    assert_eq!(all.len(), 2);
    assert_eq!(all["a"], ["Beta", "solo"]);
    assert_eq!(all["b"], ["solo"]);
    assert!(!all.contains_key("c"));
}
