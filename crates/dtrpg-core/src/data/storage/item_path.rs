//! Resolves an item file's on-disk path without a UI dependency, so
//! `LibraryApi`/MCP can locate a file headlessly (e.g. for summary
//! generation). Mirrors `dtrpg-ui`'s `resolved_file_path`, minus the
//! collection-membership and iCloud-status concerns that are UI-only.

use std::path::PathBuf;

use super::StorageConfig;
use crate::data::library::{LibraryItem, LibraryItemFile};

/// Resolves the expected on-disk path for `file` within `item`'s entry, using
/// the configured placement rules.
#[must_use]
pub fn resolved_file_path(storage: &StorageConfig, item: &LibraryItem, file: &LibraryItemFile)
                          -> PathBuf {
    let root = storage.resolve_root_for(&item.publisher, &[], file.size_mb);
    let file_name = super::sanitize_path_component(file.name.as_ref());
    StorageConfig::path_for_publisher_in(&root, &item.publisher).join(file_name)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::data::enums::ItemStatus;

    use super::*;

    fn make_item(publisher: &str) -> LibraryItem {
        LibraryItem::new("item-1", "Test Title", publisher, "", "Core", "PDF", 100, 10.0, 2024, 1,
                         ItemStatus::Cloud, "#1C2A44", "Desc.", None)
    }

    #[test]
    fn resolved_file_path_matches_path_for_publisher_join_name() {
        let dir = tempfile::tempdir().unwrap();
        let storage = StorageConfig::for_test(dir.path().to_path_buf());
        let item = make_item("Paizo");
        let file = LibraryItemFile { id:         "f1".into(),
                                     index:      0,
                                     name:       "Book.pdf".into(),
                                     format:     "PDF".into(),
                                     size_mb:    1.0,
                                     downloaded: false,
                                     offloaded:  false, };

        let resolved = resolved_file_path(&storage, &item, &file);

        assert_eq!(resolved, storage.path_for_publisher("Paizo").join("Book.pdf"));
    }
}
