//! Facet (aggregate) queries backing the sidebar's publisher list and
//! section counts — the ADR 0003 pattern of answering count questions with
//! SQL instead of a full-catalog in-memory scan.
//!
//! Issue #355 (tasks 3-4 of `catalog-reads-from-index`): the catalog-load and
//! per-item status-change paths fetched every item
//! (`LibraryController::all_items()`) just to count them per publisher and
//! per smart section.

use std::sync::Arc;

use rusqlite::Connection;

use super::items;
use crate::data::enums::ItemStatus;
use crate::data::index::IndexError;
use crate::data::library::SectionCounts;

/// Returns each publisher with its item count — the index-backed equivalent
/// of `dtrpg_ui::util::publisher::publisher_entries` over a full-catalog
/// fetch.
///
/// Ordered by lowercased name, like the Rust scan. The sort happens here
/// rather than via SQL `COLLATE NOCASE` because SQLite's `NOCASE` only folds
/// ASCII and would order non-ASCII publisher names differently than
/// `to_lowercase()`. The SQL's `ORDER BY publisher` pins the pre-sort order
/// so the stable lowercased sort is deterministic even for names differing
/// only by case.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if the query fails.
pub(super) fn publisher_counts(conn: &Connection) -> Result<Vec<(Arc<str>, usize)>, IndexError> {
    let mut stmt = conn
        .prepare("SELECT publisher, COUNT(*) FROM items GROUP BY publisher ORDER BY publisher")?;
    let mut counts: Vec<(Arc<str>, usize)> = stmt.query_map([], |row| {
                                                     let name: String = row.get(0)?;
                                                     let count: i64 = row.get(1)?;
                                                     Ok((Arc::from(name), count as usize))
                                                 })?
                                                 .collect::<Result<_, _>>()?;
    counts.sort_by_key(|(name, _)| name.to_lowercase());
    Ok(counts)
}

/// The sidebar section counts for the whole catalog, composing the existing
/// facet queries (`count_all`, `count_by_status`, `count_recently_updated`)
/// — the index-backed equivalent of `crate::data::library::section_counts`
/// over a full-catalog fetch.
///
/// # Errors
///
/// Returns [`IndexError::Sqlite`] if any constituent query fails.
pub(super) fn section_counts(conn: &Connection, now_secs: i64, window_days: u32)
                             -> Result<SectionCounts, IndexError> {
    Ok(SectionCounts { all:              items::count_all(conn)?,
                       recently_updated: items::count_recently_updated(conn,
                                                                       now_secs,
                                                                       window_days)?,
                       on_device:        items::count_by_status(conn, ItemStatus::Downloaded)?,
                       in_cloud:         items::count_by_status(conn, ItemStatus::Cloud)?, })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
// Index fixtures use unwrap for known-good in-memory database operations.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::data::index::LibraryIndex;
    use crate::data::library;

    fn item(id: &str, publisher: &str, status: ItemStatus, date_added: Option<i64>)
            -> library::LibraryItem {
        library::LibraryItem::new(id, "Title", publisher, "", "", "PDF", 0, 0.0, 2020, 0, status,
                                  "#000000", "", date_added)
    }

    fn seeded() -> (Vec<library::LibraryItem>, LibraryIndex) {
        let items = vec![item("a", "Paizo", ItemStatus::Downloaded, Some(1_700_000_000)),
                         item("b", "paizo", ItemStatus::Cloud, Some(1_000_000_000)),
                         item("c",
                              "Angry Goblin Games",
                              ItemStatus::Cloud,
                              Some(1_768_000_000)),
                         item("d", "Angry Goblin Games", ItemStatus::Downloaded, None),
                         // Recently updated via `date_updated` while its
                         // `date_added` is stale: pins the COALESCE/.or()
                         // fallback priority the two sides share.
                         {
                             let mut e =
                                 item("e", "Paizo", ItemStatus::Downloaded, Some(1_000_000_000));
                             e.date_updated = Some(1_769_000_000);
                             e
                         },];
        let mut index = LibraryIndex::open_in_memory().unwrap();
        index.replace_all_items(&items).unwrap();
        (items, index)
    }

    #[test]
    fn publisher_counts_group_and_order_case_insensitively() {
        let (_, index) = seeded();
        let counts = index.publisher_counts().unwrap();
        let names: Vec<&str> = counts.iter().map(|(n, _)| n.as_ref()).collect();
        assert_eq!(names, ["Angry Goblin Games", "Paizo", "paizo"]);
        assert_eq!(counts[0].1, 2);
        assert_eq!(counts[1].1, 2);
        assert_eq!(counts[2].1, 1);
    }

    #[test]
    fn publisher_counts_on_an_empty_index_is_empty() {
        let index = LibraryIndex::open_in_memory().unwrap();
        assert!(index.publisher_counts().unwrap().is_empty());
    }

    #[test]
    fn section_counts_equals_the_full_catalog_scan() {
        // The identity bar for the swap: same numbers, whatever the fixture,
        // as the pure scan the controller used to run. The fixture exercises
        // both recency paths (date_updated priority, date_added fallback) so
        // a wrong recency rule can't pass with two zeroes.
        let (items, index) = seeded();
        let now = 1_770_000_000;
        let window = 30;
        let scanned = library::section_counts(&items, now, window);
        assert_eq!(scanned.recently_updated, 2, "fixture must exercise recency");
        let queried = index.section_counts(now, window).unwrap();
        assert_eq!(queried, scanned);
    }

    #[test]
    fn section_counts_on_an_empty_index_is_zeroed() {
        let index = LibraryIndex::open_in_memory().unwrap();
        assert_eq!(index.section_counts(1_770_000_000, 30).unwrap(),
                   SectionCounts::default());
    }
}
