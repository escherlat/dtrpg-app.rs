//! Parity tests for `openspec/changes/group-by-sql`: the SQL-ordered grouping
//! queries must return the same `ItemGroup` shapes as the previous Rust
//! partitioners over the filtered result set.

use std::collections::HashMap;
use std::sync::Arc;

use super::super::LibraryIndex;
use super::clause::ItemFilter;
use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;
use crate::data::tag_color::TagColor;
use crate::util::search::SearchQuery;
use crate::util::sort::{SortDirection, SortMethod};

fn item(id: &str, title: &str, publisher: &str, status: ItemStatus) -> LibraryItem {
    LibraryItem::new(id,
                     title,
                     publisher,
                     "",
                     "",
                     "PDF",
                     10,
                     1.0,
                     2024,
                     0,
                     status,
                     "#1C2A44",
                     "",
                     Some(1_700_000_000))
}

fn seeded() -> LibraryIndex {
    let mut index = LibraryIndex::open_in_memory().unwrap();
    let items = vec![item("a", "Alpha Bestiary", "Kobold Press", ItemStatus::Cloud),
                     item("b", "Beta Manual", "Paizo", ItemStatus::Downloaded),
                     item("c", "Gamma Gazette", "paizo", ItemStatus::Cloud),
                     item("d", "Delta Digest", "Wizards", ItemStatus::Downloaded),
                     item("e", "Epsilon Opus", "Wizards", ItemStatus::Cloud),];
    index.replace_all_items(&items).unwrap();
    let tagging: [(&str, &[&str]); 4] = [("a", &["Solo", "horror"]),
                                         ("b", &["To Run"]),
                                         ("d", &["HORRAH"]),
                                         ("e", &["solo", "To Run"])];
    for (id, tags) in tagging {
        let tags = tags.iter()
                       .map(|t| (t.to_string(), Option::<TagColor>::None))
                       .collect::<Vec<_>>();
        index.set_item_tags(id, &tags, 1).unwrap();
    }
    index
}

fn shape(groups: &[ItemGroup]) -> Vec<(String, Vec<String>)> {
    groups.iter()
          .map(|group| {
              (group.title.to_string(), group.items.iter().map(|i| i.id.to_string()).collect())
          })
          .collect()
}

/// The old Rust `group_by_publisher`: publisher entries sorted by lowercase
/// name, items within each group in input order.
fn rust_group_by_publisher(items: Vec<LibraryItem>) -> Vec<ItemGroup> {
    let mut counts: HashMap<Arc<str>, usize> = HashMap::new();
    for item in &items {
        *counts.entry(Arc::clone(&item.publisher)).or_insert(0) += 1;
    }
    let mut entries: Vec<_> = counts.into_keys().collect();
    entries.sort_by(|a, b| {
               a.to_lowercase()
                .cmp(&b.to_lowercase())
                .then_with(|| a.cmp(b))
           });

    let mut buckets: HashMap<Arc<str>, Vec<LibraryItem>> = HashMap::new();
    for item in items {
        buckets.entry(Arc::clone(&item.publisher))
               .or_default()
               .push(item);
    }
    entries.into_iter()
           .filter_map(|name| {
               buckets.remove(&name)
                      .map(|group_items| ItemGroup { title: name,
                                                     items: group_items, })
           })
           .collect()
}

/// The old Rust `group_by_tag`: groups by lowercase tag, title from the first
/// encountered original name, untagged items last.
fn rust_group_by_tag(items: Vec<LibraryItem>, tag_names: &HashMap<String, Vec<String>>,
                     untagged: &str)
                     -> Vec<ItemGroup> {
    let mut by_tag: HashMap<String, ItemGroup> = HashMap::new();
    let mut without_tags = Vec::new();
    for item in items {
        let names = tag_names.get(&*item.id).map(Vec::as_slice).unwrap_or(&[]);
        if names.is_empty() {
            without_tags.push(item);
            continue;
        }
        for name in names {
            by_tag.entry(name.to_lowercase())
                  .or_insert_with(|| ItemGroup { title: Arc::from(name.as_str()),
                                                 items: Vec::new(), })
                  .items
                  .push(item.clone());
        }
    }
    let mut keyed: Vec<(String, ItemGroup)> = by_tag.into_iter().collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.title.cmp(&b.1.title)));
    let mut groups: Vec<ItemGroup> = keyed.into_iter().map(|(_, group)| group).collect();
    if !without_tags.is_empty() {
        groups.push(ItemGroup { title: Arc::from(untagged),
                                items: without_tags, });
    }
    groups
}

