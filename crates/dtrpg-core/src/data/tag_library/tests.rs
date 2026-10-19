//! Tests for the library's tags as the UI reads them.

#![allow(clippy::unwrap_used)]

use super::*;
use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;

fn tags(entries: &[(&str, &[&str])]) -> HashMap<String, Vec<String>> {
    entries.iter()
           .map(|(id, names)| ((*id).to_owned(), names.iter().map(|n| (*n).to_owned()).collect()))
           .collect()
}

fn library() -> TagLibrary {
    TagLibrary::new(tags(&[("a", &["Beta", "solo"]), ("b", &["solo"]), ("c", &[])]),
                    HashMap::from([("solo".to_owned(), TagColor::Red)]))
}

#[test]
fn every_tag_is_listed_once_with_its_count_and_color_ordered_ignoring_case() {
    let entries = library().entries().to_vec();

    assert_eq!(entries,
               [TagEntry { name:  "Beta".into(),
                           color: None,
                           count: 1, },
                TagEntry { name:  "solo".into(),
                           color: Some(TagColor::Red),
                           count: 2, }]);
}

#[test]
fn an_items_tags_are_its_own() {
    let library = library();

    let names: Vec<_> = library.tags_of("a")
                               .iter()
                               .map(|t| t.name.to_string())
                               .collect();
    assert_eq!(names, ["Beta", "solo"]);
    assert_eq!(library.tags_of("a")[1].color, Some(TagColor::Red));
    assert!(library.tags_of("c").is_empty());
    assert!(library.tags_of("missing").is_empty());
}

#[test]
fn the_items_with_a_tag_are_found_ignoring_case() {
    let library = library();

    let mut ids: Vec<String> = library.items_with("SOLO")
                                      .iter()
                                      .map(|id| id.to_string())
                                      .collect();
    ids.sort();
    assert_eq!(ids, ["a", "b"]);
    assert!(library.items_with("nothing").is_empty());
}

#[test]
fn a_tag_and_its_color_are_found_ignoring_case() {
    let library = library();

    assert_eq!(library.entry("SOLO").unwrap().count, 2);
    assert_eq!(library.color_of("Solo"), Some(TagColor::Red));
    assert_eq!(library.color_of("beta"), None);
    assert!(library.entry("nothing").is_none());
}

#[test]
fn a_library_with_no_tags_is_empty() {
    assert!(TagLibrary::default().is_empty());
    assert!(TagLibrary::new(tags(&[("a", &[])]), HashMap::new()).is_empty());
    assert!(!library().is_empty());
}

#[test]
fn the_library_is_read_from_the_index() {
    let mut index = LibraryIndex::open_in_memory().unwrap();
    let item = |id: &str| {
        LibraryItem::new(id,
                         "Title",
                         "Free League",
                         "",
                         "Core",
                         "PDF",
                         100,
                         10.0,
                         2024,
                         0,
                         ItemStatus::Cloud,
                         "#1C2A44",
                         "A game.",
                         None)
    };
    index.replace_all_items(&[item("a"), item("b")]).unwrap();
    index.add_tag("a", "solo").unwrap();
    index.add_tag("b", "solo").unwrap();
    index.set_tag_color("solo", Some(TagColor::Blue)).unwrap();

    let library = TagLibrary::load(&index).unwrap();

    assert_eq!(library.entry("solo").unwrap().count, 2);
    assert_eq!(library.color_of("solo"), Some(TagColor::Blue));
}
