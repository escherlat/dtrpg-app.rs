use super::*;
use crate::data::summary::ContentSummary;

fn item() -> LibraryItem {
    LibraryItem::new("summary-item",
                     "Summary Item",
                     "Publisher",
                     "",
                     "Core",
                     "PDF",
                     10,
                     1.0,
                     2024,
                     0,
                     ItemStatus::Cloud,
                     "#000000",
                     "",
                     Some(1_700_000_000))
}

#[test]
fn summaries_round_trip_and_refresh_full_text_search() {
    let mut index = LibraryIndex::open_in_memory().unwrap();
    index.replace_all_items(&[item()]).unwrap();
    let summary = ContentSummary { item_id:     "summary-item".to_string(),
                                   text:        "A haunted lighthouse mystery.".to_string(),
                                   model:       "local-extractive".to_string(),
                                   created_at:  1_700_000_001,
                                   token_count: 4, };

    index.save_summary(&summary).unwrap();

    assert_eq!(index.get_summary("summary-item").unwrap(), Some(summary));
    assert_eq!(index.search("lighthouse").unwrap().len(), 1);
}

#[test]
fn missing_summary_is_none() {
    let index = LibraryIndex::open_in_memory().unwrap();

    assert_eq!(index.get_summary("missing").unwrap(), None);
}
