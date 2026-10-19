//! Keeps the tags on downloaded files in step with the library index.
//!
//! The index is the source of truth for tags, and a file's tags follow it
//! (ADR 0004). A tag edited on a file outside Libri, in Finder for example, is
//! read back into the index, unless the index changed the same file's tags
//! since the two last agreed, in which case the later change wins (ADR 0008).
//!
//! What the two last agreed on is recorded per file (the snapshot). It tells
//! a tag removed from the file from one the file never had, which the current
//! tags alone cannot.

mod decision;

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use decision::{Decision, Sides, comparable, decide};

use crate::data::index::{IndexError, LibraryIndex};
use crate::data::tag_color::TagColor;
use crate::platform::{FileTag, PlatformIntegration};
use crate::util::datetime::now_secs;

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
    /// Why, as the operating system or the index said it.
    pub error: String,
}

/// Which way tags may move between the index and files.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncSettings {
    /// Whether the index's tags may be written to files.
    pub write_to_files:  bool,
    /// Whether the tags on files may be read into the index.
    pub read_from_files: bool,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self { write_to_files:  true,
               read_from_files: true, }
    }
}

/// What a pass over the files did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReconcileReport {
    /// How many files were compared.
    pub checked:  usize,
    /// The files that were given the index's tags.
    pub written:  Vec<PathBuf>,
    /// The files whose tags were read into the index.
    pub imported: Vec<PathBuf>,
    /// The files that could not be compared or changed.
    pub failed:   Vec<ProjectionFailure>,
}

