//! Tests for the library controller, grouped by concern.

#[cfg(test)]
mod item_check_tests {
    use std::time::{Duration, SystemTime};

    use dtrpg_core::data::enums::ItemStatus;
    use dtrpg_core::data::library::{LibraryItem, LibraryItemFile};
    use dtrpg_core::services::{LibraryServiceError, LibraryServiceErrorKind};

    use super::super::{apply_check_result, item_check_due};

    fn item(id: &str, title: &str) -> LibraryItem {
        LibraryItem::new(id,
                         title,
                         "Publisher",
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
                         None)
    }

    fn file(id: &str, downloaded: bool) -> LibraryItemFile {
        LibraryItemFile { id: id.into(),
                          index: 0,
                          name: format!("{id}.pdf").into(),
                          format: "PDF".into(),
                          size_mb: 1.0,
                          downloaded,
                          offloaded: false }
    }

    #[test]
    fn check_is_due_when_never_checked() {
        assert!(item_check_due(None));
    }

    #[test]
    fn check_is_suppressed_within_cooldown() {
        assert!(!item_check_due(Some(SystemTime::now())));
    }

    #[test]
    fn check_is_due_once_cooldown_elapses() {
        let long_ago = SystemTime::now() - Duration::from_secs(600);

        assert!(item_check_due(Some(long_ago)));
    }

    #[test]
    fn ok_result_refreshes_fields_and_marks_available() {
        let mut existing = item("b1", "Old Title");
        existing.is_available = false;
        let fresh = item("b1", "New Title");
        let now = SystemTime::now();

        apply_check_result(&mut existing, Ok(fresh), now);

        assert_eq!(existing.title.as_ref(), "New Title");
        assert!(existing.is_available);
        assert_eq!(existing.availability_last_checked, Some(now));
    }

    #[test]
    fn ok_result_preserves_identity_and_membership_ids_even_if_the_fresh_response_differs() {
        // Regression: a single-item availability re-check must never let the
        // fresh response clobber `numeric_id`/`order_product_id`/`product_id` —
        // those drive `collection_member_id` lookups, and a mismatched or
        // zeroed value from the single-item endpoint silently broke collection
        // membership display for the checked item.
        let mut existing = item("b1", "Title");
        existing.numeric_id = 111;
        existing.order_product_id = 222;
        existing.product_id = 333;
        let mut fresh = item("b1", "Title");
        fresh.numeric_id = 999;
        fresh.order_product_id = 0;
        fresh.product_id = 0;

        apply_check_result(&mut existing, Ok(fresh), SystemTime::now());

        assert_eq!(existing.numeric_id, 111);
        assert_eq!(existing.order_product_id, 222);
        assert_eq!(existing.product_id, 333);
    }

    #[test]
    fn not_found_result_marks_unavailable_without_removing() {
        let mut existing = item("b1", "Still Here");
        let now = SystemTime::now();

        apply_check_result(&mut existing,
                           Err(LibraryServiceError::new(LibraryServiceErrorKind::NotFound,
                                                        "not found")),
                           now);

        assert_eq!(existing.title.as_ref(), "Still Here");
        assert!(!existing.is_available);
        assert_eq!(existing.availability_last_checked, Some(now));
    }

    #[test]
    fn transient_error_leaves_item_and_flag_unchanged() {
        let mut existing = item("b1", "Unchanged");
        existing.is_available = true;
        let now = SystemTime::now();

        apply_check_result(&mut existing,
                           Err(LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                                        "network error")),
                           now);

        assert!(existing.is_available);
        assert_eq!(existing.availability_last_checked, None);
    }

    #[test]
    fn ok_result_preserves_downloaded_files_and_keeps_status_downloaded() {
        // Regression: a single-item availability re-check must not let the
        // fresh server response (which never reports `downloaded` — that's a
        // local-only fact) wipe files already marked downloaded on disk. See
        // `verify-downloaded-status-against-disk`, whose on-demand disk
        // verification races this same check on item selection.
        let mut existing = item("b1", "Title");
        existing.files = vec![file("f1", true), file("f2", true)];
        existing.recompute_status();
        assert_eq!(existing.status, ItemStatus::Downloaded);

        let mut fresh = item("b1", "Title");
        fresh.files = vec![file("f1", false), file("f2", false)];

        apply_check_result(&mut existing, Ok(fresh), SystemTime::now());

        assert!(existing.files.iter().all(|f| f.downloaded));
        assert_eq!(existing.status, ItemStatus::Downloaded);
    }

    #[test]
    fn ok_result_defaults_a_new_file_id_to_not_downloaded() {
        let mut existing = item("b1", "Title");
        existing.files = vec![file("f1", true)];
        existing.recompute_status();

        let mut fresh = item("b1", "Title");
        fresh.files = vec![file("f1", false), file("f2", false)];

        apply_check_result(&mut existing, Ok(fresh), SystemTime::now());

        assert!(existing.files[0].downloaded);
        assert!(!existing.files[1].downloaded);
        assert_eq!(existing.status, ItemStatus::Cloud);
    }

    #[test]
    fn ok_result_preserves_partial_download_and_keeps_status_cloud() {
        let mut existing = item("b1", "Title");
        existing.files = vec![file("f1", true), file("f2", false)];
        existing.recompute_status();
        assert_eq!(existing.status, ItemStatus::Cloud);

        let mut fresh = item("b1", "Title");
        fresh.files = vec![file("f1", false), file("f2", false)];

        apply_check_result(&mut existing, Ok(fresh), SystemTime::now());

        assert!(existing.files[0].downloaded);
        assert!(!existing.files[1].downloaded);
        assert_eq!(existing.status, ItemStatus::Cloud);
    }
}

