//! Tests for the pure catalog-merge and timing-gate helpers in
//! `super::merge`. Moved here from the UI controller's `tests_catalog.rs` in
//! ADR 0002 Slice 2, alongside the functions themselves.

use super::merge::*;
use crate::data::catalog_cache::CacheMetadata;
use crate::data::enums::ItemStatus;
use crate::data::library::{LibraryItem, LibraryItemFile};

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
                      name: "File".into(),
                      format: "PDF".into(),
                      size_mb: 1.0,
                      downloaded,
                      offloaded: false }
}

mod reconcile_catalog_tests {
    use super::*;

    #[test]
    fn item_present_in_both_is_refreshed_and_marked_available() {
        let mut local = item("b1", "Old Title");
        local.is_available = false;
        let live = item("b1", "New Title");

        let reconciled = reconcile_catalog(vec![local], vec![live]);

        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].title.as_ref(), "New Title");
        assert!(reconciled[0].is_available);
    }

    #[test]
    fn item_only_local_is_kept_and_flagged_unavailable() {
        let local = item("b1", "Local Only");

        let reconciled = reconcile_catalog(vec![local], vec![]);

        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].id.as_ref(), "b1");
        assert!(!reconciled[0].is_available);
    }

    #[test]
    fn item_only_live_is_appended_and_available() {
        let live = item("b1", "New Item");

        let reconciled = reconcile_catalog(vec![], vec![live]);

        assert_eq!(reconciled.len(), 1);
        assert_eq!(reconciled[0].id.as_ref(), "b1");
        assert!(reconciled[0].is_available);
    }

    #[test]
    fn previously_unavailable_item_reappearing_clears_the_flag() {
        let mut local = item("b1", "Was Gone");
        local.is_available = false;
        let live = item("b1", "Was Gone");

        let reconciled = reconcile_catalog(vec![local], vec![live]);

        assert!(reconciled[0].is_available);
    }

    #[test]
    fn existing_catalog_order_is_preserved_for_known_items() {
        let existing = vec![item("b1", "First"), item("b2", "Second")];
        let live = vec![item("b2", "Second Updated"), item("b1", "First Updated")];

        let reconciled = reconcile_catalog(existing, live);

        assert_eq!(reconciled.len(), 2);
        assert_eq!(reconciled[0].id.as_ref(), "b1");
        assert_eq!(reconciled[1].id.as_ref(), "b2");
    }

    #[test]
    fn downloaded_status_survives_reconcile_against_a_live_fetch() {
        let mut local = item("b1", "Old Title");
        local.files = vec![file("f1", true), file("f2", true)];
        local.recompute_status();
        assert_eq!(local.status, ItemStatus::Downloaded);

        let mut live = item("b1", "New Title");
        live.files = vec![file("f1", false), file("f2", false)];

        let reconciled = reconcile_catalog(vec![local], vec![live]);

        assert!(reconciled[0].files.iter().all(|f| f.downloaded));
        assert_eq!(reconciled[0].status, ItemStatus::Downloaded);
    }

    #[test]
    fn live_file_with_no_cached_counterpart_is_not_downloaded() {
        let mut local = item("b1", "Title");
        local.files = vec![file("f1", true)];

        let mut live = item("b1", "Title");
        live.files = vec![file("f1", false), file("f2", false)];

        let reconciled = reconcile_catalog(vec![local], vec![live]);

        let f2_downloaded = reconciled[0].files
                                         .iter()
                                         .find(|f| f.id.as_ref() == "f2")
                                         .map(|f| f.downloaded);
        assert_eq!(f2_downloaded, Some(false));
    }

    #[test]
    fn partially_downloaded_item_keeps_its_per_file_state() {
        let mut local = item("b1", "Title");
        local.files = vec![file("f1", true), file("f2", false)];

        let mut live = item("b1", "Title");
        live.files = vec![file("f1", false), file("f2", false)];

        let reconciled = reconcile_catalog(vec![local], vec![live]);

        let f1_downloaded = reconciled[0].files
                                         .iter()
                                         .find(|f| f.id.as_ref() == "f1")
                                         .map(|f| f.downloaded);
        let f2_downloaded = reconciled[0].files
                                         .iter()
                                         .find(|f| f.id.as_ref() == "f2")
                                         .map(|f| f.downloaded);
        assert_eq!(f1_downloaded, Some(true));
        assert_eq!(f2_downloaded, Some(false));
        assert_eq!(reconciled[0].status, ItemStatus::Cloud);
    }
}