/// Why one file could not be brought in step.
#[derive(Debug, thiserror::Error)]
enum SyncError {
    #[error(transparent)]
    File(#[from] io::Error),
    #[error(transparent)]
    Index(#[from] IndexError),
}

/// The file's tag names, for comparing.
fn names(tags: &[FileTag]) -> Vec<String> {
    tags.iter().map(|tag| tag.name.clone()).collect()
}

/// The index's tags for `item_id`, each with its color, ready to write.
fn index_file_tags(index: &LibraryIndex, item_id: &str) -> Result<Vec<FileTag>, IndexError> {
    let colors = index.tag_colors()?;
    Ok(index.tags_for_item(item_id)?
            .into_iter()
            .map(|name| FileTag { color: colors.get(&name.to_lowercase()).copied(),
                                  name })
            .collect())
}

/// Whether a file's tags are the index's: the same names, and for each tag the
/// same color, none included. A platform that keeps no colors never has the
/// color of a tag the index colored, so such a tag is written again.
fn has_the_tags(on_file: &[FileTag], in_index: &[FileTag]) -> bool {
    comparable(&names(on_file)) == comparable(&names(in_index))
    && in_index.iter().all(|wanted| {
                          on_file.iter().any(|tag| {
                                            tag.name.to_lowercase() == wanted.name.to_lowercase()
                                            && tag.color == wanted.color
                                        })
                      })
}

/// Records that `path` and the index now agree on `tags`.
fn record(index: &LibraryIndex, file: &TaggedFile, tags: &[String]) -> Result<(), IndexError> {
    index.record_file_tag_sync(&file.path.to_string_lossy(),
                               &file.item_id,
                               tags,
                               now_secs())
}

/// Brings one file's tags and the index in step, and says which way tags moved.
fn sync_file<P>(platform: &P, index: &mut LibraryIndex, file: &TaggedFile,
                settings: SyncSettings)
                -> Result<Option<Decision>, SyncError>
    where P: PlatformIntegration + ?Sized {
    let on_file = platform.read_tags(&file.path)?;
    let in_index = index_file_tags(index, &file.item_id)?;
    let snapshot = index.file_tag_snapshot(&file.path.to_string_lossy())?;
    let (on_file_names, in_index_names) = (names(&on_file), names(&in_index));
    let mut sides = Sides { snapshot:         snapshot.as_deref(),
                            file:             &on_file_names,
                            index:            &in_index_names,
                            file_changed_at:  None,
                            index_changed_at: index.tags_changed_at(&file.item_id)?, };
    // The file's change time is only needed to settle a conflict.
    if decide(sides) == Decision::WriteFile || decide(sides) == Decision::ReadFile {
        sides.file_changed_at = platform.tags_changed_at(&file.path)?;
    }
    let decision = decide(sides);
    match decision {
        Decision::InSync => {}
        Decision::Agreed => record(index, file, &on_file_names)?,
        Decision::WriteFile if settings.write_to_files => {
            platform.write_tags(&file.path, &in_index)?;
            record(index, file, &in_index_names)?;
        }
        Decision::ReadFile if settings.read_from_files => {
            let wanted: Vec<(String, Option<TagColor>)> =
                on_file.iter()
                       .map(|tag| (tag.name.clone(), tag.color))
                       .collect();
            let changed_at = sides.file_changed_at.unwrap_or_else(now_secs);
            index.set_item_tags(&file.item_id, &wanted, changed_at)?;
            record(index, file, &on_file_names)?;
        }
        Decision::WriteFile | Decision::ReadFile => return Ok(None),
    }
    Ok(Some(decision))
}

/// Brings the tags of each of `files` and `index` in step, and reports what it
/// did.
///
/// A file that cannot be read or changed is reported in
/// [`ReconcileReport::failed`] and does not stop the others. Nothing is done on
/// a platform that keeps no tags on files.
pub fn reconcile_files<P>(platform: &P, index: &mut LibraryIndex, files: &[TaggedFile],
                          settings: SyncSettings)
                          -> Result<ReconcileReport, IndexError>
    where P: PlatformIntegration + ?Sized {
    let mut report = ReconcileReport::default();
    if !platform.capabilities().filesystem_tags {
        return Ok(report);
    }
    let present: HashSet<String> = files.iter()
                                        .map(|file| file.path.to_string_lossy().into_owned())
                                        .collect();
    index.forget_file_tag_syncs_except(&present)?;
    for file in files {
        report.checked += 1;
        match sync_file(platform, index, file, settings) {
            Ok(Some(Decision::WriteFile)) => report.written.push(file.path.clone()),
            Ok(Some(Decision::ReadFile)) => report.imported.push(file.path.clone()),
            Ok(_) => {}
            Err(error) => report.failed
                                .push(ProjectionFailure { path:  file.path.clone(),
                                                          error: error.to_string(), }),
        }
    }
    Ok(report)
}

/// Opens the library index under `index_root` and runs [`reconcile_files`]
/// over `files`.
///
/// # Errors
///
/// Returns [`IndexError`] if the index cannot be opened or read.
pub fn reconcile_with_index<P>(platform: &P, index_root: &Path, files: &[TaggedFile],
                               settings: SyncSettings)
                               -> Result<ReconcileReport, IndexError>
    where P: PlatformIntegration + ?Sized {
    if !platform.capabilities().filesystem_tags {
        return Ok(ReconcileReport::default());
    }
    let mut index = LibraryIndex::open_and_migrate(index_root)?;
    reconcile_files(platform, &mut index, files, settings)
}

/// Gives `paths`, files of `item_id`, the tags the index holds for the item,
/// and returns the files it could not write. A file that already has them is
/// left alone, so a new download with no tags is not written when the item has
/// none either.
///
/// This is for a change made in Libri, or a file that has just been
/// downloaded, so the index wins and no time is compared. Whatever the files
/// last agreed on with the index is forgotten, so a file that replaced an
/// older one at the same path is not read as having lost its tags.
///
/// # Errors
///
/// Returns [`IndexError`] if the index cannot be opened or read.
pub fn project_indexed_tags<P>(platform: &P, index_root: &Path, item_id: &str,
                               paths: &[PathBuf], settings: SyncSettings)
                               -> Result<Vec<ProjectionFailure>, IndexError>
    where P: PlatformIntegration + ?Sized {
    if !platform.capabilities().filesystem_tags {
        return Ok(Vec::new());
    }
    let index = LibraryIndex::open_and_migrate(index_root)?;
    project_files(platform, &index, item_id, paths, settings)
}

/// [`project_indexed_tags`] on an index that is already open.
///
/// # Errors
///
/// Returns [`IndexError`] if the index cannot be read.
pub(crate) fn project_files<P>(platform: &P, index: &LibraryIndex, item_id: &str,
                               paths: &[PathBuf], settings: SyncSettings)
                               -> Result<Vec<ProjectionFailure>, IndexError>
    where P: PlatformIntegration + ?Sized {
    if !platform.capabilities().filesystem_tags {
        return Ok(Vec::new());
    }
    // A file at a path the library has seen before may be a new file, so what
    // the old one agreed on says nothing about it, whether or not it is
    // written.
    for path in paths {
        index.forget_file_tag_sync(&path.to_string_lossy())?;
    }
    if !settings.write_to_files {
        return Ok(Vec::new());
    }
    let in_index = index_file_tags(index, item_id)?;
    let mut failures = Vec::new();
    for path in paths {
        let file = TaggedFile { item_id: item_id.to_owned(),
                                path:    path.clone(), };
        let result = platform.read_tags(path).and_then(|on_file| {
                                                 if has_the_tags(&on_file, &in_index) {
                                                     Ok(())
                                                 }
                                                 else {
                                                     platform.write_tags(path, &in_index)
                                                 }
                                             });
        let result = result.map_err(SyncError::from)
                           .and_then(|()| Ok(record(index, &file, &names(&in_index))?));
        if let Err(error) = result {
            failures.push(ProjectionFailure { path:  path.clone(),
                                              error: error.to_string(), });
        }
    }
    Ok(failures)
}

#[cfg(test)]
pub(crate) mod test_platform;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_conflicts;
