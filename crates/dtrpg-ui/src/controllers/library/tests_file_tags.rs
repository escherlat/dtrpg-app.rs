//! Tests for keeping the tags on files in step with the library index: which
//! files are looked at, and when.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use dtrpg_core::data::activity::ActivityStatus;
use dtrpg_core::services::tag_projection::{ProjectionFailure, ReconcileReport};
use gpui::TestAppContext;

use super::{file_tags::files_on_disk, tests_support::*, *};
use crate::util::file_presence::resolved_file_path;

fn report(written: &[&str], imported: &[&str], failed: &[&str]) -> ReconcileReport {
    ReconcileReport { checked:  written.len() + imported.len() + failed.len(),
                      written:  written.iter().map(PathBuf::from).collect(),
                      imported: imported.iter().map(PathBuf::from).collect(),
                      failed:
                          failed.iter()
                                .map(|path| ProjectionFailure { path:  PathBuf::from(path),
                                                                error: "read-only".to_owned(), })
                                .collect(), }
}

fn fixture_with_items(cx: &mut TestAppContext) -> Fixture {
    fixture_with(cx,
                 ScriptedTransport::open(),
                 ScriptedLibrary::open(),
                 3,
                 vec![item_with_files("a", 1, 2)])
}

/// Puts the file of `item` at `index` on disk where the app would have put it.
fn put_on_disk(storage: &StorageConfig, item: &LibraryItem, index: usize) -> PathBuf {
    let path = resolved_file_path(storage, item, &item.files[index]);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"data").unwrap();
    path
}

#[test]
fn the_files_that_are_on_disk_are_listed_with_the_item_they_belong_to() {
    let root = tempfile::tempdir().unwrap();
    let storage = StorageConfig::for_test(root.path().to_path_buf());
    let catalog = vec![item_with_files("a", 1, 2), item_with_files("b", 2, 1)];
    let on_disk = put_on_disk(&storage, &catalog[0], 1);

    let files = files_on_disk(&catalog, &[], &storage);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].item_id, "a");
    assert_eq!(files[0].path, on_disk);
}

#[test]
fn a_file_is_listed_whatever_its_downloaded_flag_says() {
    let root = tempfile::tempdir().unwrap();
    let storage = StorageConfig::for_test(root.path().to_path_buf());
    // Flagged downloaded but gone, and not flagged but there: the flags are
    // only as current as the last verification, the disk is the truth.
    let mut gone = item_with_files("gone", 1, 1);
    gone.files[0].name = "Gone.pdf".into();
    gone.files[0].downloaded = true;
    let mut present = item_with_files("present", 2, 1);
    present.files[0].name = "Present.pdf".into();
    put_on_disk(&storage, &present, 0);

    let files = files_on_disk(&[gone, present], &[], &storage);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].item_id, "present");
}

#[gpui::test]
fn files_that_were_given_the_librarys_tags_are_counted_in_the_activity_panel(cx: &mut TestAppContext)
{
    let fixture = fixture_with_items(cx);

    fixture.controller
           .update(cx, |ctrl, cx| {
               ctrl.report_file_tags(Ok(report(&["/a/one.pdf", "/a/two.pdf"], &[], &[])), cx)
           });

    let items = fixture.activity.read_with(cx, |a, _| a.snapshot().items);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].status, ActivityStatus::Complete));
    assert!(items[0].label.contains('2'), "{}", items[0].label);
}

#[gpui::test]
fn files_whose_tags_were_read_into_the_library_are_counted_separately(cx: &mut TestAppContext) {
    let fixture = fixture_with_items(cx);

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.report_file_tags(Ok(report(&["/a/one.pdf"],
                                                          &["/a/two.pdf",
                                                            "/a/three.pdf",
                                                            "/a/four.pdf"],
                                                          &[])),
                                                cx)
                      });

    let items = fixture.activity.read_with(cx, |a, _| a.snapshot().items);
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|item| item.label.contains('3')));
    assert!(items.iter().any(|item| item.label.contains('1')));
}

#[gpui::test]
fn files_that_could_not_be_written_are_reported_as_an_error(cx: &mut TestAppContext) {
    let fixture = fixture_with_items(cx);

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.report_file_tags(Ok(report(&[], &[], &["/a/one.pdf"])), cx)
                      });

    let items = fixture.activity.read_with(cx, |a, _| a.snapshot().items);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].status, ActivityStatus::Error(_)));
}

#[gpui::test]
fn the_activity_panel_says_nothing_when_every_file_already_matched(cx: &mut TestAppContext) {
    let fixture = fixture_with_items(cx);

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.report_file_tags(Ok(report(&[], &[], &[])), cx)
                      });

    assert_eq!(activity_counts(&fixture, cx), (0, 0));
}

#[gpui::test]
fn the_files_are_not_compared_before_there_is_a_catalog(cx: &mut TestAppContext) {
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               ScriptedLibrary::open(),
                               3,
                               Vec::new());

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.begin_tag_sync(false, cx);
                          assert!(!ctrl.file_tags_reconciled && !ctrl.file_tag_sync_running);
                      });
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui::test]
fn the_files_are_compared_once_per_launch_and_again_when_forced(cx: &mut TestAppContext) {
    let fixture = fixture_with_items(cx);
    let running = |cx: &mut TestAppContext| {
        fixture.controller
               .read_with(cx, |ctrl, _| ctrl.file_tag_sync_running)
    };

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.begin_tag_sync(false, cx);
                          assert!(ctrl.file_tags_reconciled && ctrl.file_tag_sync_running);
                      });
    wait_for(cx, |cx| !running(cx));

    // Asked again the same launch: nothing starts.
    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.begin_tag_sync(false, cx);
                          assert!(!ctrl.file_tag_sync_running);
                      });
    // A reload forces it.
    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.begin_tag_sync(true, cx);
                          assert!(ctrl.file_tag_sync_running);
                      });
    wait_for(cx, |cx| !running(cx));
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui::test]
fn only_one_comparison_runs_at_a_time(cx: &mut TestAppContext) {
    let fixture = fixture_with_items(cx);

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.begin_tag_sync(true, cx);
                          ctrl.file_tag_sync_running = true;
                          // A forced request while one is running is dropped,
                          // not queued.
                          ctrl.begin_tag_sync(true, cx);
                      });
    cx.run_until_parked();
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_tag_added_to_a_file_outside_libri_reaches_the_library() {
    use dtrpg_core::data::catalog_cache::save_catalog_cache;
    use dtrpg_core::data::index::LibraryIndex;
    use dtrpg_core::platform::{FileTag, Platform, PlatformIntegration};
    use dtrpg_core::services::tag_projection::{SyncSettings, reconcile_with_index};

    let storage_root = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let storage = StorageConfig::for_test(storage_root.path().to_path_buf());
    let catalog = vec![item_with_files("a", 1, 1)];
    save_catalog_cache(cache.path(), &catalog).unwrap();
    let path = put_on_disk(&storage, &catalog[0], 0);
    // What Finder does: tag the file, with no one asking Libri.
    Platform.write_tags(&path, &[FileTag::plain("from finder")])
            .unwrap();

    let files = files_on_disk(&catalog, &[], &storage);
    let report =
        reconcile_with_index(&Platform, cache.path(), &files, SyncSettings::default()).unwrap();

    assert_eq!(report.imported, [path]);
    let index = LibraryIndex::open_and_migrate(cache.path()).unwrap();
    assert_eq!(index.tags_for_item("a").unwrap(), ["from finder"]);
}
