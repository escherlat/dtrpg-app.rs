//! Tests for tags that changed on both sides, the settings that stop either
//! direction, files of one item, files that come and go, and projecting an
//! item's tags onto its files.

#![allow(clippy::unwrap_used)]

use super::test_platform::{InMemory, file, index_with, plain};
use super::tests::reconcile;
use super::*;
use crate::data::index::LibraryIndex;

/// An index with item `a` tagged "solo", and one file of it, in step.
fn synced() -> (LibraryIndex, InMemory) {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();
    let platform = InMemory::new();
    reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);
    (index, platform)
}

/// Changes "solo" to "index" in the index and to "file" on the file, the file
/// at `file_changed_at`.
fn change_both(index: &mut LibraryIndex, platform: &InMemory, file_changed_at: Option<i64>) {
    index.set_item_tags("a", &[("index".to_owned(), None)], now_secs())
         .unwrap();
    platform.edit("/a/one.pdf", &plain(&["file"]), file_changed_at);
}

fn one() -> [TaggedFile; 1] {
    [file("a", "/a/one.pdf")]
}

// ── Both changed ─────────────────────────────────────────────────────────────

#[test]
fn when_both_changed_a_later_edit_to_the_file_wins() {
    let (mut index, platform) = synced();
    change_both(&mut index, &platform, Some(now_secs() + 100));

    let report = reconcile(&platform, &mut index, &one());

    assert_eq!(report.imported.len(), 1);
    assert_eq!(index.tags_for_item("a").unwrap(), ["file"]);
}

#[test]
fn when_both_changed_a_later_edit_in_libri_wins() {
    let (mut index, platform) = synced();
    change_both(&mut index, &platform, Some(now_secs() - 100));

    let report = reconcile(&platform, &mut index, &one());

    assert_eq!(report.written.len(), 1);
    assert_eq!(platform.tags_of("/a/one.pdf"), plain(&["index"]));
}

#[test]
fn when_both_changed_and_the_file_time_is_unknown_the_index_wins() {
    let (mut index, platform) = synced();
    change_both(&mut index, &platform, None);

    reconcile(&platform, &mut index, &one());

    assert_eq!(platform.tags_of("/a/one.pdf"), plain(&["index"]));
    assert_eq!(index.tags_for_item("a").unwrap(), ["index"]);
}

// ── The settings ─────────────────────────────────────────────────────────────

fn only(write_to_files: bool, read_from_files: bool) -> SyncSettings {
    SyncSettings { write_to_files,
                   read_from_files }
}

#[test]
fn with_writing_off_a_change_in_libri_waits_until_it_is_turned_on() {
    let (mut index, platform) = synced();
    index.add_tag("a", "co-op").unwrap();

    let held = reconcile_files(&platform, &mut index, &one(), only(false, true)).unwrap();
    assert!(held.written.is_empty());
    assert_eq!(platform.tags_of("/a/one.pdf"), plain(&["solo"]));

    let released = reconcile_files(&platform, &mut index, &one(), only(true, true)).unwrap();
    assert_eq!(released.written.len(), 1);
    assert_eq!(platform.tags_of("/a/one.pdf"), plain(&["co-op", "solo"]));
}

#[test]
fn with_reading_off_a_change_to_the_file_waits_until_it_is_turned_on() {
    let (mut index, platform) = synced();
    platform.edit("/a/one.pdf",
                  &plain(&["solo", "stray"]),
                  Some(now_secs() + 100));

    let held = reconcile_files(&platform, &mut index, &one(), only(true, false)).unwrap();
    assert!(held.imported.is_empty());
    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);

    let released = reconcile_files(&platform, &mut index, &one(), only(true, true)).unwrap();
    assert_eq!(released.imported.len(), 1);
    assert_eq!(index.tags_for_item("a").unwrap(), ["solo", "stray"]);
}

#[test]
fn with_reading_off_a_conflict_that_the_file_would_win_is_left_alone() {
    let (mut index, platform) = synced();
    change_both(&mut index, &platform, Some(now_secs() + 100));

    let report = reconcile_files(&platform, &mut index, &one(), only(true, false)).unwrap();

    assert!(report.imported.is_empty() && report.written.is_empty());
    assert_eq!(platform.tags_of("/a/one.pdf"), plain(&["file"]));
    assert_eq!(index.tags_for_item("a").unwrap(), ["index"]);
}

