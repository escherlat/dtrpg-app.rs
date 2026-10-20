//! `list_items_filtered` / `count_items_filtered`: the sidebar-filter + sort
//! queries that replaced `LibraryController::invalidate_cache()`'s in-memory
//! `Vec` scan (`catalog-reads-from-index`) and now also carry the search box
//! and `Tag` dimensions (#353). See the change proposals for why.

use rusqlite::{Connection, params_from_iter};

use super::clause::{ItemFilter, build_where};
use crate::data::index::IndexError;
use crate::data::index::items::{load_files_for, row_to_item};
use crate::data::library::LibraryItem;
use crate::util::sort::{SortDirection, SortMethod};

/// Returns every item matching `filter` (including its `search` dimension, if
/// set), sorted by `sort`/`direction`.
///
/// Mirrors `dtrpg_ui`'s `item_matches_filter` (for the dimensions `filter`
/// sets) and `sort_items` (for every `SortMethod`, including `Custom`'s
/// closed `col_key` set) exactly, so swapping the in-memory scan for this
/// query changes nothing the user can observe.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(in crate::data::index) fn list_items_filtered(conn: &Connection, filter: &ItemFilter<'_>,
                                                  sort: SortMethod, direction: SortDirection)
                                                  -> Result<Vec<LibraryItem>, IndexError> {
    let (where_clause, args) = build_where(filter);
    let order_by = order_by_clause(sort);
    let sql = format!("SELECT * FROM items {where_clause} ORDER BY {order_by}");

    let mut stmt = conn.prepare(&sql)?;
    let mut items = stmt.query_map(params_from_iter(args.iter().map(AsRef::as_ref)),
                                   row_to_item)?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
    for item in &mut items {
        item.files = load_files_for(conn, &item.id)?;
    }
    if direction == SortDirection::Descending {
        items.reverse();
    }
    Ok(items)
}

/// Counts the items matching `filter` without materializing any of them —
/// the "matching the filter alone, search ignored" number the toolbar's
/// `count_label_for` calls `filter_count`, which the list query can no longer
/// answer once `search` is one of its dimensions.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(in crate::data::index) fn count_items_filtered(conn: &Connection, filter: &ItemFilter<'_>)
                                                   -> Result<usize, IndexError> {
    let (where_clause, args) = build_where(filter);
    let sql = format!("SELECT COUNT(*) FROM items {where_clause}");
    let count: i64 = conn.query_row(&sql,
                                    params_from_iter(args.iter().map(AsRef::as_ref)),
                                    |row| row.get(0))?;
    Ok(count as usize)
}

/// Returns the items matching `filter`, ordered so a single linear pass
/// can split them into publisher groups.
///
/// Group order is ascending by Unicode-lowercase publisher; item order within
/// each group follows `sort`/`direction`. This matches
/// `dtrpg_ui::util::publisher::group_by_publisher`'s behavior.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(in crate::data::index) fn list_items_for_grouping_by_publisher(
    conn: &Connection, filter: &ItemFilter<'_>, sort: SortMethod, direction: SortDirection)
    -> Result<Vec<LibraryItem>, IndexError> {
    let (where_clause, args) = build_where(filter);
    let order_by = format!("ulower(publisher) ASC, publisher, {}",
                           user_sort_clause(sort, direction));
    let sql = format!("SELECT * FROM items {where_clause} ORDER BY {order_by}");

    let mut stmt = conn.prepare(&sql)?;
    let mut items = stmt.query_map(params_from_iter(args.iter().map(AsRef::as_ref)),
                                   row_to_item)?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
    for item in &mut items {
        item.files = load_files_for(conn, &item.id)?;
    }
    Ok(items)
}

