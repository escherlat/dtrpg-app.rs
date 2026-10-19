// Tests for this module.

use super::*;

fn file(id: &str, name: &str) -> LibraryItemFile {
    LibraryItemFile { id:         id.into(),
                      index:      0,
                      name:       name.into(),
                      format:     "PDF".into(),
                      size_mb:    1.0,
                      downloaded: false,
                      offloaded:  false, }
}

#[test]
fn dedupe_files_removes_repeated_ids() {
    let mut item = LibraryItem::new("e1",
                                    "Moria",
                                    "Free League",
                                    "",
                                    "",
                                    "",
                                    0,
                                    0.0,
                                    0,
                                    0,
                                    ItemStatus::Cloud,
                                    "#000000",
                                    "",
                                    None);
    item.files = vec![file("1234", "Moria Rulebook"),
                      file("1234", "Moria Rulebook"),];

    item.dedupe_files();

    assert_eq!(item.files.len(), 1);
    assert!(!item.is_multi_item());
}

fn item_with_files(files: Vec<LibraryItemFile>) -> LibraryItem {
    let mut item = LibraryItem::new("e1",
                                    "Moria",
                                    "Free League",
                                    "",
                                    "",
                                    "",
                                    0,
                                    0.0,
                                    0,
                                    0,
                                    ItemStatus::Cloud,
                                    "#000000",
                                    "",
                                    None);
    item.files = files;
    item
}

fn downloaded_file(id: &str, name: &str) -> LibraryItemFile {
    let mut f = file(id, name);
    f.downloaded = true;
    f
}

#[test]
fn is_offloaded_requires_downloaded_status() {
    let mut item = item_with_files(vec![downloaded_file("1", "A")]);
    item.files[0].offloaded = true;

    assert!(!item.is_offloaded());
    item.status = ItemStatus::Downloaded;
    assert!(item.is_offloaded());
}

#[test]
fn recompute_status_is_cloud_when_no_files_downloaded() {
    let mut item = item_with_files(vec![file("1", "A"), file("2", "B")]);

    item.recompute_status();

    assert_eq!(item.status, ItemStatus::Cloud);
}

#[test]
fn recompute_status_is_cloud_when_only_some_files_downloaded() {
    let mut item = item_with_files(vec![downloaded_file("1", "A"), file("2", "B")]);

    item.recompute_status();

    assert_eq!(item.status, ItemStatus::Cloud);
}

#[test]
fn recompute_status_is_downloaded_once_every_file_is_downloaded() {
    let mut item = item_with_files(vec![downloaded_file("1", "A"), downloaded_file("2", "B")]);

    item.recompute_status();

    assert_eq!(item.status, ItemStatus::Downloaded);
}

#[test]
fn recompute_status_completing_the_last_file_flips_cloud_to_downloaded() {
    let mut item = item_with_files(vec![downloaded_file("1", "A"), file("2", "B")]);
    item.recompute_status();
    assert_eq!(item.status, ItemStatus::Cloud);

    item.files[1].downloaded = true;
    item.recompute_status();

    assert_eq!(item.status, ItemStatus::Downloaded);
}

#[test]
fn recompute_status_is_cloud_for_an_entry_with_no_files() {
    let mut item = item_with_files(vec![]);

    item.recompute_status();

    assert_eq!(item.status, ItemStatus::Cloud);
}

#[test]
fn is_available_defaults_to_true_when_field_is_missing() {
    let json = serde_json::json!({
        "id": "e1",
        "numeric_id": 0,
        "title": "Moria",
        "publisher": "Free League",
        "line": "",
        "kind": "",
        "format": "",
        "pages": 0,
        "size_mb": 0.0,
        "year": 0,
        "added_order": 0,
        "status": "Cloud",
        "color": "#000000",
        "desc": "",
    });

    let item: LibraryItem = match serde_json::from_value(json) {
        Ok(item) => item,
        Err(e) => panic!("deserializes: {e}"),
    };

    assert!(item.is_available);
}

#[test]
fn dedupe_files_keeps_distinct_ids() {
    let mut item = LibraryItem::new("e1",
                                    "Moria",
                                    "Free League",
                                    "",
                                    "",
                                    "",
                                    0,
                                    0.0,
                                    0,
                                    0,
                                    ItemStatus::Cloud,
                                    "#000000",
                                    "",
                                    None);
    item.files = vec![file("1234", "Moria Rulebook"),
                      file("1235", "Moria Map Sheet"),];

    item.dedupe_files();

    assert_eq!(item.files.len(), 2);
    assert!(item.is_multi_item());
}

#[test]
fn dedupe_files_keeps_distinct_files_that_share_an_id() {
    // Regression: the API has been observed to reuse the same download
    // id across genuinely distinct files within a bundle. Deduping on
    // `id` alone would collapse this real 2-file bundle down to 1 and
    // hide its item-count badge — the `name` must differ, not just the
    // id, before two entries are treated as duplicates.
    let mut item = LibraryItem::new("e1",
                                    "Moria",
                                    "Free League",
                                    "",
                                    "",
                                    "",
                                    0,
                                    0.0,
                                    0,
                                    0,
                                    ItemStatus::Cloud,
                                    "#000000",
                                    "",
                                    None);
    item.files = vec![file("1234", "Moria Rulebook"),
                      file("1234", "Moria Map Sheet"),];

    item.dedupe_files();

    assert_eq!(item.files.len(), 2);
    assert!(item.is_multi_item());
}

fn item_with_dates(status: ItemStatus, date_updated: Option<i64>) -> LibraryItem {
    let mut item = LibraryItem::new("e1",
                                    "Moria",
                                    "Free League",
                                    "",
                                    "",
                                    "",
                                    0,
                                    0.0,
                                    0,
                                    0,
                                    status,
                                    "#000000",
                                    "",
                                    None);
    item.date_updated = date_updated;
    item
}

const DAY_SECS: i64 = 24 * 60 * 60;

#[test]
fn section_counts_recently_updated_reflects_only_items_in_window() {
    let now = 1_800_000_000;
    let catalog = vec![item_with_dates(ItemStatus::Downloaded, Some(now - 10 * DAY_SECS)),
                       item_with_dates(ItemStatus::Cloud, Some(now - 40 * DAY_SECS)),];

    let counts = section_counts(&catalog, now, 30);

    assert_eq!(counts.all, 2);
    assert_eq!(counts.recently_updated, 1);
}

#[test]
fn section_counts_recently_updated_respects_a_non_default_window() {
    let now = 1_800_000_000;
    let catalog = vec![item_with_dates(ItemStatus::Downloaded, Some(now - 10 * DAY_SECS)),
                       item_with_dates(ItemStatus::Cloud, Some(now - 40 * DAY_SECS)),];

    // A 7-day window excludes the item that a 30-day default includes.
    let counts = section_counts(&catalog, now, 7);

    assert_eq!(counts.recently_updated, 0);
}
