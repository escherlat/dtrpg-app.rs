//! Folds a single-item availability check back into the catalog entry.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use crate::data::library::LibraryItem;
use crate::services::{LibraryServiceError, LibraryServiceErrorKind};

/// Applies the result of a single-item availability check to `item`, per
/// `catalog-availability-flag`'s single-item check requirement.
///
/// - `Ok`: replaces `item`'s fields with the fresh data, sets `is_available =
///   true`, and records `checked_at`.
/// - `Err` with kind `NotFound`: leaves other fields as-is, sets `is_available
///   = false`, and records `checked_at`.
/// - Any other `Err` (network, session, or otherwise): leaves `item` and its
///   flag entirely unchanged — a transient failure is not evidence of removal.
pub fn apply_check_result(item: &mut LibraryItem,
                          result: Result<LibraryItem, LibraryServiceError>,
                          checked_at: SystemTime) {
    match result {
        Ok(fresh) => {
            // Preserve identity/collection-membership fields from the existing
            // item rather than trusting the single-item response
            // for them: a per-item availability re-check should
            // only refresh display data, and blindly overwriting
            // `order_product_id`/`product_id` here has been observed to
            // silently corrupt `collection_member_id` lookups for the checked
            // item (its collection membership stops resolving) when
            // the single-item endpoint's response doesn't carry the
            // exact same id values the list fetch populated.
            let id = Arc::clone(&item.id);
            let numeric_id = item.numeric_id;
            let order_product_id = item.order_product_id;
            let product_id = item.product_id;
            // `downloaded` is a local-only fact the server response knows
            // nothing about - the single-item endpoint always
            // reports it `false`. Preserve each file's existing
            // flag by id so a re-check (or the disk-presence
            // verification racing it, see
            // `verify-downloaded-status-against-disk`) doesn't get its
            // correction wiped the moment this slower network check resolves.
            let downloaded_by_id: HashMap<Arc<str>, bool> =
                item.files
                    .iter()
                    .map(|f| (Arc::clone(&f.id), f.downloaded))
                    .collect();
            *item = fresh;
            item.id = id;
            item.numeric_id = numeric_id;
            item.order_product_id = order_product_id;
            item.product_id = product_id;
            for file in &mut item.files {
                if let Some(&downloaded) = downloaded_by_id.get(&file.id) {
                    file.downloaded = downloaded;
                }
            }
            item.recompute_status();
            item.is_available = true;
            item.availability_last_checked = Some(checked_at);
        }
        Err(e) if e.kind == LibraryServiceErrorKind::NotFound => {
            item.is_available = false;
            item.availability_last_checked = Some(checked_at);
        }
        Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::apply_check_result;
    use crate::data::enums::ItemStatus;
    use crate::data::library::{LibraryItem, LibraryItemFile};
    use crate::services::{LibraryServiceError, LibraryServiceErrorKind};

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
