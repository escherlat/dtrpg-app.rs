//! The equivalence bar of `openspec/changes/search-box-semantics-sql`: for a
//! battery of search-box queries, the SQL search/tag dimensions must return
//! exactly what the old pipeline (unsearched query, then the Rust `Tag` id-set
//! pass, then the `SearchQuery::matches` pass over snapshot-shaped tags)
//! returned — same survivors, same order.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::super::LibraryIndex;
use super::clause::ItemFilter;
use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;
use crate::data::tag_color::TagColor;
use crate::util::search::SearchQuery;
use crate::util::sort::{SortDirection, SortMethod};

fn item(id: &str, title: &str, publisher: &str, line: &str, status: ItemStatus) -> LibraryItem {
    LibraryItem::new(id,
                     title,
                     publisher,
                     line,
                     "Core",
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
    let items = vec![item("t1",
                          "Moria Dungeon Handbook",
                          "Gamelords",
                          "Adventures",
                          ItemStatus::Cloud),
                     item("t2",
                          "The Path of Éowyn",
                          "Angry Goblin Games",
                          "Realms",
                          ItemStatus::Cloud),
                     item("t3",
                          "Twin  Spaces   Book",
                          "paizo",
                          "Misc",
                          ItemStatus::Downloaded),
                     item("t4",
                          "HÖRRAH Sourcebook",
                          "Paizo",
                          "Misc",
                          ItemStatus::Downloaded),
                     item("t5",
                          "Solo Ops Manual",
                          "Zenith",
                          "Adventures",
                          ItemStatus::Cloud),];
    index.replace_all_items(&items).unwrap();
    let tagging: [(&str, &[&str]); 4] = [("t1", &["Solo", "horror"]),
                                         ("t2", &["To Run"]),
                                         ("t4", &["HÖRRAH"]),
                                         ("t5", &["solo", "To Run"])];
    for (id, tags) in tagging {
        let tags = tags.iter()
                       .map(|t| (t.to_string(), Option::<TagColor>::None))
                       .collect::<Vec<_>>();
        index.set_item_tags(id, &tags, 1).unwrap();
    }
    index
}

/// What the pre-#353 controller had in memory: item id -> tag names, as the
/// `TagLibrary` snapshot served them.
type Snapshot = HashMap<String, Vec<Arc<str>>>;

fn snapshot_tags(index: &LibraryIndex) -> Snapshot {
    index.all_item_tags()
         .unwrap()
         .into_iter()
         .map(|(id, names)| (id, names.into_iter().map(Arc::from).collect()))
         .collect()
}

/// The old pipeline, verbatim: sidebar dimensions in SQL, `Tag` as a Rust
/// id-set pass, search as a Rust `matches` pass, in that order.
fn old_pipeline(index: &LibraryIndex, snapshot: &Snapshot, status: Option<ItemStatus>,
                tag_name: Option<&str>, query: &SearchQuery)
                -> Vec<String> {
    let sidebar = ItemFilter { status,
                               ..ItemFilter::default() };
    let mut matched =
        index.list_items_filtered(&sidebar, SortMethod::Title, SortDirection::Ascending)
             .unwrap();
    if let Some(name) = tag_name {
        let wanted = name.to_lowercase();
        let set: HashSet<String> =
            snapshot.iter()
                    .filter(|(_, names)| names.iter().any(|tag| tag.to_lowercase() == wanted))
                    .map(|(id, _)| id.clone())
                    .collect();
        matched.retain(|i| set.contains(i.id.as_ref()));
    }
    matched.iter()
           .filter(|i| query.matches(i, snapshot.get(i.id.as_ref()).map_or(&[], Vec::as_slice)))
           .map(|i| i.id.to_string())
           .collect()
}

/// The new pipeline: everything in one SQL query.
fn new_pipeline(index: &LibraryIndex, status: Option<ItemStatus>, tag_name: Option<&str>,
                query: &SearchQuery)
                -> Vec<String> {
    let sidebar = ItemFilter { status,
                               tag_name,
                               search: Some(query),
                               ..ItemFilter::default() };
    index.list_items_filtered(&sidebar, SortMethod::Title, SortDirection::Ascending)
         .unwrap()
         .into_iter()
         .map(|i| i.id.to_string())
         .collect()
}

/// `(label, typed query, sidebar status, tag section)` — the battery covers
/// the shapes the matcher cares about: word-crossing substrings (which FTS5
/// token matching cannot express), Unicode case folds (which SQLite's
/// ASCII-only `lower()` cannot do), every `tag:`/`-tag:` syntax variant,
/// text x tag combinations, and the `Tag` sidebar dimension.
const CASES: &[(&str, &str, Option<ItemStatus>, Option<&str>)] =
    &[("empty", "", None, None),
      ("whitespace", "   ", None, None),
      ("word-crossing substring", "ia d", None, None),
      ("umlaut substring via tag", "örr", None, None),
      ("near miss of umlaut", "orr", None, None),
      ("unicode upper query", "ÉOWYN", None, None),
      ("unicode lower query", "hörrah", None, None),
      ("publisher substring", "goblin", None, None),
      ("line substring", "adventures", None, None),
      ("tag substring via text", "run", None, None),
      ("tag: required", "tag:solo", None, None),
      ("TAG: case", "TAG:Solo", None, None),
      ("-tag: excluded", "-tag:solo", None, None),
      ("quoted tag", "tag:\"to run\"", None, None),
      ("unquoted split", "tag:to run", None, None),
      ("text and tag", "solo tag:horror", None, None),
      ("bare tag: is nothing", "tag:", None, None),
      ("tagged is text", "tagged", None, None),
      ("status x search", "solo", Some(ItemStatus::Cloud), None),
      ("tag section", "", None, Some("Solo")),
      ("tag section x search", "source", None, Some("HÖRRAH")),
      ("tag section x text", "solo", None, Some("solo")),
      ("everything excluded", "eowyn -tag:solo tag:solo", None, None)];

#[test]
fn search_parity_over_the_full_battery() {
    let index = seeded();
    let snapshot = snapshot_tags(&index);
    for (label, typed, status, tag_name) in CASES {
        let query = SearchQuery::parse(typed);
        let old = old_pipeline(&index, &snapshot, *status, *tag_name, &query);
        let new = new_pipeline(&index, *status, *tag_name, &query);
        assert_eq!(new, old, "parity broken for {label} ({typed:?})");
        assert!(!old.is_empty()
                || matches!(*typed,
                            "   " | "orr" | "tag:to run" | "tagged" | "eowyn -tag:solo tag:solo"),
                "case {label} should match something: {new:?} vs {old:?}");
    }
}

#[test]
fn each_battery_case_narrows_or_partitions_meaningfully() {
    // Guard the battery itself: every search term is expected to hit, and the
    // tag sections partition, so a regression in the fixture (or a silently
    // always-true predicate) cannot pass the parity test by matching nothing.
    let index = seeded();
    let all = index.list_items_filtered(&ItemFilter::default(),
                                        SortMethod::Title,
                                        SortDirection::Ascending)
                   .unwrap();
    assert_eq!(all.len(), 5);
    let mut seen_hits = HashMap::new();
    for (label, typed, _, _) in CASES {
        let query = SearchQuery::parse(typed);
        let hits = new_pipeline(&index, None, None, &query);
        seen_hits.insert(*label, hits.len());
    }
    for expect in [("word-crossing substring", 1),
                   ("umlaut substring via tag", 1),
                   ("unicode upper query", 1),
                   ("tag: required", 2),
                   ("quoted tag", 2),
                   ("status x search", 2)]
    {
        assert_eq!(seen_hits.get(expect.0),
                   Some(&expect.1),
                   "fixture drifted for {}",
                   expect.0);
    }
}

/// The `Collection` sidebar dimension renders `IN` lists while the search
/// dimensions render numbered `?n` placeholders in the same statement — the
/// combination never exercised by the CASES battery, and the one where
/// clause-text order vs. argument-push order could silently drift.
#[test]
fn collection_dimension_composes_with_search() {
    let mut index = LibraryIndex::open_in_memory().unwrap();
    let mut a = item("c1", "Alpha Source", "Pub", "Line", ItemStatus::Cloud);
    a.product_id = 7;
    let mut b = item("c2", "Beta Source", "Pub", "Line", ItemStatus::Cloud);
    b.product_id = 7;
    let mut c = item("c3", "Gamma Source", "Pub", "Line", ItemStatus::Cloud);
    c.order_product_id = 7;
    index.replace_all_items(&[a, b, c]).unwrap();
    let snapshot = snapshot_tags(&index);

    let members: HashSet<u64> = [7].into_iter().collect();
    // Text that hits every member row plus a `-tag:` that nothing carries:
    // both numbered placeholder shapes coexist with the `IN` lists, and the
    // result stays non-empty so the assertion has teeth.
    let query = SearchQuery::parse("source -tag:missing");

    // Old pipeline: unsearched SQL (collection dimension, the only pre-#353
    // dimension here rendered as `IN` lists) then the Rust `matches` pass.
    let sidebar = ItemFilter { collection_member_ids: Some(&members),
                               ..ItemFilter::default() };
    let old =
        index.list_items_filtered(&sidebar, SortMethod::Title, SortDirection::Ascending)
             .unwrap()
             .iter()
             .filter(|i| query.matches(i, snapshot.get(i.id.as_ref()).map_or(&[], Vec::as_slice)))
             .map(|i| i.id.to_string())
             .collect::<Vec<_>>();
    assert!(!old.is_empty(), "fixture must exercise the combination");

    let searched = ItemFilter { collection_member_ids: Some(&members),
                                search: Some(&query),
                                ..ItemFilter::default() };
    let new = index.list_items_filtered(&searched, SortMethod::Title, SortDirection::Ascending)
                   .unwrap()
                   .into_iter()
                   .map(|i| i.id.to_string())
                   .collect::<Vec<_>>();
    assert_eq!(new, old);

    // An empty member set matches nothing, deterministically in BOTH queries
    // (the `1 = 0` clause, not an `IN ()` syntax error with divergent
    // list/count error paths).
    let none: HashSet<u64> = HashSet::new();
    let empty = ItemFilter { collection_member_ids: Some(&none),
                             ..ItemFilter::default() };
    assert_eq!(index.count_items_filtered(&empty).unwrap(), 0);
    assert!(index.list_items_filtered(&empty, SortMethod::Title, SortDirection::Ascending)
                 .unwrap()
                 .is_empty());
}

#[test]
fn count_ignores_search_but_not_other_dimensions() {
    let index = seeded();
    let snapshot = snapshot_tags(&index);
    let unsearched = ItemFilter::default();
    assert_eq!(index.count_items_filtered(&unsearched).unwrap(), 5);

    let cloud = ItemFilter { status: Some(ItemStatus::Cloud),
                             ..ItemFilter::default() };
    // `filter_count` semantics: section size with search ignored.
    assert_eq!(index.count_items_filtered(&cloud).unwrap(),
               old_pipeline(&index,
                            &snapshot,
                            Some(ItemStatus::Cloud),
                            None,
                            &SearchQuery::parse("")).len());

    let query = SearchQuery::parse("solo");
    let searched = ItemFilter { search: Some(&query),
                                ..ItemFilter::default() };
    let searched_len =
        index.list_items_filtered(&searched, SortMethod::Title, SortDirection::Ascending)
             .unwrap()
             .len();
    assert!(searched_len < index.count_items_filtered(&unsearched).unwrap(),
            "fixture must let search narrow below the section count");
}
