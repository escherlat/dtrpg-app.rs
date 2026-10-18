//! Projects an item's tags, which the library index owns, onto its files.
//!
//! The index is the source of truth for tags (ADR 0004). Where the platform
//! keeps tags on files, the files are made to match the index, both when an
//! item's tags change and once on launch, in case a tag was edited outside
//! Libri while it was closed.
//!
//! An item with no tags in the index is left alone by the launch pass. A file
//! may carry tags the user set in Finder before ever using Libri's, and
//! clearing them because the index happens to know nothing about them would
//! destroy data. Removing an item's last tag in Libri clears its files
//! through [`project_item_tags`] instead.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::data::index::{IndexError, LibraryIndex};
use crate::platform::PlatformIntegration;

/// A downloaded file and the item it belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaggedFile {
    /// The stable catalog id of the item the file belongs to.
    pub item_id: String,
    /// Where the file is on disk.
    pub path:    PathBuf,
}

/// A file whose tags could not be read or written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionFailure {
    /// The file.
    pub path:  PathBuf,
    /// Why, as the operating system said it.
    pub error: String,
}

/// What a launch pass did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReconcileReport {
    /// How many files had their tags compared.
    pub checked:  usize,
    /// The files whose tags were rewritten to match the index.
    pub restored: Vec<PathBuf>,
    /// The files that could not be compared or rewritten.
    pub failed:   Vec<ProjectionFailure>,
}

/// Returns `tags` lower-cased and sorted, to compare two lists of tags that
/// are the same set however they are spelled or ordered.
fn comparable(tags: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = tags.iter().map(|tag| tag.to_lowercase()).collect();
    tags.sort();
    tags.dedup();
    tags
}

/// Makes each file's tags match the index, and reports what it changed.
///
/// `tags_by_item` maps an item's id to its tags in the index. A file whose
/// item has no tags there is skipped, as is every file on a platform that
/// keeps no tags on files. A file that cannot be read or written is reported
/// in [`ReconcileReport::failed`] and does not stop the others.
#[must_use]
pub fn reconcile_file_tags<P>(platform: &P, tags_by_item: &HashMap<String, Vec<String>>,
                              files: &[TaggedFile])
                              -> ReconcileReport
    where P: PlatformIntegration + ?Sized {
    let mut report = ReconcileReport::default();
    if !platform.capabilities().filesystem_tags {
        return report;
    }
    for file in files {
        let Some(wanted) = tags_by_item.get(&file.item_id)
                                       .filter(|tags| !tags.is_empty())
        else {
            continue;
        };
        report.checked += 1;
        let result = platform.read_tags(&file.path).and_then(|found| {
                                                       if comparable(&found) == comparable(wanted) {
                                                           Ok(false)
                                                       }
                                                       else {
                                                           platform.write_tags(&file.path, wanted)
                                                                   .map(|()| true)
                                                       }
                                                   });
        match result {
            Ok(true) => report.restored.push(file.path.clone()),
            Ok(false) => {}
            Err(error) => report.failed
                                .push(ProjectionFailure { path:  file.path.clone(),
                                                          error: error.to_string(), }),
        }
    }
    report
}

/// Reads every item's tags from the library index under `index_root`, then
/// runs [`reconcile_file_tags`] over `files`.
///
/// # Errors
///
/// Returns [`IndexError`] if the index cannot be opened or read. Nothing is
/// written to any file in that case.
pub fn reconcile_with_index<P>(platform: &P, index_root: &Path, files: &[TaggedFile])
                               -> Result<ReconcileReport, IndexError>
    where P: PlatformIntegration + ?Sized {
    if !platform.capabilities().filesystem_tags {
        return Ok(ReconcileReport::default());
    }
    let tags_by_item = LibraryIndex::open_and_migrate(index_root)?.all_item_tags()?;
    Ok(reconcile_file_tags(platform, &tags_by_item, files))
}

/// Writes the tags the library index holds for `item_id` onto `paths`, for a
/// file that has just been downloaded, and returns the files it could not
/// write. An item with no tags leaves its files alone.
///
/// # Errors
///
/// Returns [`IndexError`] if the index cannot be opened or read.
pub fn project_indexed_tags<P>(platform: &P, index_root: &Path, item_id: &str, paths: &[PathBuf])
                               -> Result<Vec<ProjectionFailure>, IndexError>
    where P: PlatformIntegration + ?Sized {
    if !platform.capabilities().filesystem_tags {
        return Ok(Vec::new());
    }
    let tags = LibraryIndex::open_and_migrate(index_root)?.tags_for_item(item_id)?;
    if tags.is_empty() {
        return Ok(Vec::new());
    }
    Ok(project_item_tags(platform, &tags, paths))
}

/// Makes the tags of each of `paths` exactly `tags`, and returns the files that
/// could not be written. An empty `tags` clears them. Nothing is attempted on a
/// platform that keeps no tags on files.
#[must_use]
pub fn project_item_tags<P>(platform: &P, tags: &[String], paths: &[PathBuf])
                            -> Vec<ProjectionFailure>
    where P: PlatformIntegration + ?Sized {
    if !platform.capabilities().filesystem_tags {
        return Vec::new();
    }
    paths.iter()
         .filter_map(|path| {
             platform.write_tags(path, tags)
                     .err()
                     .map(|error| ProjectionFailure { path:  path.clone(),
                                                      error: error.to_string(), })
         })
         .collect()
}

#[cfg(test)]
mod tests;
