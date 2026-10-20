//! `LibraryIndex::list_items_filtered`: the sidebar-filter + sort query that
//! replaces `LibraryController::invalidate_cache()`'s in-memory `Vec` scan.
//! See `openspec/changes/catalog-reads-from-index` for why.

use std::collections::HashSet;

use rusqlite::{Connection, params_from_iter};

use super::items::{row_to_item, status_to_text};
use crate::data::enums::ItemStatus;
use crate::data::index::IndexError;
use crate::data::library::LibraryItem;
use crate::util::sort::{SortDirection, SortMethod};

/// A sidebar-filter dimension to apply, beyond what's always true
/// (`SidebarFilter::AllTitles`). At most one is ever set by the current UI
/// (see `catalog-reads-from-index`'s design notes), but each is independent
/// of the others here rather than folded into one enum, since a future
/// caller combining them should not need a new query function.
#[derive(Default)]
pub struct ItemFilter<'a> {
    pub status:                Option<ItemStatus>,
    pub publisher:             Option<&'a str>,
    /// Member ids to match an item's `order_product_id` or `product_id`
    /// against, resolved by the caller (the controller's own
    /// `collection_members`, kept current by optimistic edits) rather than
    /// joined against the index's own `collection_membership` table — see
    /// design.md's risk note on why those two can briefly disagree.
    pub collection_member_ids: Option<&'a HashSet<u64>>,
    /// `(now_secs, window_days)`, matching
    /// [`super::items::count_recently_updated`]'s window semantics exactly.
    pub recently_updated:      Option<(i64, u32)>,
}

/// Returns every item matching `filter`, sorted by `sort`/`direction`.
///
/// Mirrors `dtrpg_ui`'s `item_matches_filter` (for the dimensions `filter`
/// sets) and `sort_items` (for every `SortMethod`, including `Custom`'s
/// closed `col_key` set) exactly, so swapping the in-memory scan for this
/// query changes nothing the user can observe.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn list_items_filtered(conn: &Connection, filter: &ItemFilter<'_>, sort: SortMethod,
                                  direction: SortDirection)
                                  -> Result<Vec<LibraryItem>, IndexError> {
    let mut clauses: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(status) = filter.status {
        clauses.push(format!("status = ?{}", args.len() + 1));
        args.push(Box::new(status_to_text(status)));
    }
    if let Some(publisher) = filter.publisher {
        clauses.push(format!("publisher = ?{}", args.len() + 1));
        args.push(Box::new(publisher.to_owned()));
    }
    if let Some((now_secs, window_days)) = filter.recently_updated {
        let window_secs = i64::from(window_days) * 24 * 60 * 60;
        clauses.push(format!("COALESCE(date_updated, date_added) >= ?{} - ?{}",
                             args.len() + 1,
                             args.len() + 2));
        args.push(Box::new(now_secs));
        args.push(Box::new(window_secs));
    }
    if let Some(member_ids) = filter.collection_member_ids {
        let placeholders = member_ids.iter()
                                     .map(|_| "?".to_string())
                                     .collect::<Vec<_>>()
                                     .join(", ");
        clauses.push(format!(
            "(order_product_id IN ({placeholders}) OR product_id IN ({placeholders}))"
        ));
        // Each `IN` list needs its own copy of the id set, since the same
        // placeholders string is reused for both columns above.
        for id in member_ids {
            args.push(Box::new(*id as i64));
        }
        for id in member_ids {
            args.push(Box::new(*id as i64));
        }
    }

    let where_clause = if clauses.is_empty() {
        String::new()
    }
    else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    let order_by = order_by_clause(sort);
    let sql = format!("SELECT * FROM items {where_clause} ORDER BY {order_by}");

    let mut stmt = conn.prepare(&sql)?;
    let mut items = stmt.query_map(params_from_iter(args.iter().map(AsRef::as_ref)),
                                   row_to_item)?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
    for item in &mut items {
        item.files = super::items::load_files_for(conn, &item.id)?;
    }
    if direction == SortDirection::Descending {
        items.reverse();
    }
    Ok(items)
}