#[test]
fn with_both_off_nothing_moves() {
    let (mut index, platform) = synced();
    index.add_tag("a", "co-op").unwrap();
    let writes = platform.write_count();

    let report = reconcile_files(&platform, &mut index, &one(), only(false, false)).unwrap();

    assert!(report.written.is_empty() && report.imported.is_empty());
    assert_eq!(platform.write_count(), writes);
}

// ── Files of one item ────────────────────────────────────────────────────────

#[test]
fn a_change_to_one_file_of_an_item_reaches_the_item_and_its_other_files() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();
    let platform = InMemory::new();
    let both = [file("a", "/a/one.pdf"), file("a", "/a/two.pdf")];
    reconcile(&platform, &mut index, &both);
    platform.edit("/a/one.pdf",
                  &plain(&["solo", "stray"]),
                  Some(now_secs() + 100));

    let report = reconcile(&platform, &mut index, &both);

    assert_eq!(report.imported, [PathBuf::from("/a/one.pdf")]);
    assert_eq!(report.written, [PathBuf::from("/a/two.pdf")]);
    assert_eq!(platform.tags_of("/a/two.pdf"), plain(&["solo", "stray"]));
}

// ── Files that come and go ───────────────────────────────────────────────────

#[test]
fn a_file_that_went_away_and_came_back_is_not_read_as_having_lost_its_tags() {
    let (mut index, platform) = synced();
    reconcile(&platform, &mut index, &[]);
    platform.edit("/a/one.pdf", &[], None);

    let report = reconcile(&platform, &mut index, &one());

    assert_eq!(report.written.len(), 1);
    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);
    assert_eq!(platform.tags_of("/a/one.pdf"), plain(&["solo"]));
}

#[test]
fn a_file_that_is_still_listed_keeps_its_record() {
    let (mut index, platform) = synced();

    reconcile(&platform, &mut index, &one());

    assert!(index.file_tag_snapshot("/a/one.pdf").unwrap().is_some());
}

// ── Projecting an item's tags onto its files ─────────────────────────────────

/// A library index on disk, which `project_indexed_tags` opens by its root.
fn on_disk(setup: impl FnOnce(&mut LibraryIndex)) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let mut index = LibraryIndex::open_and_migrate(dir.path()).unwrap();
    index.replace_all_items(&[crate::data::library::LibraryItem::new("a",
                                                                      "Title a",
                                                                      "Free League",
                                                                      "",
                                                                      "Core",
                                                                      "PDF",
                                                                      100,
                                                                      10.0,
                                                                      2024,
                                                                      0,
                                                                      crate::data::enums::ItemStatus::Downloaded,
                                                                      "#1C2A44",
                                                                      "A game.",
                                                                      None)])
         .unwrap();
    setup(&mut index);
    dir
}

fn project(platform: &InMemory, dir: &Path, paths: &[&str], settings: SyncSettings)
           -> Vec<ProjectionFailure> {
    let paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    project_indexed_tags(platform, dir, "a", &paths, settings).unwrap()
}

#[test]
fn a_downloaded_file_gets_the_tags_and_colors_its_item_has() {
    let dir = on_disk(|index| {
        index.add_tag("a", "solo").unwrap();
        index.set_tag_color("solo", Some(TagColor::Red)).unwrap();
    });
    let platform = InMemory::new();

    let failures = project(&platform,
                           dir.path(),
                           &["/a/one.pdf"],
                           SyncSettings::default());

    assert!(failures.is_empty());
    assert_eq!(platform.tags_of("/a/one.pdf"),
               [FileTag::colored("solo", TagColor::Red)]);
    let index = LibraryIndex::open_and_migrate(dir.path()).unwrap();
    assert_eq!(index.file_tag_snapshot("/a/one.pdf").unwrap(),
               Some(vec!["solo".to_owned()]));
}

#[test]
fn a_downloaded_file_of_an_untagged_item_is_not_written() {
    let dir = on_disk(|_| {});
    let platform = InMemory::new();

    project(&platform,
            dir.path(),
            &["/a/one.pdf"],
            SyncSettings::default());

    assert_eq!(platform.write_count(), 0);
}

