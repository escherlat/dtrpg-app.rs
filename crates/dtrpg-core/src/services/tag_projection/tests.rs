//! Tests for projecting tags onto files, against a platform kept in memory.

#![allow(clippy::unwrap_used)]

use std::collections::HashSet;
use std::io;
use std::path::Path;
use std::sync::Mutex;

use super::*;
use crate::platform::{Capabilities, PlatformKind};

/// A platform whose files hold their tags in a map.
struct InMemory {
    can_tag: bool,
    files:   Mutex<HashMap<PathBuf, Vec<String>>>,
    broken:  HashSet<PathBuf>,
    writes:  Mutex<Vec<PathBuf>>,
}

impl InMemory {
    fn new(can_tag: bool) -> Self {
        Self { can_tag,
               files: Mutex::new(HashMap::new()),
               broken: HashSet::new(),
               writes: Mutex::new(Vec::new()) }
    }

    fn with_file(self, path: &str, tags: &[&str]) -> Self {
        self.files
            .lock()
            .unwrap()
            .insert(PathBuf::from(path), strings(tags));
        self
    }

    fn with_broken(mut self, path: &str) -> Self {
        self.broken.insert(PathBuf::from(path));
        self
    }

    fn tags_of(&self, path: &str) -> Vec<String> {
        self.files
            .lock()
            .unwrap()
            .get(Path::new(path))
            .cloned()
            .unwrap_or_default()
    }

    fn write_count(&self) -> usize {
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

    fn file_state(&self, _path: &Path) -> io::Result<crate::platform::FileState> {
        Ok(crate::platform::FileState::Local)
    }

    fn reveal(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }

    fn read_tags(&self, path: &Path) -> io::Result<Vec<String>> {
        if self.broken.contains(path) {
            return Err(io::Error::other("read-only volume"));
        }
        Ok(self.files
               .lock()
               .unwrap()
               .get(path)
               .cloned()
               .unwrap_or_default())
    }

    fn write_tags(&self, path: &Path, tags: &[String]) -> io::Result<()> {
        if self.broken.contains(path) {
            return Err(io::Error::other("read-only volume"));
        }
        self.writes.lock().unwrap().push(path.to_path_buf());
        self.files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), tags.to_vec());
        Ok(())
    }
}

fn strings(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|tag| (*tag).to_owned()).collect()
}

fn file(item_id: &str, path: &str) -> TaggedFile {
    TaggedFile { item_id: item_id.to_owned(),
                 path:    PathBuf::from(path), }
}

fn index(entries: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
    entries.iter()
           .map(|(id, tags)| ((*id).to_owned(), strings(tags)))
           .collect()
}

#[test]
fn a_tag_removed_outside_libri_is_restored() {
    let platform = InMemory::new(true).with_file("/a/one.pdf", &["solo"]);

    let report = reconcile_file_tags(&platform,
                                     &index(&[("a", &["solo", "to run"])]),
                                     &[file("a", "/a/one.pdf")]);

    assert_eq!(report.restored, [PathBuf::from("/a/one.pdf")]);
    assert_eq!(platform.tags_of("/a/one.pdf"), ["solo", "to run"]);
}

#[test]
fn a_tag_added_outside_libri_is_removed() {
    let platform = InMemory::new(true).with_file("/a/one.pdf", &["solo", "stray"]);

    let report = reconcile_file_tags(&platform,
                                     &index(&[("a", &["solo"])]),
                                     &[file("a", "/a/one.pdf")]);

    assert_eq!(report.restored.len(), 1);
    assert_eq!(platform.tags_of("/a/one.pdf"), ["solo"]);
}

#[test]
fn a_file_with_no_tags_gets_its_items_tags() {
    let platform = InMemory::new(true);

    let report = reconcile_file_tags(&platform,
                                     &index(&[("a", &["solo"])]),
                                     &[file("a", "/a/one.pdf")]);

    assert_eq!(report.restored.len(), 1);
    assert_eq!(platform.tags_of("/a/one.pdf"), ["solo"]);
}

#[test]
fn a_file_that_already_matches_is_not_rewritten() {
    let platform = InMemory::new(true).with_file("/a/one.pdf", &["To Run", "solo"]);

    // Same tags, different order and case.
    let report = reconcile_file_tags(&platform,
                                     &index(&[("a", &["solo", "to run"])]),
                                     &[file("a", "/a/one.pdf")]);

    assert_eq!(report.checked, 1);
    assert!(report.restored.is_empty());
    assert_eq!(platform.write_count(), 0);
}