/// Splits publisher-ordered items at exact publisher boundaries.
fn sql_split_by_publisher(items: Vec<LibraryItem>) -> Vec<ItemGroup> {
    let mut groups: Vec<ItemGroup> = Vec::new();
    for item in items {
        if let Some(last) = groups.last_mut()
           && last.title == item.publisher
        {
            last.items.push(item);
            continue;
        }
        groups.push(ItemGroup { title: Arc::clone(&item.publisher),
                                items: vec![item], });
    }
    groups
}

/// Splits tag-ordered rows at lowercase tag boundaries; `None` rows form the
/// final untagged group.
fn sql_split_by_tag(rows: Vec<(Option<String>, LibraryItem)>, untagged: &str) -> Vec<ItemGroup> {
    let mut groups: Vec<ItemGroup> = Vec::new();
    let mut untagged_items = Vec::new();
    for (group, item) in rows {
        match group {
            None => untagged_items.push(item),
            Some(name) => {
                if let Some(last) = groups.last_mut()
                   && last.title.to_lowercase() == name.to_lowercase()
                {
                    last.items.push(item);
                    continue;
                }
                groups.push(ItemGroup { title: Arc::from(name),
                                        items: vec![item], });
            }
        }
    }
    if !untagged_items.is_empty() {
        groups.push(ItemGroup { title: Arc::from(untagged),
                                items: untagged_items, });
    }
    groups
}

/// Reused structure: a group title and its items. Defined here so the test
/// oracle matches the UI `ItemGroup` shape without a cross-crate dependency.
#[derive(Debug, Clone)]
struct ItemGroup {
    title: Arc<str>,
    items: Vec<LibraryItem>,
}

#[test]
fn sql_publisher_grouping_matches_rust_reference() {
    let index = seeded();
    for direction in [SortDirection::Ascending, SortDirection::Descending] {
        let sql = sql_split_by_publisher(
            index
                .list_items_for_grouping_by_publisher(
                    &ItemFilter::default(),
                    SortMethod::Title,
                    direction,
                )
                .unwrap(),
        );
        let reference = rust_group_by_publisher(index.list_items_filtered(&ItemFilter::default(),
                                                                          SortMethod::Title,
                                                                          direction)
                                                     .unwrap());
        assert_eq!(shape(&sql), shape(&reference), "direction={direction:?}");
    }
}

#[test]
fn sql_publisher_grouping_respects_search_filter() {
    let index = seeded();
    let query = SearchQuery::parse("Manual");
    let filter = ItemFilter { search: Some(&query),
                              ..ItemFilter::default() };
    let groups = sql_split_by_publisher(
        index
            .list_items_for_grouping_by_publisher(
                &filter,
                SortMethod::Title,
                SortDirection::Ascending,
            )
            .unwrap(),
    );
    assert_eq!(groups.len(), 1);
    assert_eq!(&*groups[0].title, "Paizo");
    assert_eq!(groups[0].items.iter().map(|i| &*i.id).collect::<Vec<_>>(),
               ["b"]);
}

#[test]
fn sql_tag_grouping_matches_rust_reference() {
    let index = seeded();
    let tag_names: HashMap<String, Vec<String>> =
        index.all_item_tags()
             .unwrap()
             .into_iter()
             .map(|(id, names)| (id, names.into_iter().map(|n| n.to_string()).collect()))
             .collect();
    for direction in [SortDirection::Ascending, SortDirection::Descending] {
        let sql = sql_split_by_tag(index.list_items_for_grouping_by_tag(&ItemFilter::default(),
                                                                        SortMethod::Title,
                                                                        direction)
                                        .unwrap(),
                                   "Untagged");
        let reference = rust_group_by_tag(index.list_items_filtered(&ItemFilter::default(),
                                                                    SortMethod::Title,
                                                                    direction)
                                               .unwrap(),
                                          &tag_names,
                                          "Untagged");
        assert_eq!(shape(&sql), shape(&reference), "direction={direction:?}");
    }
}

#[test]
fn sql_tag_grouping_puts_untagged_last() {
    let index = seeded();
    let groups = sql_split_by_tag(index.list_items_for_grouping_by_tag(&ItemFilter::default(),
                                                                       SortMethod::Title,
                                                                       SortDirection::Ascending)
                                       .unwrap(),
                                  "Untagged");
    assert_eq!(&*groups.last().unwrap().title, "Untagged");
    assert_eq!(groups.last()
                     .unwrap()
                     .items
                     .iter()
                     .map(|i| &*i.id)
                     .collect::<Vec<_>>(),
               ["c"]);
}