#[test]
fn a_file_that_already_has_the_tags_is_not_rewritten() {
    let dir = on_disk(|index| {
        index.add_tag("a", "solo").unwrap();
    });
    let platform = InMemory::new();
    platform.edit("/a/one.pdf", &plain(&["Solo"]), None);

    project(&platform,
            dir.path(),
            &["/a/one.pdf"],
            SyncSettings::default());

    assert_eq!(platform.write_count(), 0);
}

#[test]
fn removing_an_items_last_tag_clears_its_files() {
    let dir = on_disk(|_| {});
    let platform = InMemory::new();
    platform.edit("/a/one.pdf", &plain(&["solo"]), None);

    project(&platform,
            dir.path(),
            &["/a/one.pdf"],
            SyncSettings::default());

    assert!(platform.tags_of("/a/one.pdf").is_empty());
}

#[test]
fn a_file_downloaded_over_an_old_one_is_not_read_as_having_lost_its_tags() {
    let dir = on_disk(|index| {
        index.add_tag("a", "solo").unwrap();
    });
    let platform = InMemory::new();
    let mut index = LibraryIndex::open_and_migrate(dir.path()).unwrap();
    reconcile(&platform, &mut index, &one());
    drop(index);

    // The file is downloaded again, with writing off, so it is not given the
    // tags.
    project(&platform, dir.path(), &["/a/one.pdf"], only(false, true));
    platform.edit("/a/one.pdf", &[], Some(now_secs() + 100));
    let mut index = LibraryIndex::open_and_migrate(dir.path()).unwrap();
    reconcile_files(&platform, &mut index, &one(), only(false, true)).unwrap();

    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);
}

#[test]
fn projecting_does_nothing_with_writing_off_or_where_tags_are_not_kept_on_files() {
    let dir = on_disk(|index| {
        index.add_tag("a", "solo").unwrap();
    });
    let off = InMemory::new();
    project(&off, dir.path(), &["/a/one.pdf"], only(false, true));
    let unsupported = InMemory::without_tags_on_files();
    project(&unsupported,
            dir.path(),
            &["/a/one.pdf"],
            SyncSettings::default());

    assert_eq!(off.write_count() + unsupported.write_count(), 0);
}

#[test]
fn projecting_reports_the_files_it_could_not_write_and_still_does_the_rest() {
    let dir = on_disk(|index| {
        index.add_tag("a", "solo").unwrap();
    });
    let platform = InMemory::new().with_broken("/a/locked.pdf");

    let failures = project(&platform,
                           dir.path(),
                           &["/a/locked.pdf", "/a/two.pdf"],
                           SyncSettings::default());

    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].path, PathBuf::from("/a/locked.pdf"));
    assert_eq!(platform.tags_of("/a/two.pdf"), plain(&["solo"]));
}

// ── Against real files ───────────────────────────────────────────────────────

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn tags_edited_on_a_real_file_are_read_back_and_a_tag_added_in_libri_is_written() {
    use crate::platform::Platform;

    let dir = on_disk(|index| {
        index.add_tag("a", "solo").unwrap();
    });
    let path = dir.path().join("book.pdf");
    std::fs::write(&path, b"data").unwrap();
    let files = [TaggedFile { item_id: "a".to_owned(),
                              path:    path.clone(), }];

    let first =
        reconcile_with_index(&Platform, dir.path(), &files, SyncSettings::default()).unwrap();
    assert_eq!(first.written, std::slice::from_ref(&path));
    assert_eq!(Platform.read_tags(&path).unwrap(), plain(&["solo"]));

    // Edited outside Libri, and later than anything the index did.
    std::thread::sleep(std::time::Duration::from_millis(1_100));
    Platform.write_tags(&path, &plain(&["solo", "stray"]))
            .unwrap();
    let second =
        reconcile_with_index(&Platform, dir.path(), &files, SyncSettings::default()).unwrap();
    assert_eq!(second.imported, std::slice::from_ref(&path));
    let index = LibraryIndex::open_and_migrate(dir.path()).unwrap();
    assert_eq!(index.tags_for_item("a").unwrap(), ["solo", "stray"]);
}
