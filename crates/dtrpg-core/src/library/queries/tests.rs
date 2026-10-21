//! Core-level tests for the filtered/grouped query cache: ADR 0002 Slice 3.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::SystemTime;

use super::ViewState;
use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;
use crate::data::storage::StorageConfig;
use crate::library::Library;

fn item(id: &str, publisher: &str) -> LibraryItem {
    LibraryItem::new(id,
                     "Title",
                     publisher,
                     "",
                     "Core",
                     "PDF",
                     0,
                     0.0,
                     2020,
                     0,
                     ItemStatus::Cloud,
                     "#000000",
                     "",
                     None)
}

fn test_storage_config() -> StorageConfig {
    StorageConfig { override_path:                None,
                    max_concurrent_downloads:
                        crate::data::storage::DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                    create_collections:           false,
                    recently_updated_window_days:
                        crate::data::constants::RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                    extra_roots:                  Vec::new(),
                    rules:                        Vec::new(),
                    next_root_id:                 1, }
}

fn open_library() -> Arc<Library> {
    let dir = tempfile::tempdir().expect("tempdir");
    let (library, _work_events, _sync_events) =
        Library::open(dir.keep(), &test_storage_config()).expect("open library");
    Arc::new(library)
}

#[test]
fn invalidate_cache_populates_items_filter_count_and_catalog_ids() {
    let library = open_library();
    library.index
           .lock()
           .unwrap()
           .replace_all_items(&[item("a", "Paizo"), item("b", "Paizo")])
           .unwrap();

    library.invalidate_cache(&ViewState::default(), 0, "Untagged");

    assert_eq!(library.visible_items().len(), 2);
    assert_eq!(library.visible_items_count(), 2);
    assert_eq!(library.filter_count(), 2);
    assert_eq!(library.section_counts().all, 2);
    assert_eq!(library.publishers().len(), 1);
}

#[test]
fn visible_items_grouped_is_empty_without_a_group_by() {
    let library = open_library();
    library.index
           .lock()
           .unwrap()
           .replace_all_items(&[item("a", "Paizo")])
           .unwrap();

    library.invalidate_cache(&ViewState::default(), 0, "Untagged");

    assert!(library.visible_items_grouped().is_empty());
}

#[test]
fn note_thumbnail_attempt_patches_the_cache_without_recomputing_it() {
    let library = open_library();
    library.index
           .lock()
           .unwrap()
           .replace_all_items(&[item("a", "Paizo")])
           .unwrap();
    library.invalidate_cache(&ViewState::default(), 0, "Untagged");

    let before = Arc::as_ptr(&library.visible_items());
    let at = SystemTime::now();
    library.note_thumbnail_attempt(&Arc::from("a"), at);
    let after = Arc::as_ptr(&library.visible_items());

    assert_eq!(before, after,
               "the result set was rebuilt, not just patched");
    let patched = library.visible_items();
    assert_eq!(patched[0].thumbnail_last_attempted, Some(at));
}

#[test]
fn item_by_id_overlays_ephemeral_state() {
    let library = open_library();
    library.index
           .lock()
           .unwrap()
           .replace_all_items(&[item("a", "Paizo")])
           .unwrap();
    let at = SystemTime::now();
    library.note_availability_check(&Arc::from("a"), at);

    let found = library.item_by_id("a").expect("item present");
    assert_eq!(found.availability_last_checked, Some(at));
}

#[test]
fn tags_round_trip_through_set_and_get() {
    let library = open_library();
    assert_eq!(library.tags().entries().len(), 0);

    let tags = crate::data::tag_library::TagLibrary::new([("a".to_owned(),
                                                           vec!["solo".to_owned()])].into(),
                                                         Default::default());
    library.set_tags(tags);

    assert_eq!(library.tags().entries().len(), 1);
}
