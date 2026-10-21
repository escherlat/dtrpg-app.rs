//! Tests for the library controller, grouped by concern.

#[cfg(test)]
mod item_check_tests {
    use std::time::{Duration, SystemTime};

    use super::super::item_check_due;

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
