//! Tests for keeping files' tags in step with the index, against a platform
//! kept in memory and a real in-memory index. Conflicts are in
//! `tests_conflicts.rs`.

#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Mutex;

use super::*;
use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;
use crate::platform::{Capabilities, FileState, PlatformKind};

/// What a fake file holds.
#[derive(Clone, Default)]
struct FakeFile {
    tags:       Vec<FileTag>,
    changed_at: Option<i64>,
}

/// A platform whose files hold their tags in a map.
pub(super) struct InMemory {
    can_tag: bool,
    files:   Mutex<HashMap<PathBuf, FakeFile>>,
    broken:  HashSet<PathBuf>,
    writes:  Mutex<Vec<PathBuf>>,
}

impl InMemory {
    pub(super) fn new() -> Self {
        Self { can_tag: true,
               files:   Mutex::new(HashMap::new()),
               broken:  HashSet::new(),
               writes:  Mutex::new(Vec::new()), }
    }

    pub(super) fn without_tags_on_files() -> Self {
        Self { can_tag: false,
               ..Self::new() }
    }

    pub(super) fn with_broken(mut self, path: &str) -> Self {
        self.broken.insert(PathBuf::from(path));
        self
    }

    /// Edits a file's tags as if outside Libri, at `changed_at`.
    pub(super) fn edit(&self, path: &str, tags: &[FileTag], changed_at: Option<i64>) {
        self.files.lock().unwrap().insert(PathBuf::from(path),
                                          FakeFile { tags: tags.to_vec(),
                                                     changed_at });
    }

    pub(super) fn tags_of(&self, path: &str) -> Vec<FileTag> {
        self.files
            .lock()
            .unwrap()
            .get(Path::new(path))
            .map(|file| file.tags.clone())
            .unwrap_or_default()
    }

    pub(super) fn write_count(&self) -> usize {
        self.writes.lock().unwrap().len()
    }
}

impl PlatformIntegration for InMemory {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Linux
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { filesystem_tags: self.can_tag,
                       ..Capabilities::default() }
    }

    fn file_state(&self, _path: &Path) -> io::Result<FileState> {
        Ok(FileState::Local)
    }

    fn reveal(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }

    fn read_tags(&self, path: &Path) -> io::Result<Vec<FileTag>> {
        if self.broken.contains(path) {
            return Err(io::Error::other("read-only volume"));
        }
        Ok(self.tags_of(&path.to_string_lossy()))
    }

    fn write_tags(&self, path: &Path, tags: &[FileTag]) -> io::Result<()> {
        if self.broken.contains(path) {
            return Err(io::Error::other("read-only volume"));
        }
        self.writes.lock().unwrap().push(path.to_path_buf());
        self.files.lock().unwrap().insert(path.to_path_buf(),
                                          FakeFile { tags:       tags.to_vec(),
                                                     changed_at: Some(now_secs()), });
        Ok(())
    }

    fn tags_changed_at(&self, path: &Path) -> io::Result<Option<i64>> {
        Ok(self.files
               .lock()
               .unwrap()
               .get(path)
               .and_then(|file| file.changed_at))
    }
}

pub(super) fn plain(names: &[&str]) -> Vec<FileTag> {
    names.iter().map(|name| FileTag::plain(*name)).collect()
}

pub(super) fn file(item_id: &str, path: &str) -> TaggedFile {
    TaggedFile { item_id: item_id.to_owned(),
                 path:    PathBuf::from(path), }
}

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
                     ItemStatus::Downloaded,
                     "#1C2A44",
                     "A game.",
                     Some(1_700_000_000))
}

/// An in-memory index holding the items `ids`.
pub(super) fn index_with(ids: &[&str]) -> LibraryIndex {
    let mut index = LibraryIndex::open_in_memory().unwrap();
    let items: Vec<_> = ids.iter().map(|id| item(id)).collect();
    index.replace_all_items(&items).unwrap();
    index
}

pub(super) fn reconcile(platform: &InMemory, index: &mut LibraryIndex, files: &[TaggedFile])
                        -> ReconcileReport {
    reconcile_files(platform, index, files, SyncSettings::default()).unwrap()
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

// ── The first time a file is seen ────────────────────────────────────────────

#[test]
fn a_file_with_no_tags_is_given_its_items_tags_and_colors() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("a", "to run").unwrap();
    index.set_tag_color("solo", Some(TagColor::Red)).unwrap();
    let platform = InMemory::new();

    let report = reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert_eq!(report.written, paths(&["/a/one.pdf"]));
    assert_eq!(platform.tags_of("/a/one.pdf"),
               [FileTag::colored("solo", TagColor::Red),
                FileTag::plain("to run")]);
}

#[test]
fn tags_a_file_already_had_are_read_into_an_index_that_has_none() {
    // The user may have tagged it in Finder before using Libri's tags.
    let mut index = index_with(&["a"]);
    let platform = InMemory::new();
    platform.edit("/a/one.pdf",
                  &[FileTag::colored("from finder", TagColor::Green)],
                  Some(100));

    let report = reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert_eq!(report.imported, paths(&["/a/one.pdf"]));
    assert_eq!(index.tags_for_item("a").unwrap(), ["from finder"]);
    assert_eq!(index.tag_colors().unwrap()["from finder"], TagColor::Green);
    assert_eq!(platform.write_count(), 0);
}

#[test]
fn a_file_and_an_item_with_no_tags_are_left_alone_and_leave_no_record() {
    let mut index = index_with(&["a"]);
    let platform = InMemory::new();

    let report = reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert_eq!(report.checked, 1);
    assert!(report.written.is_empty() && report.imported.is_empty());
    assert_eq!(index.file_tag_snapshot("/a/one.pdf").unwrap(), None);
}

