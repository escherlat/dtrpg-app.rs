//! Tests for the library search box.

use super::*;
use crate::data::enums::ItemStatus;

fn item(title: &str, publisher: &str, line: &str) -> LibraryItem {
    LibraryItem::new("a",
                     title,
                     publisher,
                     line,
                     "Core",
                     "PDF",
                     100,
                     10.0,
                     2024,
                     0,
                     ItemStatus::Cloud,
                     "#000000",
                     "",
                     None)
}

fn tags(names: &[&str]) -> Vec<Arc<str>> {
    names.iter().map(|name| Arc::from(*name)).collect()
}

fn matches(query: &str, item: &LibraryItem, names: &[&str]) -> bool {
    SearchQuery::parse(query).matches(item, &tags(names))
}

#[test]
fn an_empty_search_matches_everything() {
    let moria = item("Moria", "Free League", "The One Ring");

    assert!(matches("", &moria, &[]));
}

#[test]
fn plain_text_matches_title_publisher_and_line_ignoring_case() {
    let moria = item("Moria", "Free League", "The One Ring");

    assert!(matches("moria", &moria, &[]));
    assert!(matches("FREE", &moria, &[]));
    assert!(matches("one ring", &moria, &[]));
    assert!(!matches("starfinder", &moria, &[]));
}

#[test]
fn plain_text_is_kept_exactly_as_typed() {
    let moria = item("Moria", "Free  League", "");

    // Two spaces in the publisher, matched by two spaces in the search.
    assert!(matches("free  league", &moria, &[]));
}

#[test]
fn plain_text_also_matches_a_tag_name() {
    let moria = item("Moria", "Free League", "");

    assert!(matches("solo", &moria, &["Solo play"]));
    assert!(!matches("solo", &moria, &["co-op"]));
}

#[test]
fn a_tag_term_keeps_only_the_items_with_the_tag() {
    let moria = item("Moria", "Free League", "");

    assert!(matches("tag:solo", &moria, &["Solo"]));
    assert!(!matches("tag:solo", &moria, &["co-op"]));
    assert!(!matches("tag:solo", &moria, &[]));
}

#[test]
fn a_tag_term_matches_the_whole_tag_name_not_part_of_it() {
    let moria = item("Moria", "Free League", "");

    assert!(!matches("tag:sol", &moria, &["solo"]));
}

#[test]
fn a_negated_tag_term_drops_the_items_with_the_tag() {
    let moria = item("Moria", "Free League", "");

    assert!(!matches("-tag:solo", &moria, &["solo"]));
    assert!(matches("-tag:solo", &moria, &["co-op"]));
    assert!(matches("-tag:solo", &moria, &[]));
}

#[test]
fn every_tag_term_must_hold() {
    let moria = item("Moria", "Free League", "");

    assert!(matches("tag:solo tag:printed", &moria, &["printed", "solo"]));
    assert!(!matches("tag:solo tag:printed", &moria, &["solo"]));
    assert!(matches("tag:solo -tag:printed", &moria, &["solo"]));
    assert!(!matches("tag:solo -tag:printed", &moria, &["solo", "printed"]));
}

#[test]
fn a_tag_name_with_spaces_is_quoted() {
    let moria = item("Moria", "Free League", "");

    assert!(matches("tag:\"to run\"", &moria, &["To Run"]));
    assert!(matches("-tag:\"to run\"", &moria, &["solo"]));
    assert!(!matches("tag:\"to run\"", &moria, &["to"]));
}

#[test]
fn a_quote_that_is_never_closed_runs_to_the_end() {
    let moria = item("Moria", "Free League", "");

    assert!(matches("tag:\"to run", &moria, &["to run"]));
}

#[test]
fn a_tag_term_is_read_ignoring_case_and_combines_with_text() {
    let moria = item("Moria", "Free League", "");
    let ring = item("The One Ring", "Free League", "");

    assert!(matches("TAG:Solo moria", &moria, &["solo"]));
    assert!(!matches("tag:solo moria", &ring, &["solo"]));
    assert!(matches("free tag:solo", &ring, &["solo"]));
}

#[test]
fn a_tag_term_with_no_name_asks_for_nothing() {
    let moria = item("Moria", "Free League", "");

    assert!(!SearchQuery::parse("tag:").has_tag_terms());
    assert!(matches("tag: moria", &moria, &[]));
    assert!(matches("tag:\"\"", &moria, &[]));
}

#[test]
fn only_a_search_with_a_tag_term_needs_the_items_tags() {
    assert!(SearchQuery::parse("tag:solo").has_tag_terms());
    assert!(SearchQuery::parse("moria -tag:solo").has_tag_terms());
    assert!(!SearchQuery::parse("moria").has_tag_terms());
    assert!(!SearchQuery::parse("tagged").has_tag_terms());
}
