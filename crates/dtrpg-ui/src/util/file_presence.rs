//! Verifies `LibraryItemFile::downloaded` against actual on-disk presence,
//! rather than trusting the flag left behind by a prior download or catalog
//! reconcile. See the `verify-downloaded-status-against-disk` capability.

use std::path::PathBuf;
use std::sync::Arc;

use crate::util::icloud_status;
use dtrpg_core::data::library::{LibraryItem, LibraryItemFile};
use dtrpg_core::data::storage::StorageConfig;

/// Resolves the expected on-disk path for `file` within `item`'s entry,
/// matching `dispatch_download`'s destination computation so both the
/// download path and this verification path agree on "where does this file
/// live."
#[must_use]
pub fn resolved_file_path(
    storage: &StorageConfig, item: &LibraryItem, file: &LibraryItemFile,
) -> PathBuf {
    storage
        .path_for_publisher(&item.publisher)
        .join(file.name.as_ref())
}

/// A file whose on-disk presence differs from its `downloaded` flag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresenceChange {
    /// The file's display name, which is also its on-disk name.
    pub file_name: Arc<str>,
    /// Whether the file is actually on disk.
    pub present: bool,
    /// Whether a cloud provider has evicted the file's local bytes.
    pub offloaded: bool,
}

/// What a disk check found for one catalog entry, ready to be applied to the
/// live catalog by [`apply_presence_changes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemVerification {
    /// The catalog entry's id.
    pub item_id: Arc<str>,
    /// The files whose flags need to change.
    pub changes: Vec<PresenceChange>,
}

/// Checks `item`'s files against disk and reports what is out of date, or
/// `None` if the item is already consistent: every flag matches the disk and
/// `status` matches the flags.
///
/// This only reads. Reporting just the differences, rather than handing back
/// a corrected copy of the item, lets the result be applied to a catalog that
/// has moved on since the check started (for example a download that finished
/// in the meantime) without overwriting that newer state.
#[must_use]
pub fn find_presence_changes(
    item: &LibraryItem, storage: &StorageConfig,
) -> Option<ItemVerification> {
    let changes: Vec<PresenceChange> = item
        .files
        .iter()
        .filter_map(|file| {
            let path = resolved_file_path(storage, item, file);
            let present = path.exists();
            let offloaded = present && icloud_status::is_offloaded(&path);
            (file.downloaded != present || file.offloaded != offloaded).then(|| PresenceChange {
                file_name: Arc::clone(&file.name),
                present,
                offloaded,
            })
        })
        .collect();

    if changes.is_empty() && item.computed_status() == item.status {
        return None;
    }
    Some(ItemVerification {
        item_id: Arc::clone(&item.id),
        changes,
    })
}

/// Applies `changes` to `item`'s files, matched by file name, then recomputes
/// `item.status` from the flags.
///
/// Returns `true` if any file's flag or the item's status changed.
pub fn apply_presence_changes(item: &mut LibraryItem, changes: &[PresenceChange]) -> bool {
    let mut changed = false;
    for change in changes {
        for file in item.files.iter_mut().filter(|f| f.name == change.file_name) {
            if file.downloaded != change.present {
                file.downloaded = change.present;
                changed = true;
            }
            if file.offloaded != change.offloaded {
                file.offloaded = change.offloaded;
                changed = true;
            }
        }
    }
    let prior_status = item.status;
    item.recompute_status();
    changed || item.status != prior_status
}