#[test]
fn a_file_that_already_matches_is_recorded_not_rewritten() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "Solo").unwrap();
    let platform = InMemory::new();
    platform.edit("/a/one.pdf", &plain(&["solo"]), Some(100));

    let report = reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert!(report.written.is_empty() && report.imported.is_empty());
    assert_eq!(platform.write_count(), 0);
    assert_eq!(index.file_tag_snapshot("/a/one.pdf").unwrap(),
               Some(vec!["solo".to_owned()]));
}

// ── After they have agreed ───────────────────────────────────────────────────

/// An index with item `a` tagged "solo" and "to run", and one file of it, in
/// step.
fn synced() -> (LibraryIndex, InMemory) {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();
    index.add_tag("a", "to run").unwrap();
    let platform = InMemory::new();
    reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);
    (index, platform)
}

#[test]
fn nothing_changes_when_nothing_changed() {
    let (mut index, platform) = synced();
    let writes = platform.write_count();

    let report = reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert_eq!(report.checked, 1);
    assert!(report.written.is_empty() && report.imported.is_empty() && report.failed.is_empty());
    assert_eq!(platform.write_count(), writes);
}

#[test]
fn a_tag_removed_from_a_file_outside_libri_is_removed_from_the_index() {
    let (mut index, platform) = synced();
    platform.edit("/a/one.pdf", &plain(&["solo"]), Some(now_secs() + 100));

    let report = reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert_eq!(report.imported, paths(&["/a/one.pdf"]));
    assert_eq!(index.tags_for_item("a").unwrap(), ["solo"]);
    assert_eq!(index.list_tags().unwrap(), ["solo"]);
}

#[test]
fn a_tag_added_to_a_file_outside_libri_is_added_to_the_index() {
    let (mut index, platform) = synced();
    platform.edit("/a/one.pdf",
                  &plain(&["solo", "to run", "stray"]),
                  Some(now_secs() + 100));

    reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert_eq!(index.tags_for_item("a").unwrap(),
               ["solo", "stray", "to run"]);
}

#[test]
fn every_tag_removed_from_a_file_outside_libri_clears_the_item() {
    let (mut index, platform) = synced();
    platform.edit("/a/one.pdf", &[], Some(now_secs() + 100));

    reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert!(index.tags_for_item("a").unwrap().is_empty());
}

#[test]
fn a_tag_added_in_libri_is_written_to_the_file() {
    let (mut index, platform) = synced();
    index.add_tag("a", "co-op").unwrap();
    index.set_tag_color("co-op", Some(TagColor::Blue)).unwrap();

    let report = reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert_eq!(report.written, paths(&["/a/one.pdf"]));
    assert_eq!(platform.tags_of("/a/one.pdf"),
               [FileTag::colored("co-op", TagColor::Blue),
                FileTag::plain("solo"),
                FileTag::plain("to run")]);
}

#[test]
fn the_last_tag_removed_in_libri_clears_the_file() {
    let (mut index, platform) = synced();
    index.remove_tag("a", "solo").unwrap();
    index.remove_tag("a", "to run").unwrap();

    reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert!(platform.tags_of("/a/one.pdf").is_empty());
}

#[test]
fn a_file_is_in_step_after_each_pass_so_a_second_pass_does_nothing() {
    let (mut index, platform) = synced();
    platform.edit("/a/one.pdf", &plain(&["solo"]), Some(now_secs() + 100));
    reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);
    let writes = platform.write_count();

    let report = reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert!(report.written.is_empty() && report.imported.is_empty());
    assert_eq!(platform.write_count(), writes);
}

#[test]
fn a_tag_read_from_a_file_takes_the_time_the_file_changed() {
    let (mut index, platform) = synced();
    platform.edit("/a/one.pdf", &plain(&["solo"]), Some(1_234));

    reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert_eq!(index.tags_changed_at("a").unwrap(), Some(1_234));
}

// ── Failures and platforms ───────────────────────────────────────────────────

#[test]
fn a_file_that_cannot_be_read_is_reported_and_the_others_still_are() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();
    let platform = InMemory::new().with_broken("/a/locked.pdf");

    let report = reconcile(&platform,
                           &mut index,
                           &[file("a", "/a/locked.pdf"), file("a", "/a/two.pdf")]);

    assert_eq!(report.failed,
               [ProjectionFailure { path:  PathBuf::from("/a/locked.pdf"),
                                    error: "read-only volume".to_owned(), }]);
    assert_eq!(report.written, paths(&["/a/two.pdf"]));
}

#[test]
fn a_file_whose_item_is_not_in_the_index_is_reported() {
    let mut index = index_with(&["a"]);
    let platform = InMemory::new();
    platform.edit("/m/one.pdf", &plain(&["solo"]), Some(100));

    let report = reconcile(&platform, &mut index, &[file("missing", "/m/one.pdf")]);

    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].path, PathBuf::from("/m/one.pdf"));
}

#[test]
fn nothing_is_attempted_where_the_platform_keeps_no_tags_on_files() {
    let mut index = index_with(&["a"]);
    index.add_tag("a", "solo").unwrap();
    let platform = InMemory::without_tags_on_files();

    let report = reconcile(&platform, &mut index, &[file("a", "/a/one.pdf")]);

    assert_eq!(report, ReconcileReport::default());
    assert_eq!(platform.write_count(), 0);
    assert_eq!(index.file_tag_snapshot("/a/one.pdf").unwrap(), None);
}
