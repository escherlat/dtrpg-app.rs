//! Tests for the library controller, grouped by concern.

#[cfg(test)]
mod advanced_details_tests {
    use std::collections::HashMap;

    use super::super::toggle_bool_flag;

    #[test]
    fn missing_entry_defaults_to_false() {
        let map: HashMap<&str, bool> = HashMap::new();
        assert!(!map.get("entry-a").copied().unwrap_or(false));
    }

    #[test]
    fn toggle_flips_repeatedly_for_same_key() {
        let mut map: HashMap<&str, bool> = HashMap::new();

        assert!(toggle_bool_flag(&mut map, "entry-a"));
        assert!(!toggle_bool_flag(&mut map, "entry-a"));
        assert!(toggle_bool_flag(&mut map, "entry-a"));
    }

    #[test]
    fn toggle_does_not_affect_other_keys() {
        let mut map: HashMap<&str, bool> = HashMap::new();

        toggle_bool_flag(&mut map, "entry-a");

        assert!(map.get("entry-a").copied().unwrap_or(false));
        assert!(!map.get("entry-b").copied().unwrap_or(false));
    }
}

#[cfg(test)]
mod item_row_selection_tests {
    use std::collections::HashMap;

    use super::super::toggle_row_selection;

    #[test]
    fn selecting_a_row_selects_only_that_row() {
        // Regression: two files in a bundle can share the same download id
        // (see `LibraryItem::dedupe_files`), so selection must be tracked by
        // row position, not by id, or selecting one row would also select
        // any other row sharing its id.
        let mut map: HashMap<&str, usize> = HashMap::new();

        toggle_row_selection(&mut map, "entry-a", 0);

        assert_eq!(map.get("entry-a").copied(), Some(0));
    }

    #[test]
    fn selecting_a_different_row_switches_the_selection() {
        let mut map: HashMap<&str, usize> = HashMap::new();
        toggle_row_selection(&mut map, "entry-a", 0);

        toggle_row_selection(&mut map, "entry-a", 1);

        assert_eq!(map.get("entry-a").copied(), Some(1));
    }

    #[test]
    fn selecting_the_same_row_again_deselects_it() {
        let mut map: HashMap<&str, usize> = HashMap::new();
        toggle_row_selection(&mut map, "entry-a", 0);

        toggle_row_selection(&mut map, "entry-a", 0);

        assert_eq!(map.get("entry-a"), None);
    }

    #[test]
    fn selection_does_not_affect_other_entries() {
        let mut map: HashMap<&str, usize> = HashMap::new();

        toggle_row_selection(&mut map, "entry-a", 0);
        toggle_row_selection(&mut map, "entry-b", 0);

        assert_eq!(map.get("entry-a").copied(), Some(0));
        assert_eq!(map.get("entry-b").copied(), Some(0));
    }
}

#[cfg(test)]
mod download_queue_tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use dtrpg_core::data::library::LibraryItemFile;

    use super::super::{dequeue_file, missing_file_indices};

    fn file(downloaded: bool) -> LibraryItemFile {
        LibraryItemFile {
            id: "f".into(),
            index: 0,
            name: "File".into(),
            format: "PDF".into(),
            size_mb: 1.0,
            downloaded,
            offloaded: false,
        }
    }

    #[test]
    fn missing_file_indices_returns_every_index_when_none_downloaded() {
        let files = vec![file(false), file(false), file(false)];

        assert_eq!(missing_file_indices(&files), vec![0, 1, 2]);
    }

    #[test]
    fn missing_file_indices_skips_already_downloaded_files() {
        let files = vec![file(true), file(false), file(true), file(false)];

        assert_eq!(missing_file_indices(&files), vec![1, 3]);
    }

    #[test]
    fn missing_file_indices_is_empty_when_all_downloaded() {
        let files = vec![file(true), file(true)];

        assert!(missing_file_indices(&files).is_empty());
    }

    #[test]
    fn dequeue_file_removes_only_the_matching_entry() {
        let mut queue: VecDeque<(Arc<str>, u32, String)> = VecDeque::from([
            (Arc::from("entry-a"), 0, "File 0".to_string()),
            (Arc::from("entry-a"), 1, "File 1".to_string()),
        ]);

        let removed = dequeue_file(&mut queue, "entry-a", 0);

        assert!(removed);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].1, 1);
    }

    #[test]
    fn dequeue_file_leaves_other_entries_untouched() {
        let mut queue: VecDeque<(Arc<str>, u32, String)> = VecDeque::from([
            (Arc::from("entry-a"), 0, "A".to_string()),
            (Arc::from("entry-b"), 0, "B".to_string()),
        ]);

        dequeue_file(&mut queue, "entry-a", 0);

        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].0.as_ref(), "entry-b");
    }

    #[test]
    fn dequeue_file_returns_false_when_nothing_matches() {
        let mut queue: VecDeque<(Arc<str>, u32, String)> =
            VecDeque::from([(Arc::from("entry-a"), 0, "A".to_string())]);

        let removed = dequeue_file(&mut queue, "entry-a", 5);

        assert!(!removed);
        assert_eq!(queue.len(), 1);
    }
}

