//! Tests for editing tags: the index changes, and the files of the items the
//! edit affected follow it.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::data::index::IndexError;
use crate::data::tag_library::TagLibrary;
use crate::platform::FileTag;
use crate::services::tag_projection::test_platform::{InMemory, file, item, plain};

/// A library index on disk holding items `a`, `b` and `c`.
fn on_disk() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut index = LibraryIndex::open_and_migrate(dir.path()).unwrap();
    index.replace_all_items(&[item("a"), item("b"), item("c")])
         .unwrap();
    dir
}

fn index(dir: &tempfile::TempDir) -> LibraryIndex {
    LibraryIndex::open_and_migrate(dir.path()).unwrap()
}

fn files() -> Vec<TaggedFile> {
    vec![file("a", "/a/one.pdf"),
         file("a", "/a/two.pdf"),
         file("b", "/b/one.pdf")]
}

fn add(item_id: &str, tag: &str) -> TagEdit {
    TagEdit::Add { item_id: item_id.to_owned(),
                   tag:     tag.to_owned(), }
}

fn apply(platform: &InMemory, dir: &tempfile::TempDir, edit: &TagEdit, settings: SyncSettings)
         -> Result<TagEditOutcome, IndexError> {
    apply_tag_edit(platform, dir.path(), edit, &files(), settings)
}

#[test]
fn adding_a_tag_tags_the_item_and_every_one_of_its_files_and_no_others() {
    let dir = on_disk();
    let platform = InMemory::new();

    let outcome = apply(&platform, &dir, &add("a", "solo"), SyncSettings::default()).unwrap();

    assert!(outcome.changed && outcome.failures.is_empty());
    assert_eq!(index(&dir).tags_for_item("a").unwrap(), ["solo"]);
    assert_eq!(platform.tags_of("/a/one.pdf"), plain(&["solo"]));
    assert_eq!(platform.tags_of("/a/two.pdf"), plain(&["solo"]));
    assert!(platform.tags_of("/b/one.pdf").is_empty());
    assert!(outcome.failures.is_empty());
}

#[test]
fn adding_a_tag_the_item_has_changes_nothing_and_writes_nothing() {
    let dir = on_disk();
    let platform = InMemory::new();
    apply(&platform, &dir, &add("a", "solo"), SyncSettings::default()).unwrap();
    let writes = platform.write_count();

    let outcome = apply(&platform, &dir, &add("a", "SOLO"), SyncSettings::default()).unwrap();

    assert!(!outcome.changed);
    assert_eq!(platform.write_count(), writes);
}

#[test]
fn removing_a_tag_takes_it_off_the_item_and_its_files() {
    let dir = on_disk();
    let platform = InMemory::new();
    apply(&platform, &dir, &add("a", "solo"), SyncSettings::default()).unwrap();
    apply(&platform,
          &dir,
          &add("a", "to run"),
          SyncSettings::default()).unwrap();

    let outcome = apply(&platform,
                        &dir,
                        &TagEdit::Remove { item_id: "a".to_owned(),
                                           tag:     "solo".to_owned(), },
                        SyncSettings::default()).unwrap();

    assert!(outcome.changed);
    assert_eq!(platform.tags_of("/a/one.pdf"), plain(&["to run"]));
    assert!(TagLibrary::load(&index(&dir)).unwrap()
                                          .entry("solo")
                                          .is_none());
}

#[test]
fn removing_the_last_tag_clears_the_files() {
    let dir = on_disk();
    let platform = InMemory::new();
    apply(&platform, &dir, &add("a", "solo"), SyncSettings::default()).unwrap();

    apply(&platform,
          &dir,
          &TagEdit::Remove { item_id: "a".to_owned(),
                             tag:     "solo".to_owned(), },
          SyncSettings::default()).unwrap();

    assert!(platform.tags_of("/a/one.pdf").is_empty());
    assert!(platform.tags_of("/a/two.pdf").is_empty());
}

#[test]
fn a_tag_that_cannot_be_stored_is_an_error_and_changes_nothing() {
    let dir = on_disk();
    let platform = InMemory::new();

    let result = apply(&platform,
                       &dir,
                       &add("a", "no, commas"),
                       SyncSettings::default());

    assert!(matches!(result, Err(IndexError::InvalidTag { .. })));
    assert!(index(&dir).list_tags().unwrap().is_empty());
    assert_eq!(platform.write_count(), 0);
}

