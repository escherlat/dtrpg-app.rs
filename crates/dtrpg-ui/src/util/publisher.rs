//! Data model, filtering, sorting, and stub catalog for the Libri library view.

use std::collections::HashMap;
use std::sync::Arc;

use dtrpg_core::data::library::LibraryItem;
use dtrpg_core::data::tag_library::TagLibrary;

// ── Publisher aggregation
// ─────────────────────────────────────────────────────

/// A publisher entry with the count of its items in the current result set.
#[derive(Debug, Clone)]
pub struct PublisherEntry {
    pub name:  Arc<str>,
    pub count: usize,
}

/// Returns publisher entries sorted by name ascending (case-insensitive).
#[must_use]
pub fn publisher_entries(items: &[LibraryItem]) -> Vec<PublisherEntry> {
    let mut map: HashMap<Arc<str>, usize> = HashMap::new();
    for item in items {
        *map.entry(Arc::clone(&item.publisher)).or_insert(0) += 1;
    }
    let mut entries: Vec<PublisherEntry> = map.into_iter()
                                              .map(|(name, count)| PublisherEntry { name, count })
                                              .collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    entries
}

// ── Grouped view
// ──────────────────────────────────────────────────────────────

/// Items under one heading in a grouped catalog (already filtered and sorted).
#[derive(Debug, Clone)]
pub struct ItemGroup {
    /// The publisher or tag the items are grouped under.
    pub title: Arc<str>,
    pub items: Vec<LibraryItem>,
}

/// Partitions `items` into publisher groups in the same order as
/// `publisher_entries`.
#[must_use]
pub fn group_by_publisher(items: Vec<LibraryItem>) -> Vec<ItemGroup> {
    let entries = publisher_entries(&items);
    let mut map: HashMap<Arc<str>, Vec<LibraryItem>> = HashMap::new();
    for item in items {
        map.entry(Arc::clone(&item.publisher))
           .or_default()
           .push(item);
    }
    entries.into_iter()
           .filter_map(|e| {
               map.remove(&e.name)
                  .map(|group_items| ItemGroup { title: e.name,
                                                 items: group_items, })
           })
           .collect()
}

/// Partitions `items` into one group per tag, by tag name ascending
/// (case-insensitive).
///
/// An item with several tags is in the group of each of them. Items with no
/// tag are in a last group called `untagged`, if there are any.
#[must_use]
pub fn group_by_tag(items: Vec<LibraryItem>, tags: &TagLibrary, untagged: &str) -> Vec<ItemGroup> {
    let mut by_tag: HashMap<String, ItemGroup> = HashMap::new();
    let mut without_tags = Vec::new();
    for item in items {
        let names = tags.tag_names_of(&item.id);
        if names.is_empty() {
            without_tags.push(item);
            continue;
        }
        for name in names {
            by_tag.entry(name.to_lowercase())
                  .or_insert_with(|| {
                      let title = tags.entry(name).map_or(name, |entry| &entry.name);
                      ItemGroup { title: Arc::clone(title),
                                  items: Vec::new(), }
                  })
                  .items
                  .push(item.clone());
        }
    }

    let mut keyed: Vec<(String, ItemGroup)> = by_tag.into_iter().collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    let mut groups: Vec<ItemGroup> = keyed.into_iter().map(|(_, group)| group).collect();
    if !without_tags.is_empty() {
        groups.push(ItemGroup { title: Arc::from(untagged),
                                items: without_tags, });
    }
    groups
}

// ── Footer totals
// ─────────────────────────────────────────────────────────────