#[cfg(test)]
mod disk_space_check_tests {
    use dtrpg_core::data::enums::ItemStatus;
    use dtrpg_core::data::library::{LibraryItem, LibraryItemFile};

    use super::super::{missing_files_size_mb, missing_files_size_mb_for_items};

    fn file(size_mb: f64, downloaded: bool) -> LibraryItemFile {
        LibraryItemFile {
            id: "f".into(),
            index: 0,
            name: "File".into(),
            format: "PDF".into(),
            size_mb,
            downloaded,
            offloaded: false,
        }
    }

    fn item(id: &str, files: Vec<LibraryItemFile>) -> LibraryItem {
        let mut item = LibraryItem::new(
            id,
            "Title",
            "Pub",
            "Line",
            "Core",
            "PDF",
            0,
            0.0,
            2020,
            0,
            ItemStatus::Cloud,
            "#000000",
            "",
            None,
        );
        item.files = files;
        item
    }

    #[test]
    fn missing_files_size_mb_sums_only_not_yet_downloaded_files() {
        let files = vec![file(10.0, false), file(5.0, true), file(2.5, false)];

        assert_eq!(missing_files_size_mb(&files), 12.5);
    }

    #[test]
    fn missing_files_size_mb_is_zero_when_all_downloaded() {
        let files = vec![file(10.0, true), file(5.0, true)];

        assert_eq!(missing_files_size_mb(&files), 0.0);
    }

    #[test]
    fn missing_files_size_mb_for_items_sums_across_matching_items_only() {
        let catalog = vec![
            item("a", vec![file(10.0, false), file(5.0, true)]),
            item("b", vec![file(3.0, false)]),
            item("c", vec![file(100.0, false)]),
        ];

        let total = missing_files_size_mb_for_items(&catalog, ["a", "b"].into_iter());

        assert_eq!(total, 13.0);
    }

    #[test]
    fn missing_files_size_mb_for_items_ignores_unknown_ids() {
        let catalog = vec![item("a", vec![file(10.0, false)])];

        let total = missing_files_size_mb_for_items(&catalog, ["nonexistent"].into_iter());

        assert_eq!(total, 0.0);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod file_info_cache_tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::super::cached_file_info;
    use crate::util::file_info::FileInfo;

    #[test]
    fn undownloaded_file_returns_none_without_caching() {
        let mut cache = HashMap::new();
        let entry_id: Arc<str> = Arc::from("entry-a");
        let path = std::path::Path::new("/does/not/exist.pdf");

        let info = cached_file_info(&mut cache, &entry_id, 0, false, path, "PDF");

        assert_eq!(info, FileInfo::None);
        assert!(cache.is_empty());
    }

    #[test]
    fn downloaded_unsupported_format_caches_none() {
        let mut cache = HashMap::new();
        let entry_id: Arc<str> = Arc::from("entry-a");
        let path = std::path::Path::new("/does/not/exist.txt");

        let info = cached_file_info(&mut cache, &entry_id, 0, true, path, "TXT");

        assert_eq!(info, FileInfo::None);
        assert_eq!(
            cache.get(&(Arc::clone(&entry_id), 0)),
            Some(&FileInfo::None)
        );
    }

    #[test]
    fn second_call_reuses_cached_value_without_re_reading_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.png");
        image::RgbImage::new(5, 6).save(&path).unwrap();
        let mut cache = HashMap::new();
        let entry_id: Arc<str> = Arc::from("entry-a");

        let first = cached_file_info(&mut cache, &entry_id, 0, true, &path, "PNG");
        assert_eq!(first, FileInfo::Dimensions(5, 6));

        // Delete the underlying file: if the second call re-read from disk,
        // extraction would fail and return `FileInfo::None` instead of the
        // cached dimensions.
        std::fs::remove_file(&path).unwrap();

        let second = cached_file_info(&mut cache, &entry_id, 0, true, &path, "PNG");
        assert_eq!(second, FileInfo::Dimensions(5, 6));
    }