mod reload_cooldown_tests {
    use super::*;

    fn meta(saved_at_secs: u64) -> CacheMetadata {
        CacheMetadata { saved_at_secs,
                        item_count: 10,
                        schema_version: 2,
                        last_item_check_batch_secs: None,
                        last_fresh_install_request_secs: None }
    }

    #[test]
    fn reload_is_suppressed_when_cache_is_recent() {
        let now = 1_000;
        let recent = meta(now - 10); // 10s ago, within the 60s cooldown

        assert!(reload_cooldown_active(Some(&recent), now));
    }

    #[test]
    fn reload_proceeds_when_cache_is_older_than_cooldown() {
        let now = 1_000;
        let old = meta(now - 61); // just past the 60s cooldown

        assert!(!reload_cooldown_active(Some(&old), now));
    }

    #[test]
    fn reload_proceeds_when_no_metadata_exists() {
        let now = 1_000;

        assert!(!reload_cooldown_active(None, now));
    }
}

mod fresh_install_gating_tests {
    use super::*;

    #[test]
    fn request_is_suppressed_within_the_minimum_interval() {
        let now = 1_000;
        assert!(fresh_install_request_gated(Some(now - 10), now)); // 10s ago, within 60s
    }

    #[test]
    fn request_proceeds_once_the_minimum_interval_elapses() {
        let now = 1_000;
        assert!(!fresh_install_request_gated(Some(now - 61), now)); // just past 60s
    }

    #[test]
    fn request_proceeds_when_never_recorded() {
        let now = 1_000;
        assert!(!fresh_install_request_gated(None, now));
    }
}

mod partial_fetch_tests {
    use super::*;

    #[test]
    fn growth_only_mismatch_triggers_partial_fetch() {
        assert!(should_attempt_partial_fetch(12, 10));
    }

    #[test]
    fn decrease_does_not_trigger_partial_fetch() {
        assert!(!should_attempt_partial_fetch(8, 10));
    }

    #[test]
    fn matching_count_does_not_trigger_partial_fetch() {
        assert!(!should_attempt_partial_fetch(10, 10));
    }

    #[test]
    fn partial_fetch_merges_new_and_refreshed_items_without_flagging_unavailable() {
        let mut existing_a = item("b1", "Old Title");
        existing_a.is_available = false; // already unavailable before the partial fetch
        let existing_b = item("b2", "Unrelated");
        let refreshed_a = item("b1", "New Title");
        let new_c = item("b3", "Brand New");

        let merged = merge_partial_fetch(vec![existing_a, existing_b.clone()],
                                         vec![refreshed_a, new_c]);

        assert_eq!(merged.len(), 3);
        match merged.iter().find(|i| i.id.as_ref() == "b1") {
            Some(a) => {
                assert_eq!(a.title.as_ref(), "New Title");
                assert!(a.is_available); // refreshed via the partial fetch, so flag clears
            }
            None => panic!("expected item b1 to be present"),
        }
        match merged.iter().find(|i| i.id.as_ref() == "b3") {
            Some(c) => assert!(c.is_available),
            None => panic!("expected item b3 to be present"),
        }
    }

    #[test]
    fn partial_fetch_leaves_items_absent_from_the_response_untouched() {
        let mut existing = item("b1", "Untouched");
        existing.is_available = false;

        let merged = merge_partial_fetch(vec![existing], vec![]);

        // Absence from a partial response proves nothing — the flag must not
        // change.
        assert!(!merged[0].is_available);
    }

    #[test]
    fn since_is_derived_from_the_most_recent_timestamp() {
        let mut older = item("b1", "Older");
        older.date_updated = Some(1_700_000_000);
        let mut newer = item("b2", "Newer");
        newer.date_updated = Some(1_800_000_000);

        let since = partial_fetch_since(&[older, newer]);

        assert_eq!(since.as_deref(), Some("2027-01-15T08:00:00+00:00"));
    }

    #[test]
    fn since_is_none_for_an_empty_catalog() {
        assert_eq!(partial_fetch_since(&[]), None);
    }
}