/// Formats `bytes` as a human-readable size string (GB or MB).
#[must_use]
pub fn format_total_size(items: &[LibraryItem]) -> String {
    let total_mb: f64 = items.iter().map(|i| i.size_mb).sum();
    if total_mb >= 1024.0 {
        format!("{:.1} GB", total_mb / 1024.0)
    }
    else {
        format!("{:.0} MB", total_mb)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use dtrpg_core::data::library::LibraryItem;

    use super::*;

    fn item(publisher: &str) -> LibraryItem {
        LibraryItem::new("id",
                         "Title",
                         publisher,
                         "",
                         "",
                         "PDF",
                         0,
                         0.0,
                         2020,
                         0,
                         dtrpg_core::data::enums::ItemStatus::Cloud,
                         "#000000",
                         "",
                         None)
    }

    #[test]
    fn single_publisher_returns_one_entry() {
        let items = vec![item("Paizo"), item("Paizo")];
        let entries = publisher_entries(&items);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name.as_ref(), "Paizo");
    }

    #[test]
    fn multiple_publishers_sorted_alphabetically() {
        let items = vec![item("Wizards of the Coast"),
                         item("Paizo"),
                         item("Kobold Press"),];
        let entries = publisher_entries(&items);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_ref()).collect();
        assert_eq!(names, ["Kobold Press", "Paizo", "Wizards of the Coast"]);
    }

    #[test]
    fn sort_is_case_insensitive() {
        let items = vec![item("B Publisher"), item("a publisher")];
        let entries = publisher_entries(&items);
        assert_eq!(entries[0].name.as_ref(), "a publisher");
        assert_eq!(entries[1].name.as_ref(), "B Publisher");
    }

    #[test]
    fn alphabetical_order_is_independent_of_count() {
        let mut items = Vec::new();
        for _ in 0..50 {
            items.push(item("Zyborg Games"));
        }
        items.push(item("Aaeon Press"));
        let entries = publisher_entries(&items);
        assert_eq!(entries[0].name.as_ref(), "Aaeon Press");
        assert_eq!(entries[1].name.as_ref(), "Zyborg Games");
    }

    // ── Grouping by tag
    // ───────────────────────────────────────────────────────────

    fn titled(id: &str) -> LibraryItem {
        let mut item = item("Paizo");
        item.id = Arc::from(id);
        item
    }

    fn library(tags_by_item: &[(&str, &[&str])]) -> TagLibrary {
        TagLibrary::new(tags_by_item.iter()
                                    .map(|(id, names)| {
                                        ((*id).to_owned(),
                                         names.iter().map(|n| (*n).to_owned()).collect())
                                    })
                                    .collect(),
                        HashMap::new())
    }

    fn shape(groups: &[ItemGroup]) -> Vec<(String, Vec<String>)> {
        groups.iter()
              .map(|group| {
                  (group.title.to_string(), group.items.iter().map(|i| i.id.to_string()).collect())
              })
              .collect()
    }

    #[test]
    fn an_item_is_in_the_group_of_each_of_its_tags() {
        let tags = library(&[("a", &["solo", "horror"]), ("b", &["solo"])]);

        let groups = group_by_tag(vec![titled("a"), titled("b")], &tags, "Untagged");

        assert_eq!(shape(&groups),
                   [("horror".to_owned(), vec!["a".to_owned()]),
                    ("solo".to_owned(), vec!["a".to_owned(), "b".to_owned()])]);
    }

    #[test]
    fn untagged_items_are_last_and_only_when_there_are_some() {
        let tags = library(&[("a", &["solo"])]);

        let with = group_by_tag(vec![titled("b"), titled("a")], &tags, "Untagged");
        let without = group_by_tag(vec![titled("a")], &tags, "Untagged");

        assert_eq!(shape(&with),
                   [("solo".to_owned(), vec!["a".to_owned()]),
                    ("Untagged".to_owned(), vec!["b".to_owned()])]);
        assert_eq!(without.len(), 1);
    }

    #[test]
    fn tags_are_ordered_without_regard_to_case_and_keep_the_order_of_the_items() {
        let tags = library(&[("a", &["beta"]), ("b", &["Alpha"]), ("c", &["alpha"])]);

        let groups = group_by_tag(vec![titled("c"), titled("a"), titled("b")],
                                  &tags,
                                  "Untagged");

        assert_eq!(groups[0].items.iter().map(|i| &*i.id).collect::<Vec<_>>(),
                   ["c", "b"]);
        assert_eq!(&*groups[1].title, "beta");
    }
}