    #[test]
    fn different_rows_cache_independently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.png");
        image::RgbImage::new(2, 2).save(&path).unwrap();
        let mut cache = HashMap::new();
        let entry_id: Arc<str> = Arc::from("entry-a");

        cached_file_info(&mut cache, &entry_id, 0, true, &path, "PNG");

        assert_eq!(cache.get(&(Arc::clone(&entry_id), 1)), None);
    }
}

#[cfg(test)]
mod bulk_download_target_tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use dtrpg_core::data::enums::ItemStatus;
    use dtrpg_core::data::library::LibraryItem;
    use dtrpg_core::util::filter::SidebarFilter;

    use super::super::{
        collection_download_targets, filter_download_targets, publisher_download_targets,
    };

    fn item(
        id: &str, title: &str, publisher: &str, order_product_id: u64, product_id: u64,
    ) -> LibraryItem {
        let mut item = LibraryItem::new(
            id,
            title,
            publisher,
            "Line",
            "Core",
            "PDF",
            0,
            0.0,
            2020,
            0,
            ItemStatus::Cloud,
            "#000000",
            "",
            None,
        );
        item.order_product_id = order_product_id;
        item.product_id = product_id;
        item
    }

    #[test]
    fn collection_download_targets_includes_every_member() {
        let catalog = vec![
            item("b1", "Book One", "Pub", 1, 0),
            item("b2", "Book Two", "Pub", 2, 0),
            item("b3", "Not A Member", "Pub", 3, 0),
        ];

        let targets = collection_download_targets(&catalog, &[1, 2]);

        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].0.as_ref(), "b1");
        assert_eq!(targets[1].0.as_ref(), "b2");
    }

    #[test]
    fn collection_download_targets_matches_on_product_id_fallback() {
        let catalog = vec![item("b1", "Book One", "Pub", 0, 42)];

        let targets = collection_download_targets(&catalog, &[42]);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0.as_ref(), "b1");
    }

    #[test]
    fn collection_download_targets_is_empty_for_unknown_collection() {
        let catalog = vec![item("b1", "Book One", "Pub", 1, 0)];

        let targets = collection_download_targets(&catalog, &[999]);

        assert!(targets.is_empty());
    }

    #[test]
    fn publisher_download_targets_includes_every_matching_item() {
        let catalog = vec![
            item("b1", "Book One", "Acme", 1, 0),
            item("b2", "Book Two", "Acme", 2, 0),
            item("b3", "Other Publisher", "Other Co", 3, 0),
        ];

        let targets = publisher_download_targets(&catalog, "Acme");

        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].0.as_ref(), "b1");
        assert_eq!(targets[1].0.as_ref(), "b2");
    }

    #[test]
    fn publisher_download_targets_is_empty_for_unknown_publisher() {
        let catalog = vec![item("b1", "Book One", "Acme", 1, 0)];

        let targets = publisher_download_targets(&catalog, "Nobody");

        assert!(targets.is_empty());
    }

    #[test]
    fn filter_download_targets_all_titles_includes_every_item() {
        let catalog = vec![
            item("b1", "Book One", "Acme", 1, 0),
            item("b2", "Book Two", "Other Co", 2, 0),
        ];

        let targets =
            filter_download_targets(&catalog, &SidebarFilter::AllTitles, &HashSet::new(), 0, 30);

        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn filter_download_targets_publisher_matches_only_that_publisher() {
        let catalog = vec![
            item("b1", "Book One", "Acme", 1, 0),
            item("b2", "Other Publisher", "Other Co", 2, 0),
        ];

        let targets = filter_download_targets(
            &catalog,
            &SidebarFilter::Publisher(Arc::from("Acme")),
            &HashSet::new(),
            0,
            30,
        );

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0.as_ref(), "b1");
    }

    #[test]
    fn filter_download_targets_on_device_matches_only_downloaded_items() {
        let mut downloaded = item("b1", "Downloaded Book", "Acme", 1, 0);
        downloaded.status = ItemStatus::Downloaded;
        let cloud = item("b2", "Cloud Book", "Acme", 2, 0);
        let catalog = vec![downloaded, cloud];

        let targets =
            filter_download_targets(&catalog, &SidebarFilter::OnDevice, &HashSet::new(), 0, 30);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0.as_ref(), "b1");
    }
}
