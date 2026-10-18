//! Tests for reconciling the tags on downloaded files with the library index.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use dtrpg_core::data::activity::ActivityStatus;
use dtrpg_core::services::tag_projection::{ProjectionFailure, ReconcileReport};
use gpui::TestAppContext;

use super::{file_tags::downloaded_files, tests_support::*, *};
use crate::util::file_presence::resolved_file_path;

fn downloaded(mut item: LibraryItem, files: &[usize]) -> LibraryItem {
    for index in files {
        item.files[*index].downloaded = true;
    }
    item
}

fn report(restored: &[&str], failed: &[&str]) -> ReconcileReport {
    ReconcileReport { checked:  restored.len() + failed.len(),
                      restored: restored.iter().map(PathBuf::from).collect(),
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

#[test]
fn only_downloaded_files_are_listed_with_the_item_they_belong_to() {
    let root = tempfile::tempdir().unwrap();
    let storage = StorageConfig::for_test(root.path().to_path_buf());
    let catalog = vec![downloaded(item_with_files("a", 1, 2), &[1]),
                       item_with_files("b", 2, 1)];

    let files = downloaded_files(&catalog, &[], &storage);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].item_id, "a");
    assert_eq!(files[0].path,
               resolved_file_path(&storage, &catalog[0], &catalog[0].files[1]));
}

#[gpui::test]
fn files_that_were_restored_are_counted_in_the_activity_panel(cx: &mut TestAppContext) {
    let fixture = fixture_with_items(cx);

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.report_file_tags(Ok(report(&["/a/one.pdf", "/a/two.pdf"], &[])), cx)
                      });

    let items = fixture.activity.read_with(cx, |a, _| a.snapshot().items);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].status, ActivityStatus::Complete));
    assert!(items[0].label.contains('2'), "{}", items[0].label);
}

#[gpui::test]
fn files_that_could_not_be_written_are_reported_as_an_error(cx: &mut TestAppContext) {
    let fixture = fixture_with_items(cx);

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.report_file_tags(Ok(report(&[], &["/a/one.pdf"])), cx)
                      });

    let items = fixture.activity.read_with(cx, |a, _| a.snapshot().items);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].status, ActivityStatus::Error(_)));
}

#[gpui::test]
fn the_activity_panel_says_nothing_when_every_file_already_matched(cx: &mut TestAppContext) {
    let fixture = fixture_with_items(cx);

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.report_file_tags(Ok(report(&[], &[])), cx)
                      });

    assert_eq!(activity_counts(&fixture, cx), (0, 0));
}

#[gpui::test]
fn nothing_is_reconciled_unless_a_full_load_asked_for_it(cx: &mut TestAppContext) {
    let fixture = fixture_with_items(cx);

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.reconcile_file_tags_if_requested(cx);
                          assert!(!ctrl.file_tags_reconciled);
                      });
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[gpui::test]
fn a_request_is_consumed_and_marks_the_launch_pass_as_started(cx: &mut TestAppContext) {
    let fixture = fixture_with_items(cx);

    fixture.controller.update(cx, |ctrl, cx| {
                          ctrl.request_file_tag_reconcile();
                          ctrl.reconcile_file_tags_if_requested(cx);
                          assert!(ctrl.file_tags_reconciled);
                          assert!(!ctrl.reconcile_tags_after_verify);

                          // A later load asks again; the request is still
                          // consumed.
                          ctrl.request_file_tag_reconcile();
                          ctrl.reconcile_file_tags_if_requested(cx);
                          assert!(!ctrl.reconcile_tags_after_verify);
                      });
    cx.run_until_parked();
}
