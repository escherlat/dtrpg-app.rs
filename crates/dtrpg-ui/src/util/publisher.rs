//! Data model, filtering, sorting, and stub catalog for the Libri library view.

use std::sync::Arc;

use crate::data::library::LibraryItem;

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
    let mut map: std::collections::HashMap<Arc<str>, usize> = std::collections::HashMap::new();
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

/// A publisher group containing its items (already filtered and sorted).
#[derive(Debug, Clone)]
pub struct PublisherGroup {
    pub publisher: Arc<str>,
    pub items:     Vec<LibraryItem>,
}

/// Partitions `items` into publisher groups in the same order as
/// `publisher_entries`.
#[must_use]
pub fn group_by_publisher(items: Vec<LibraryItem>) -> Vec<PublisherGroup> {
    let entries = publisher_entries(&items);
    let mut map: std::collections::HashMap<Arc<str>, Vec<LibraryItem>> =
        std::collections::HashMap::new();
    for item in items {
        map.entry(Arc::clone(&item.publisher))
           .or_default()
           .push(item);
    }
    entries.into_iter()
           .filter_map(|e| {
               map.remove(&e.name)
                  .map(|group_items| PublisherGroup { publisher: e.name,
                                                      items:     group_items, })
           })
           .collect()
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
    use super::*;
    use crate::data::library::LibraryItem;

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
                         crate::data::enums::ItemStatus::Cloud,
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
}
