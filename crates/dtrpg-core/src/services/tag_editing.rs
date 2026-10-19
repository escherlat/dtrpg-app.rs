//! Changing an item's tags, or a tag's color, from the UI.
//!
//! An edit is made in the library index, which owns tags, and then the new tags
//! are written to the files of every item it affected, where the platform
//! keeps tags on files and writing is turned on (ADR 0008).

use std::path::{Path, PathBuf};

use crate::data::index::{IndexError, LibraryIndex};
use crate::data::tag_color::TagColor;
use crate::data::tag_library::TagLibrary;
use crate::platform::PlatformIntegration;
use crate::services::tag_projection::{ProjectionFailure, SyncSettings, TaggedFile, project_files};

/// One change to the library's tags.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TagEdit {
    /// Give the item the tag, creating the tag if it is new.
    Add { item_id: String, tag: String },
    /// Take the tag off the item.
    Remove { item_id: String, tag: String },
    /// Set the tag's color on every item, or clear it with `None`.
    SetColor {
        tag:   String,
        color: Option<TagColor>,
    },
}

/// What an edit did. The tags as they now are are read from the index, not
/// carried here, so that two edits that overlap cannot show each other's result
/// out of date.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagEditOutcome {
    /// Whether the edit changed anything: adding a tag the item has, or taking
    /// off one it does not, does not.
    pub changed:  bool,
    /// The files that could not be given the new tags. The edit stands.
    pub failures: Vec<ProjectionFailure>,
}

/// Makes `edit` in the library index under `index_root`, and writes the result
/// to the files in `files` that belong to an item the edit affected.
///
/// # Errors
///
/// Returns [`IndexError::InvalidTag`] if a tag cannot be stored,
/// [`IndexError::NoSuchItem`] if the item is not in the library index, and
/// [`IndexError::Sqlite`] if the index cannot be read or written. Nothing is
/// changed on error.
pub fn apply_tag_edit<P>(platform: &P, index_root: &Path, edit: &TagEdit, files: &[TaggedFile],
                         settings: SyncSettings)
                         -> Result<TagEditOutcome, IndexError>
    where P: PlatformIntegration + ?Sized {
    let mut index = LibraryIndex::open_and_migrate(index_root)?;
    let (changed, affected) = match edit {
        TagEdit::Add { item_id, tag } => (index.add_tag(item_id, tag)?, vec![item_id.clone()]),
        TagEdit::Remove { item_id, tag } => {
            (index.remove_tag(item_id, tag)?, vec![item_id.clone()])
        }
        TagEdit::SetColor { tag, color } => {
            let affected = TagLibrary::load(&index)?.items_with(tag)
                                                    .iter()
                                                    .map(ToString::to_string)
                                                    .collect();
            (index.set_tag_color(tag, *color)?, affected)
        }
    };
    let mut failures = Vec::new();
    if changed {
        for item_id in &affected {
            let paths: Vec<PathBuf> = files.iter()
                                           .filter(|file| &file.item_id == item_id)
                                           .map(|file| file.path.clone())
                                           .collect();
            failures.extend(project_files(platform, &index, item_id, &paths, settings)?);
        }
    }
    Ok(TagEditOutcome { changed, failures })
}

#[cfg(test)]
mod tests;