#[cfg(test)]
mod check_queue_tests {
    use std::collections::{HashSet, VecDeque};
    use std::sync::Arc;

    use super::super::should_enqueue_check;

    #[test]
    fn id_not_queued_or_in_flight_should_be_enqueued() {
        let queue: VecDeque<Arc<str>> = VecDeque::new();
        let checking: HashSet<Arc<str>> = HashSet::new();

        assert!(should_enqueue_check(&queue, &checking, &Arc::from("b1")));
    }

    #[test]
    fn id_already_queued_is_not_re_enqueued() {
        let queue: VecDeque<Arc<str>> = VecDeque::from([Arc::from("b1")]);
        let checking: HashSet<Arc<str>> = HashSet::new();

        assert!(!should_enqueue_check(&queue, &checking, &Arc::from("b1")));
    }

    #[test]
    fn id_already_in_flight_is_not_re_enqueued() {
        let queue: VecDeque<Arc<str>> = VecDeque::new();
        let checking: HashSet<Arc<str>> = HashSet::from([Arc::from("b1")]);

        assert!(!should_enqueue_check(&queue, &checking, &Arc::from("b1")));
    }
}

#[cfg(test)]
mod check_batch_tests {
    use std::sync::Arc;
    use std::time::{Duration, SystemTime};

    use dtrpg_core::data::enums::ItemStatus;
    use dtrpg_core::data::library::LibraryItem;

    use super::super::{check_batch_cooldown_active, select_check_batch};

    fn item(id: &str) -> LibraryItem {
        LibraryItem::new(id,
                         "Title",
                         "Publisher",
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
                         None)
    }

    #[test]
    fn batch_is_suppressed_when_a_recent_batch_was_already_enqueued() {
        let now = 10_000;
        assert!(check_batch_cooldown_active(Some(now - 10), now));
    }

    #[test]
    fn batch_proceeds_once_cooldown_elapses() {
        let now = 10_000;
        assert!(!check_batch_cooldown_active(Some(now - 901), now));
    }

    #[test]
    fn batch_proceeds_when_no_prior_batch_recorded() {
        assert!(!check_batch_cooldown_active(None, 10_000));
    }

    #[test]
    fn batch_selection_prefers_oldest_checked_items_first() {
        let mut never_checked = item("b1");
        never_checked.availability_last_checked = None;
        let mut checked_long_ago = item("b2");
        checked_long_ago.availability_last_checked =
            Some(SystemTime::now() - Duration::from_secs(10_000));
        let mut checked_recently_within_cooldown = item("b3");
        checked_recently_within_cooldown.availability_last_checked = Some(SystemTime::now());

        let catalog = [checked_recently_within_cooldown,
                       checked_long_ago.clone(),
                       never_checked.clone()];
        let batch = select_check_batch(&catalog, 10);

        // The recently-checked item is within its cooldown and not due, so it's
        // excluded; the never-checked item sorts before the long-ago-checked
        // one.
        assert_eq!(batch,
                   vec![Arc::clone(&never_checked.id),
                        Arc::clone(&checked_long_ago.id)]);
    }

    #[test]
    fn batch_selection_is_bounded_by_limit() {
        let catalog: Vec<LibraryItem> = (0..10).map(|i| item(&format!("b{i}"))).collect();

        let batch = select_check_batch(&catalog, 3);

        assert_eq!(batch.len(), 3);
    }
}

#[cfg(test)]
mod item_activity_tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use dtrpg_core::data::enums::ItemActivity;

    use super::super::item_activity_map;

    fn ids(names: &[&str]) -> Vec<Arc<str>> {
        names.iter().map(|n| Arc::<str>::from(*n)).collect()
    }

    #[test]
    fn activity_is_idle_unless_something_is_in_progress() {
        assert_eq!(ItemActivity::of(false, false), ItemActivity::Idle);
        assert_eq!(ItemActivity::of(false, true), ItemActivity::Verifying);
        assert_eq!(ItemActivity::of(true, false), ItemActivity::Downloading);
    }

    #[test]
    fn a_download_wins_over_a_disk_check() {
        assert_eq!(ItemActivity::of(true, true), ItemActivity::Downloading);
    }

    #[test]
    fn map_lists_only_non_idle_items_with_downloads_winning() {
        let verifying: HashSet<Arc<str>> = ids(&["checking", "both"]).into_iter().collect();
        let downloading = ids(&["both", "fetching"]);

        let map = item_activity_map(&verifying, downloading.iter());

        assert_eq!(map.len(), 3);
        assert_eq!(map["checking"], ItemActivity::Verifying);
        assert_eq!(map["both"], ItemActivity::Downloading);
        assert_eq!(map["fetching"], ItemActivity::Downloading);
        assert_eq!(map.get("untouched"), None);
    }
}