#[test]
fn a_file_of_an_item_with_no_tags_in_the_index_is_left_alone() {
    // The user may have tagged it in Finder before using Libri's tags.
    let platform = InMemory::new(true).with_file("/a/one.pdf", &["from finder"]);

    let report = reconcile_file_tags(&platform, &index(&[]), &[file("a", "/a/one.pdf")]);

    assert_eq!(report.checked, 0);
    assert_eq!(platform.tags_of("/a/one.pdf"), ["from finder"]);
}

#[test]
fn an_item_with_an_empty_tag_list_is_treated_as_having_no_tags() {
    let platform = InMemory::new(true).with_file("/a/one.pdf", &["from finder"]);

    let report = reconcile_file_tags(&platform, &index(&[("a", &[])]), &[file("a", "/a/one.pdf")]);

    assert_eq!(report.checked, 0);
    assert_eq!(platform.tags_of("/a/one.pdf"), ["from finder"]);
}

#[test]
fn every_file_of_an_item_is_reconciled() {
    let platform = InMemory::new(true);

    let report = reconcile_file_tags(&platform,
                                     &index(&[("a", &["solo"])]),
                                     &[file("a", "/a/one.pdf"), file("a", "/a/two.pdf")]);

    assert_eq!(report.restored.len(), 2);
    assert_eq!(platform.tags_of("/a/two.pdf"), ["solo"]);
}

#[test]
fn a_file_that_cannot_be_written_is_reported_and_the_others_still_are() {
    let platform = InMemory::new(true).with_broken("/a/locked.pdf");

    let report = reconcile_file_tags(&platform,
                                     &index(&[("a", &["solo"])]),
                                     &[file("a", "/a/locked.pdf"), file("a", "/a/two.pdf")]);

    assert_eq!(report.failed,
               [ProjectionFailure { path:  PathBuf::from("/a/locked.pdf"),
                                    error: "read-only volume".to_owned(), }]);
    assert_eq!(report.restored, [PathBuf::from("/a/two.pdf")]);
}

#[test]
fn nothing_is_attempted_where_the_platform_keeps_no_tags_on_files() {
    let platform = InMemory::new(false);

    let report = reconcile_file_tags(&platform,
                                     &index(&[("a", &["solo"])]),
                                     &[file("a", "/a/one.pdf")]);

    assert_eq!(report, ReconcileReport::default());
    assert_eq!(platform.write_count(), 0);
}

#[test]
fn projecting_an_items_tags_writes_every_file() {
    let platform = InMemory::new(true).with_file("/a/one.pdf", &["old"]);

    let failures = project_item_tags(&platform,
                                     &strings(&["solo"]),
                                     &[PathBuf::from("/a/one.pdf"), PathBuf::from("/a/two.pdf")]);

    assert!(failures.is_empty());
    assert_eq!(platform.tags_of("/a/one.pdf"), ["solo"]);
    assert_eq!(platform.tags_of("/a/two.pdf"), ["solo"]);
}

#[test]
fn projecting_no_tags_clears_the_files() {
    let platform = InMemory::new(true).with_file("/a/one.pdf", &["solo"]);

    let failures = project_item_tags(&platform, &[], &[PathBuf::from("/a/one.pdf")]);

    assert!(failures.is_empty());
    assert!(platform.tags_of("/a/one.pdf").is_empty());
}

#[test]
fn projecting_reports_the_files_it_could_not_write() {
    let platform = InMemory::new(true).with_broken("/a/locked.pdf");

    let failures = project_item_tags(&platform,
                                     &strings(&["solo"]),
                                     &[PathBuf::from("/a/locked.pdf"),
                                       PathBuf::from("/a/two.pdf")]);

    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].path, PathBuf::from("/a/locked.pdf"));
    assert_eq!(platform.tags_of("/a/two.pdf"), ["solo"]);
}

#[test]
fn projecting_does_nothing_where_the_platform_keeps_no_tags_on_files() {
    let platform = InMemory::new(false);

    let failures = project_item_tags(&platform,
                                     &strings(&["solo"]),
                                     &[PathBuf::from("/a/one.pdf")]);

    assert!(failures.is_empty());
    assert_eq!(platform.write_count(), 0);
}
