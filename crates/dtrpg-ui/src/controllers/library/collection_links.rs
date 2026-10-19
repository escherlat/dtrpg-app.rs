//! Symlinks that put a downloaded file in the folder of each collection its
//! item belongs to, when "Create collections" is on.
//!
//! They are made when a file finishes downloading, and again when an item that
//! is already downloaded is added to another collection.

use std::path::{Path, PathBuf};

use dtrpg_core::data::collection::CollectionEntry;
use dtrpg_core::data::library::LibraryItem;
use dtrpg_core::data::storage::{StorageConfig, collection_dir};

use super::file_tags::files_on_disk;

/// The storage root a file downloaded to `dest` lives under: `dest` is
/// `{root}/items/{publisher}/{file}`.
pub(super) fn root_of(dest: &Path) -> Option<PathBuf> {
    dest.parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

/// Makes `dest` appear in the folder of the collection called `collection_name`
/// under `root`, as a symlink. A failure is logged and does not stop the
/// caller.
pub(crate) fn link_into_collection(dest: &Path, root: &Path, collection_name: &str) {
    let Some(file_name) = dest.file_name()
    else {
        return;
    };
    let link_dir = collection_dir(root, collection_name);
    if let Err(e) = std::fs::create_dir_all(&link_dir) {
        tracing::warn!(dir = %link_dir.display(), error = %e,
                       "failed to create collections symlink directory");
        return;
    }
    let link = link_dir.join(file_name);
    if let Err(e) = crate::util::symlink::ensure_symlink(dest, &link) {
        tracing::warn!(target = %dest.display(), link = %link.display(), error = %e,
                       "failed to create collections symlink");
    }
}

/// Links each file of `item` that is on disk into the folder of the collection
/// called `collection_name`, and returns how many files it looked at.
///
/// `collections` is what the item belongs to now, which decides where its files
/// are expected to be. Reads the disk, so it must run in the background.
pub(super) fn link_item_into_collection(storage: &StorageConfig, item: &LibraryItem,
                                        collections: &[CollectionEntry], collection_name: &str)
                                        -> usize {
    let files = files_on_disk(std::slice::from_ref(item), collections, storage);
    for file in &files {
        let root = root_of(&file.path).unwrap_or_else(|| storage.root_path());
        link_into_collection(&file.path, &root, collection_name);
    }
    files.len()
}
