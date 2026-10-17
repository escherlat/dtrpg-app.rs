//! Data model, filtering, sorting, and stub catalog for the Libri library view.

use std::collections::HashSet;

use crate::data::collection::CollectionEntry;
use crate::data::library::LibraryItem;

// ── Sorting
// ───────────────────────────────────────────────────────────────────

/// Field used to sort the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMethod {
    #[default]
    Title,
    Publisher,
    DateAdded,
    PageCount,
    /// Sort driven by a column header click; carries the column key string.
    Custom {
        col_key: &'static str,
    },
}

/// Direction for catalog sorting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

/// Sorts `items` in place according to `method` and `direction`.
pub fn sort_items(items: &mut [LibraryItem], method: SortMethod, direction: SortDirection) {
    match method {
        SortMethod::Title => items.sort_by(|a, b| a.title.cmp(&b.title)),
        SortMethod::Publisher => items.sort_by(|a, b| {
            a.publisher
                .cmp(&b.publisher)
                .then_with(|| a.title.cmp(&b.title))
        }),
        SortMethod::DateAdded => items.sort_by_key(|i| i.added_order),
        SortMethod::PageCount => {
            items.sort_by(|a, b| a.pages.cmp(&b.pages).then_with(|| a.title.cmp(&b.title)))
        }
        SortMethod::Custom {
            col_key: "publisher",
        } => items.sort_by(|a, b| {
            a.publisher
                .cmp(&b.publisher)
                .then_with(|| a.title.cmp(&b.title))
        }),
        SortMethod::Custom { col_key: "system" } => {
            items.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.title.cmp(&b.title)))
        }
        SortMethod::Custom { col_key: "pages" } => {
            items.sort_by(|a, b| a.pages.cmp(&b.pages).then_with(|| a.title.cmp(&b.title)))
        }
        SortMethod::Custom { col_key: "size" } => items.sort_by(|a, b| {
            a.size_mb
                .partial_cmp(&b.size_mb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.title.cmp(&b.title))
        }),
        SortMethod::Custom { col_key: "added" } => items.sort_by_key(|i| i.added_order),
        SortMethod::Custom { .. } => items.sort_by(|a, b| a.title.cmp(&b.title)),
    }

    if direction == SortDirection::Descending {
        items.reverse();
    }
}

/// Field used to sort the sidebar's Collections list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollectionSortMethod {
    #[default]
    Name,
    DateCreated,
    ItemCount,
}

