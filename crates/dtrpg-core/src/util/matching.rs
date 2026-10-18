//! Item filtering and text matching predicates.

use std::collections::HashSet;

use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;
use crate::util::filter::SidebarFilter;

/// Seconds in a day, used to convert a "Recently Updated window" day count
/// into the seconds [`item_recently_updated`] compares against.
const DAY_SECS: i64 = 24 * 60 * 60;

// ── Matching functions
// ─────────────────────────────────────────────────────────────────

/// Returns `true` if `item`'s most recent timestamp (`date_updated`, falling
/// back to `date_added`) falls within the last `window_days` relative to
/// `now_secs`.
///
/// Returns `false` if neither timestamp is set — an item can't be "recent"
/// without a date.
#[must_use]
pub fn item_recently_updated(item: &LibraryItem, now_secs: i64, window_days: u32) -> bool {
    let Some(ts) = item.date_updated.or(item.date_added)
    else {
        return false;
    };
    ts >= now_secs - i64::from(window_days) * DAY_SECS
}

/// Returns `true` if `item` passes the given sidebar filter.
///
/// For `Collection` filters, `collection_members` contains `productId` values
/// from the product-list-items API. The item passes if either
/// `order_product_id` or `product_id` matches, since the API returns
/// `productId` while catalog items carry both IDs.
///
/// `window_days` is the user-configured "Recently Updated window" applied
/// when `filter` is [`SidebarFilter::RecentlyUpdated`]; ignored otherwise.
#[must_use]
pub fn item_matches_filter(item: &LibraryItem, filter: &SidebarFilter,
                           collection_members: &HashSet<u64>, now_secs: i64, window_days: u32)
                           -> bool {
    match filter {
        SidebarFilter::AllTitles => true,
        SidebarFilter::RecentlyUpdated => item_recently_updated(item, now_secs, window_days),
        SidebarFilter::OnDevice => item.status == ItemStatus::Downloaded,
        SidebarFilter::InCloud => item.status == ItemStatus::Cloud,
        SidebarFilter::Publisher(name) => item.publisher.as_ref() == name.as_ref(),
        SidebarFilter::Collection(_, _) => {
            (item.order_product_id > 0 && collection_members.contains(&item.order_product_id))
            || (item.product_id > 0 && collection_members.contains(&item.product_id))
        }
    }
}

/// Returns `true` if `member_ids` contains `order_product_id` or `product_id`
/// (each treated as absent when `0`).
///
/// The `product_list_items` API resource is keyed by `product_id` alone — it
/// never returns `order_product_id` — so a collection's cached `member_ids`
/// only ever contains `product_id` values in practice. Some call sites still
/// track membership under [`collection_member_id`]'s order-preferring value,
/// so any membership check must tolerate either id space rather than relying
/// on a single preferred id.
#[must_use]
pub fn member_ids_contain(member_ids: &[u64], order_product_id: u64, product_id: u64) -> bool {
    (order_product_id > 0 && member_ids.contains(&order_product_id))
    || (product_id > 0 && member_ids.contains(&product_id))
}

/// Resolves the id used for collection membership: `order_product_id`,
/// falling back to `product_id` when the former is absent (`0`).
///
/// Mirrors the preference `item_matches_filter` and
/// `RustSdkCollectionsService::list_collections` (in `dtrpg-core`) already
/// use when reading membership, so an id added here matches correctly when
/// the collections cache is next reloaded from the API.
#[must_use]
pub fn collection_member_id(item: &LibraryItem) -> u64 {
    if item.order_product_id > 0 {
        item.order_product_id
    }
    else {
        item.product_id
    }
}

/// Returns `true` if `item` contains `query` in title, publisher, or game line
/// (case-insensitive).
#[must_use]
pub fn item_matches_query(item: &LibraryItem, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let q = query.to_lowercase();
    item.title.to_lowercase().contains(&q)
    || item.publisher.to_lowercase().contains(&q)
    || item.line.to_lowercase().contains(&q)
}

/// Returns `true` if `name` contains `query` (case-insensitive).
///
/// Used to filter the sidebar's publishers and collections sections by their
/// inline search bars.
#[must_use]
pub fn name_matches_query(name: &str, query: &str) -> bool {
    query.is_empty() || name.to_lowercase().contains(&query.to_lowercase())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_ids_contain_matches_on_order_product_id() {
        assert!(member_ids_contain(&[42], 42, 99));
    }

    #[test]
    fn member_ids_contain_matches_on_product_id() {
        assert!(member_ids_contain(&[99], 42, 99));
    }

    #[test]
    fn member_ids_contain_matches_when_ids_differ_and_only_product_id_is_cached() {
        // Mirrors the real-world case: the API's product_list_items resource is
        // keyed by product_id alone, so a genuinely-member item whose
        // order_product_id differs from its product_id must still match.
        assert!(member_ids_contain(&[144_239], 22_654_728, 144_239));
    }

    #[test]
    fn member_ids_contain_ignores_zero_ids() {
        assert!(!member_ids_contain(&[0], 0, 0));
    }

    #[test]
    fn member_ids_contain_returns_false_when_neither_id_present() {
        assert!(!member_ids_contain(&[1, 2, 3], 42, 99));
    }

    fn item(date_added: Option<i64>, date_updated: Option<i64>) -> LibraryItem {
        let mut item = LibraryItem::new("e1",
                                        "Moria",
                                        "Free League",
                                        "",
                                        "",
                                        "",
                                        0,
                                        0.0,
                                        0,
                                        0,
                                        ItemStatus::Cloud,
                                        "#000000",
                                        "",
                                        date_added);
        item.date_updated = date_updated;
        item
    }

    #[test]
    fn item_recently_updated_matches_within_30_days_via_date_updated() {
        let now = 1_800_000_000;
        assert!(item_recently_updated(&item(None, Some(now - 10 * DAY_SECS)), now, 30));
    }

    #[test]
    fn item_recently_updated_falls_back_to_date_added_when_no_date_updated() {
        let now = 1_800_000_000;
        assert!(item_recently_updated(&item(Some(now - 10 * DAY_SECS), None), now, 30));
    }

    #[test]
    fn item_recently_updated_excludes_just_past_the_30_day_window() {
        let now = 1_800_000_000;
        assert!(!item_recently_updated(&item(None, Some(now - 30 * DAY_SECS - 1)), now, 30));
    }

    #[test]
    fn item_recently_updated_false_when_no_timestamps() {
        let now = 1_800_000_000;
        assert!(!item_recently_updated(&item(None, None), now, 30));
    }

    #[test]
    fn item_recently_updated_respects_a_non_default_window() {
        let now = 1_800_000_000;
        let ten_days_old = item(None, Some(now - 10 * DAY_SECS));
        // A 7-day window excludes an item that a 30-day (or wider) window
        // would include.
        assert!(!item_recently_updated(&ten_days_old, now, 7));
        assert!(item_recently_updated(&ten_days_old, now, 14));
    }

    #[test]
    fn item_matches_filter_delegates_to_item_recently_updated() {
        let now = 1_800_000_000;
        let recent = item(None, Some(now - 10 * DAY_SECS));
        let stale = item(None, Some(now - 40 * DAY_SECS));
        let members = HashSet::new();
        assert!(item_matches_filter(&recent, &SidebarFilter::RecentlyUpdated, &members, now, 30));
        assert!(!item_matches_filter(&stale, &SidebarFilter::RecentlyUpdated, &members, now, 30));
    }
}