#[test]
fn tagging_an_item_that_is_not_in_the_index_is_an_error() {
    let dir = on_disk();
    let platform = InMemory::new();

    let result = apply(&platform,
                       &dir,
                       &add("missing", "solo"),
                       SyncSettings::default());

    assert!(matches!(result, Err(IndexError::NoSuchItem(_))));
}

#[test]
fn a_color_is_set_on_the_tag_and_written_to_the_files_of_every_item_that_has_it() {
    let dir = on_disk();
    let platform = InMemory::new();
    apply(&platform, &dir, &add("a", "solo"), SyncSettings::default()).unwrap();
    apply(&platform, &dir, &add("b", "solo"), SyncSettings::default()).unwrap();
    apply(&platform, &dir, &add("c", "other"), SyncSettings::default()).unwrap();

    let outcome = apply(&platform,
                        &dir,
                        &TagEdit::SetColor { tag:   "Solo".to_owned(),
                                             color: Some(TagColor::Red), },
                        SyncSettings::default()).unwrap();

    assert!(outcome.changed);
    assert_eq!(index(&dir).tag_colors().unwrap()["solo"], TagColor::Red);
    let red = [FileTag::colored("solo", TagColor::Red)];
    assert_eq!(platform.tags_of("/a/one.pdf"), red);
    assert_eq!(platform.tags_of("/a/two.pdf"), red);
    assert_eq!(platform.tags_of("/b/one.pdf"), red);
}

#[test]
fn clearing_a_color_clears_it_on_the_files() {
    let dir = on_disk();
    let platform = InMemory::new();
    apply(&platform, &dir, &add("a", "solo"), SyncSettings::default()).unwrap();
    let color = |color| TagEdit::SetColor { tag: "solo".to_owned(),
                                            color };
    apply(&platform,
          &dir,
          &color(Some(TagColor::Red)),
          SyncSettings::default()).unwrap();

    apply(&platform, &dir, &color(None), SyncSettings::default()).unwrap();

    assert_eq!(platform.tags_of("/a/one.pdf"), plain(&["solo"]));
}

#[test]
fn coloring_a_tag_nothing_has_changes_nothing() {
    let dir = on_disk();
    let platform = InMemory::new();

    let outcome = apply(&platform,
                        &dir,
                        &TagEdit::SetColor { tag:   "nothing".to_owned(),
                                             color: Some(TagColor::Red), },
                        SyncSettings::default()).unwrap();

    assert!(!outcome.changed);
    assert_eq!(platform.write_count(), 0);
}

#[test]
fn with_writing_off_the_index_changes_and_the_files_do_not() {
    let dir = on_disk();
    let platform = InMemory::new();
    let settings = SyncSettings { write_to_files:  false,
                                  read_from_files: true, };

    let outcome = apply(&platform, &dir, &add("a", "solo"), settings).unwrap();

    assert!(outcome.changed && outcome.failures.is_empty());
    assert_eq!(index(&dir).tags_for_item("a").unwrap(), ["solo"]);
    assert_eq!(platform.write_count(), 0);
}

#[test]
fn where_tags_are_not_kept_on_files_only_the_index_changes() {
    let dir = on_disk();
    let platform = InMemory::without_tags_on_files();

    let outcome = apply(&platform, &dir, &add("a", "solo"), SyncSettings::default()).unwrap();

    assert!(outcome.changed);
    assert_eq!(index(&dir).tags_for_item("a").unwrap(), ["solo"]);
    assert_eq!(platform.write_count(), 0);
}

#[test]
fn a_file_that_cannot_be_written_is_reported_and_the_edit_stands() {
    let dir = on_disk();
    let platform = InMemory::new().with_broken("/a/one.pdf");

    let outcome = apply(&platform, &dir, &add("a", "solo"), SyncSettings::default()).unwrap();

    assert_eq!(outcome.failures.len(), 1);
    assert_eq!(outcome.failures[0].path, PathBuf::from("/a/one.pdf"));
    assert_eq!(index(&dir).tags_for_item("a").unwrap(), ["solo"]);
    assert_eq!(platform.tags_of("/a/two.pdf"), plain(&["solo"]));
}