/// Sorts `entries` in place according to `method` and `direction`.
///
/// `ItemCount` sorts by the same catalog-intersected count
/// `sidebar_view::render_sidebar` computes per row (`member_ids` filtered
/// against `catalog_ids`), not the raw API item count, so the sort order
/// matches what's displayed.
pub fn sort_collections(
    entries: &mut [CollectionEntry], method: CollectionSortMethod, direction: SortDirection,
    catalog_ids: &HashSet<u64>,
) {
    match method {
        CollectionSortMethod::Name => entries.sort_by(|a, b| a.name.cmp(&b.name)),
        CollectionSortMethod::DateCreated => entries.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.name.cmp(&b.name))
        }),
        CollectionSortMethod::ItemCount => entries.sort_by(|a, b| {
            let count_of = |e: &CollectionEntry| {
                e.member_ids
                    .iter()
                    .filter(|id| catalog_ids.contains(id))
                    .count()
            };
            count_of(a)
                .cmp(&count_of(b))
                .then_with(|| a.name.cmp(&b.name))
        }),
    }

    if direction == SortDirection::Descending {
        entries.reverse();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn entry(id: u64, name: &str, created_at: i64, member_ids: &[u64]) -> CollectionEntry {
        CollectionEntry {
            id,
            name: Arc::from(name),
            member_ids: Arc::from(member_ids),
            created_at,
        }
    }

    fn names(entries: &[CollectionEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_ref()).collect()
    }

    #[test]
    fn sorts_by_name_ascending() {
        let mut entries = vec![entry(1, "Zeta", 10, &[]), entry(2, "Alpha", 20, &[])];
        sort_collections(
            &mut entries,
            CollectionSortMethod::Name,
            SortDirection::Ascending,
            &HashSet::new(),
        );
        assert_eq!(names(&entries), vec!["Alpha", "Zeta"]);
    }

    #[test]
    fn sorts_by_name_descending() {
        let mut entries = vec![entry(1, "Zeta", 10, &[]), entry(2, "Alpha", 20, &[])];
        sort_collections(
            &mut entries,
            CollectionSortMethod::Name,
            SortDirection::Descending,
            &HashSet::new(),
        );
        assert_eq!(names(&entries), vec!["Zeta", "Alpha"]);
    }

    #[test]
    fn sorts_by_date_created_ascending() {
        let mut entries = vec![entry(1, "Newer", 200, &[]), entry(2, "Older", 100, &[])];
        sort_collections(
            &mut entries,
            CollectionSortMethod::DateCreated,
            SortDirection::Ascending,
            &HashSet::new(),
        );
        assert_eq!(names(&entries), vec!["Older", "Newer"]);
    }

    #[test]
    fn sorts_by_date_created_descending() {
        let mut entries = vec![entry(1, "Newer", 200, &[]), entry(2, "Older", 100, &[])];
        sort_collections(
            &mut entries,
            CollectionSortMethod::DateCreated,
            SortDirection::Descending,
            &HashSet::new(),
        );
        assert_eq!(names(&entries), vec!["Newer", "Older"]);
    }

    #[test]
    fn date_created_tie_breaks_by_name() {
        let mut entries = vec![entry(1, "Zeta", 100, &[]), entry(2, "Alpha", 100, &[])];
        sort_collections(
            &mut entries,
            CollectionSortMethod::DateCreated,
            SortDirection::Ascending,
            &HashSet::new(),
        );
        assert_eq!(names(&entries), vec!["Alpha", "Zeta"]);
    }

    #[test]
    fn sorts_by_item_count_using_catalog_intersection_ascending() {
        // "Big" has 3 member ids, but only 1 is in the catalog; "Small" has 2
        // member ids, both in the catalog — so by catalog-intersected count,
        // Big (1) sorts before Small (2), even though Big's raw member count
        // is larger.
        let mut entries = vec![
            entry(1, "Big", 10, &[1, 2, 3]),
            entry(2, "Small", 20, &[4, 5]),
        ];
        let catalog_ids: HashSet<u64> = [1u64, 4, 5].into_iter().collect();
        sort_collections(
            &mut entries,
            CollectionSortMethod::ItemCount,
            SortDirection::Ascending,
            &catalog_ids,
        );
        assert_eq!(names(&entries), vec!["Big", "Small"]);
    }

    #[test]
    fn sorts_by_item_count_descending() {
        let mut entries = vec![
            entry(1, "Big", 10, &[1, 2, 3]),
            entry(2, "Small", 20, &[4, 5]),
        ];
        let catalog_ids: HashSet<u64> = [1u64, 2, 3, 4, 5].into_iter().collect();
        sort_collections(
            &mut entries,
            CollectionSortMethod::ItemCount,
            SortDirection::Descending,
            &catalog_ids,
        );
        assert_eq!(names(&entries), vec!["Big", "Small"]);
    }

    #[test]
    fn item_count_tie_breaks_by_name() {
        let mut entries = vec![entry(1, "Zeta", 10, &[1]), entry(2, "Alpha", 20, &[2])];
        let catalog_ids: HashSet<u64> = [1u64, 2].into_iter().collect();
        sort_collections(
            &mut entries,
            CollectionSortMethod::ItemCount,
            SortDirection::Ascending,
            &catalog_ids,
        );
        assert_eq!(names(&entries), vec!["Alpha", "Zeta"]);
    }
}