/// Returns the items matching `filter`, expanded to one row per tag the item
/// carries, ordered so a single linear pass can split them into tag groups.
/// Rows with `group = None` are untagged items and belong in the final group.
///
/// Group order is ascending by Unicode-lowercase tag; item order within each
/// group follows `sort`/`direction`. This matches
/// `dtrpg_ui::util::publisher::group_by_tag`'s behavior.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if either query fails.
pub(in crate::data::index) fn list_items_for_grouping_by_tag(
    conn: &Connection, filter: &ItemFilter<'_>, sort: SortMethod, direction: SortDirection)
    -> Result<Vec<(Option<String>, LibraryItem)>, IndexError> {
    let (where_clause, args) = build_where(filter);
    let user_sort = user_sort_clause(sort, direction);

    let mut out: Vec<(Option<String>, LibraryItem)> = Vec::new();

    // Tagged items: one row per (item, tag). The WHERE already filtered items
    // by search/tag dimensions; this JOIN only expands each surviving item
    // into its tag groups.
    let order_by = format!("ulower(tg.name) ASC, tg.name, {user_sort}");
    let tagged_sql = format!("SELECT items.*, tg.name AS group_name FROM items          JOIN item_tags it ON it.item_id = items.id          JOIN tags tg ON tg.id = it.tag_id          {where_clause} ORDER BY {order_by}");
    let mut stmt = conn.prepare(&tagged_sql)?;
    let tagged = stmt.query_map(params_from_iter(args.iter().map(AsRef::as_ref)), |row| {
                         let group = row.get::<_, String>("group_name")?;
                         let item = row_to_item(row)?;
                         Ok((Some(group), item))
                     })?
                     .collect::<rusqlite::Result<Vec<_>>>()?;
    out.extend(tagged);

    // Untagged items: appended as the final group.
    let exists_clause = "NOT EXISTS (SELECT 1 FROM item_tags WHERE item_id = items.id)";
    let untagged_sql = if where_clause.is_empty() {
        format!("SELECT * FROM items WHERE {exists_clause} ORDER BY {user_sort}")
    }
    else {
        format!("SELECT * FROM items {where_clause} AND {exists_clause} ORDER BY {user_sort}")
    };
    let mut stmt = conn.prepare(&untagged_sql)?;
    let mut untagged = stmt.query_map(params_from_iter(args.iter().map(AsRef::as_ref)),
                                      row_to_item)?
                           .collect::<rusqlite::Result<Vec<_>>>()?;
    for item in &mut untagged {
        item.files = load_files_for(conn, &item.id)?;
    }
    out.extend(untagged.into_iter().map(|item| (None, item)));

    Ok(out)
}

/// The user-sort part of an `ORDER BY` clause, with `direction` applied to
/// every key. Kept separate from the grouping key so group order is always
/// ascending while the secondary order respects the user's choice.
fn user_sort_clause(sort: SortMethod, direction: SortDirection) -> String {
    let clause = order_by_clause(sort);
    if direction == SortDirection::Ascending {
        clause.to_string()
    }
    else {
        clause.split(", ")
              .map(|col| format!("{col} DESC"))
              .collect::<Vec<_>>()
              .join(", ")
    }
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
    use crate::data::enums::ItemStatus;
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

    #[test]
    fn count_matches_list_length_across_dimensions() {
        let index = seeded();
        let book_query = crate::util::search::SearchQuery::parse("book");
        let cases = [ItemFilter::default(),
                     ItemFilter { status: Some(ItemStatus::Cloud),
                                  ..Default::default() },
                     ItemFilter { publisher: Some("Mid House"),
                                  ..Default::default() },
                     ItemFilter { tag_name: Some("nothing"),
                                  ..Default::default() },
                     ItemFilter { search: Some(&book_query),
                                  ..Default::default() }];
        for filter in cases {
            let listed =
                index.list_items_filtered(&filter, SortMethod::Title, SortDirection::Ascending)
                     .unwrap();
            assert_eq!(index.count_items_filtered(&filter).unwrap(),
                       listed.len(),
                       "count disagreed with list for {filter:?}");
        }
    }
}