/// Maps `sort` to an `ORDER BY` clause, mirroring `sort_items`'s match
/// (including its fallback-to-title arm for an unrecognized `Custom`
/// `col_key`) exactly.
fn order_by_clause(sort: SortMethod) -> &'static str {
    match sort {
        SortMethod::Title => "title",
        SortMethod::Publisher | SortMethod::Custom { col_key: "publisher", } => "publisher, title",
        SortMethod::DateAdded | SortMethod::Custom { col_key: "added" } => "added_order",
        SortMethod::PageCount | SortMethod::Custom { col_key: "pages" } => "pages, title",
        SortMethod::Custom { col_key: "system" } => "line, title",
        SortMethod::Custom { col_key: "size" } => "size_mb, title",
        SortMethod::Custom { .. } => "title",
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::data::index::LibraryIndex;

    // Test fixture builder mirroring `LibraryItem::new`'s own arity.
    #[allow(clippy::too_many_arguments)]
    fn item(id: &str, title: &str, publisher: &str, line: &str, pages: u32, size_mb: f64,
            added_order: u32, status: ItemStatus)
            -> LibraryItem {
        LibraryItem::new(id,
                         title,
                         publisher,
                         line,
                         "Core",
                         "PDF",
                         pages,
                         size_mb,
                         2024,
                         added_order,
                         status,
                         "#1C2A44",
                         "",
                         Some(1_700_000_000))
    }

    fn seeded() -> LibraryIndex {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        let items = vec![item("a1",
                              "Zed Book",
                              "Zenith",
                              "Line Z",
                              50,
                              5.0,
                              2,
                              ItemStatus::Cloud),
                         item("a2",
                              "Alpha Book",
                              "Alpha Press",
                              "Line A",
                              200,
                              20.0,
                              0,
                              ItemStatus::Downloaded),
                         item("a3",
                              "Mid Book",
                              "Mid House",
                              "Line M",
                              100,
                              10.0,
                              1,
                              ItemStatus::Cloud),];
        index.replace_all_items(&items).unwrap();
        index
    }

    fn titles(items: &[LibraryItem]) -> Vec<&str> {
        items.iter().map(|i| i.title.as_ref()).collect()
    }

    #[test]
    fn no_filter_returns_every_item() {
        let index = seeded();
        let items = index.list_items_filtered(&ItemFilter::default(),
                                              SortMethod::Title,
                                              SortDirection::Ascending)
                         .unwrap();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn sorts_by_title_ascending() {
        let index = seeded();
        let items = index.list_items_filtered(&ItemFilter::default(),
                                              SortMethod::Title,
                                              SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Alpha Book", "Mid Book", "Zed Book"]);
    }

    #[test]
    fn sorts_by_title_descending() {
        let index = seeded();
        let items = index.list_items_filtered(&ItemFilter::default(),
                                              SortMethod::Title,
                                              SortDirection::Descending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Zed Book", "Mid Book", "Alpha Book"]);
    }

    #[test]
    fn sorts_by_publisher() {
        let index = seeded();
        let items = index.list_items_filtered(&ItemFilter::default(),
                                              SortMethod::Publisher,
                                              SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Alpha Book", "Mid Book", "Zed Book"]);
    }

    #[test]
    fn sorts_by_date_added_using_added_order() {
        let index = seeded();
        let items = index.list_items_filtered(&ItemFilter::default(),
                                              SortMethod::DateAdded,
                                              SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Alpha Book", "Mid Book", "Zed Book"]);
    }

    #[test]
    fn sorts_by_page_count() {
        let index = seeded();
        let items = index.list_items_filtered(&ItemFilter::default(),
                                              SortMethod::PageCount,
                                              SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Zed Book", "Mid Book", "Alpha Book"]);
    }

    #[test]
    fn custom_sort_by_size_column() {
        let index = seeded();
        let items = index.list_items_filtered(&ItemFilter::default(),
                                              SortMethod::Custom { col_key: "size" },
                                              SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Zed Book", "Mid Book", "Alpha Book"]);
    }

    #[test]
    fn custom_sort_by_system_column() {
        let index = seeded();
        let items = index.list_items_filtered(&ItemFilter::default(),
                                              SortMethod::Custom { col_key: "system" },
                                              SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Alpha Book", "Mid Book", "Zed Book"]);
    }

    #[test]
    fn unrecognized_custom_col_key_falls_back_to_title() {
        let index = seeded();
        let items = index.list_items_filtered(&ItemFilter::default(),
                                              SortMethod::Custom { col_key: "nonsense", },
                                              SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Alpha Book", "Mid Book", "Zed Book"]);
    }

    #[test]
    fn status_filter_matches_only_downloaded() {
        let index = seeded();
        let filter = ItemFilter { status: Some(ItemStatus::Downloaded),
                                  ..Default::default() };
        let items = index.list_items_filtered(&filter, SortMethod::Title, SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Alpha Book"]);
    }

    #[test]
    fn status_filter_matches_only_cloud() {
        let index = seeded();
        let filter = ItemFilter { status: Some(ItemStatus::Cloud),
                                  ..Default::default() };
        let items = index.list_items_filtered(&filter, SortMethod::Title, SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Mid Book", "Zed Book"]);
    }

    #[test]
    fn publisher_filter_matches_only_that_publisher() {
        let index = seeded();
        let filter = ItemFilter { publisher: Some("Mid House"),
                                  ..Default::default() };
        let items = index.list_items_filtered(&filter, SortMethod::Title, SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Mid Book"]);
    }

    #[test]
    fn recently_updated_filter_excludes_items_outside_the_window() {
        let mut index = LibraryIndex::open_in_memory().unwrap();
        let now = 1_800_000_000;
        let day_secs = 24 * 60 * 60;
        let mut recent = item("r1",
                              "Recent Book",
                              "Pub",
                              "Line",
                              10,
                              1.0,
                              0,
                              ItemStatus::Cloud);
        recent.date_updated = Some(now - 10 * day_secs);
        let mut stale = item("r2",
                             "Stale Book",
                             "Pub",
                             "Line",
                             10,
                             1.0,
                             1,
                             ItemStatus::Cloud);
        stale.date_updated = Some(now - 40 * day_secs);
        index.replace_all_items(&[recent, stale]).unwrap();

        let filter = ItemFilter { recently_updated: Some((now, 30)),
                                  ..Default::default() };
        let items = index.list_items_filtered(&filter, SortMethod::Title, SortDirection::Ascending)
                         .unwrap();
        assert_eq!(titles(&items), vec!["Recent Book"]);
    }
}