/// Sets every file's `downloaded` flag to whether it actually exists at its
/// [`resolved_file_path`], in both directions, then recomputes `item.status`.
///
/// Returns `true` if any file's flag or the item's status changed.
pub fn verify_item_downloads(item: &mut LibraryItem, storage: &StorageConfig) -> bool {
    match find_presence_changes(item, storage) {
        Some(verification) => apply_presence_changes(item, &verification.changes),
        None => false,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use dtrpg_core::data::enums::ItemStatus;

    use super::*;

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
        LibraryItem::new(
            id,
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
            None,
        )
    }

    fn make_file(id: &str, name: &str, downloaded: bool) -> LibraryItemFile {
        LibraryItemFile {
            id: id.into(),
            index: 0,
            name: name.into(),
            format: "PDF".into(),
            size_mb: 1.0,
            downloaded,
            offloaded: false,
        }
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
        it.files = vec![
            make_file("f1", "Present.pdf", false),
            make_file("f2", "Missing.pdf", true),
        ];

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

    #[test]
    fn find_presence_changes_reports_only_files_that_differ() {
        let dir = test_dir("find-differ");
        let storage = storage_at(&dir);
        let mut it = make_item("verify-6", "Presence Publisher");
        it.files = vec![
            make_file("f1", "Gone.pdf", true),
            make_file("f2", "Here.pdf", false),
            make_file("f3", "Unchanged.pdf", false),
        ];
        let dest_dir = storage.path_for_publisher(&it.publisher);
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(dest_dir.join("Here.pdf"), b"pdf").unwrap();

        let verification = find_presence_changes(&it, &storage).unwrap();

        assert_eq!(verification.item_id, it.id);
        assert_eq!(
            verification.changes,
            vec![
                PresenceChange {
                    file_name: "Gone.pdf".into(),
                    present: false,
                    offloaded: false,
                },
                PresenceChange {
                    file_name: "Here.pdf".into(),
                    present: true,
                    offloaded: false,
                },
            ]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_presence_changes_reports_a_stale_status_with_no_flag_changes() {
        let dir = test_dir("find-stale-status");
        let storage = storage_at(&dir);
        let mut it = make_item("verify-7", "Presence Publisher");
        it.files = vec![make_file("f1", "Present.pdf", true)];
        // `make_item` starts as `Cloud`, but the flags say `Downloaded`.
        let dest_dir = storage.path_for_publisher(&it.publisher);
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(dest_dir.join("Present.pdf"), b"pdf").unwrap();

        let verification = find_presence_changes(&it, &storage).unwrap();
        assert!(verification.changes.is_empty());
        assert!(apply_presence_changes(&mut it, &verification.changes));
        assert_eq!(it.status, ItemStatus::Downloaded);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_presence_changes_is_none_when_everything_is_consistent() {
        let dir = test_dir("find-consistent");
        let storage = storage_at(&dir);
        let mut it = make_item("verify-8", "Presence Publisher");
        it.files = vec![make_file("f1", "NotDownloaded.pdf", false)];

        assert_eq!(find_presence_changes(&it, &storage), None);
    }

    /// A download that finishes after a disk check has read the disk must not
    /// be undone when that check's results are applied.
    #[test]
    fn applying_changes_keeps_a_download_that_finished_after_the_check_started() {
        let dir = test_dir("race");
        let storage = storage_at(&dir);
        let mut snapshot = make_item("verify-9", "Presence Publisher");
        snapshot.files = vec![
            make_file("f1", "Finishing.pdf", false),
            make_file("f2", "AlreadyThere.pdf", false),
        ];
        let dest_dir = storage.path_for_publisher(&snapshot.publisher);
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(dest_dir.join("AlreadyThere.pdf"), b"pdf").unwrap();
        // The check reads the disk: "Finishing.pdf" is not there yet.
        let verification = find_presence_changes(&snapshot, &storage).unwrap();

        // Meanwhile the live catalog's copy finishes downloading it.
        let mut live = snapshot.clone();
        live.files[0].downloaded = true;
        live.recompute_status();

        apply_presence_changes(&mut live, &verification.changes);

        assert!(
            live.files[0].downloaded,
            "the finished download was reverted"
        );
        assert!(live.files[1].downloaded);
        assert_eq!(live.status, ItemStatus::Downloaded);

        std::fs::remove_dir_all(&dir).ok();
    }
}
