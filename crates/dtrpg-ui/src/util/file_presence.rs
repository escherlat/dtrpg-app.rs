//! Verifies `LibraryItemFile::downloaded` against actual on-disk presence,
//! rather than trusting the flag left behind by a prior download or catalog
//! reconcile. See the `verify-downloaded-status-against-disk` capability.

use std::path::PathBuf;

use crate::data::library::{LibraryItem, LibraryItemFile};
use crate::data::storage::StorageConfig;

/// Resolves the expected on-disk path for `file` within `item`'s entry,
/// matching `dispatch_download`'s destination computation so both the
/// download path and this verification path agree on "where does this file
/// live."
#[must_use]
pub fn resolved_file_path(storage: &StorageConfig, item: &LibraryItem, file: &LibraryItemFile)
                          -> PathBuf {
    storage.path_for_publisher(&item.publisher)
           .join(file.name.as_ref())
}

/// Sets every file's `downloaded` flag to whether it actually exists at its
/// [`resolved_file_path`], in both directions, then recomputes `item.status`.
///
/// Returns `true` if any file's flag or the item's status changed.
pub fn verify_item_downloads(item: &mut LibraryItem, storage: &StorageConfig) -> bool {
    let mut changed = false;
    for i in 0..item.files.len() {
        let path = resolved_file_path(storage, item, &item.files[i]);
        let present = path.exists();
        let file = &mut item.files[i];
        if file.downloaded != present {
            file.downloaded = present;
            changed = true;
        }
    }
    let prior_status = item.status;
    item.recompute_status();
    changed || item.status != prior_status
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::data::enums::ItemStatus;

    /// Per-test isolated root under the system tempdir, matching the
    /// `test_dir` convention used by `catalog_cache`/`cover_cache`/
    /// `collections_cache` tests.
    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dtrpg_file_presence_test_{name}"))
    }

    /// A `StorageConfig` whose root is pinned to an isolated test directory,
    /// entirely in-memory — does not read or write the real `storage.toml`.
    ///
    /// Previously used `StorageConfig::load()` + `set_root_path()`, which
    /// both reads and persists to the developer's actual, shared config
    /// file, silently corrupting real settings every time these tests ran.
    fn storage_at(root: &std::path::Path) -> StorageConfig {
        StorageConfig::for_test(root.to_path_buf())
    }

    fn make_item(id: &str, publisher: &str) -> LibraryItem {
        LibraryItem::new(id,
                         "Test Title",
                         publisher,
                         "",
                         "Core",
                         "PDF",
                         100,
                         10.0,
                         2024,
                         1,
                         ItemStatus::Cloud,
                         "#1C2A44",
                         "Desc.",
                         None)
    }

    fn make_file(id: &str, name: &str, downloaded: bool) -> LibraryItemFile {
        LibraryItemFile { id: id.into(),
                          index: 0,
                          name: name.into(),
                          format: "PDF".into(),
                          size_mb: 1.0,
                          downloaded }
    }

    #[test]
    fn resolved_file_path_matches_path_for_publisher_join_name() {
        let dir = test_dir("resolved-path");
        let storage = storage_at(&dir);
        let it = make_item("verify-1", "Test Publisher");
        let file = make_file("f1", "Book.pdf", false);

        let expected = storage.path_for_publisher(&it.publisher).join("Book.pdf");
        assert_eq!(resolved_file_path(&storage, &it, &file), expected);
    }

    #[test]
    fn verify_item_downloads_marks_present_file_downloaded() {
        let dir = test_dir("present");
        let storage = storage_at(&dir);
        let mut it = make_item("verify-2", "Presence Publisher");
        it.files = vec![make_file("f1", "Book.pdf", false)];

        let dest_dir = storage.path_for_publisher(&it.publisher);
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(dest_dir.join("Book.pdf"), b"pdf").unwrap();

        let changed = verify_item_downloads(&mut it, &storage);

        assert!(changed);
        assert!(it.files[0].downloaded);
        assert_eq!(it.status, ItemStatus::Downloaded);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_item_downloads_marks_missing_file_not_downloaded() {
        let dir = test_dir("missing");
        let storage = storage_at(&dir);
        let mut it = make_item("verify-3", "Presence Publisher");
        it.files = vec![make_file("f1", "Missing.pdf", true)];

        let changed = verify_item_downloads(&mut it, &storage);

        assert!(changed);
        assert!(!it.files[0].downloaded);
        assert_eq!(it.status, ItemStatus::Cloud);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_item_downloads_recomputes_status_for_mixed_presence() {
        let dir = test_dir("mixed");
        let storage = storage_at(&dir);
        let mut it = make_item("verify-4", "Presence Publisher");
        it.files = vec![make_file("f1", "Present.pdf", false),
                        make_file("f2", "Missing.pdf", true),];

        let dest_dir = storage.path_for_publisher(&it.publisher);
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(dest_dir.join("Present.pdf"), b"pdf").unwrap();

        let changed = verify_item_downloads(&mut it, &storage);

        assert!(changed);
        assert!(it.files[0].downloaded);
        assert!(!it.files[1].downloaded);
        assert_eq!(it.status, ItemStatus::Cloud);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_item_downloads_returns_false_when_nothing_changed() {
        let dir = test_dir("unchanged");
        let storage = storage_at(&dir);
        let mut it = make_item("verify-5", "Presence Publisher");
        it.files = vec![make_file("f1", "AlreadyPresent.pdf", true)];
        it.recompute_status();

        let dest_dir = storage.path_for_publisher(&it.publisher);
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(dest_dir.join("AlreadyPresent.pdf"), b"pdf").unwrap();

        let changed = verify_item_downloads(&mut it, &storage);

        assert!(!changed);
        assert!(it.files[0].downloaded);
        assert_eq!(it.status, ItemStatus::Downloaded);

        std::fs::remove_dir_all(&dir).ok();
    }
}
