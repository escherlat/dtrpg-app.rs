// Tests for this module.

use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;

fn test_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("dtrpg_cache_test_{name}"))
}

fn make_item(id: &str) -> LibraryItem {
    LibraryItem::new(id,
                     "Test Title",
                     "Test Publisher",
                     "",
                     "Core",
                     "PDF",
                     100,
                     10.0,
                     2024,
                     1,
                     ItemStatus::Cloud,
                     "#1C2A44",
                     "Desc.",
                     None)
}

#[test]
fn load_missing_file_returns_none() {
    let dir = std::path::PathBuf::from("/nonexistent/dtrpg_cache_surely_missing");
    assert!(load_catalog_cache(&dir).is_none());
}

#[test]
fn load_malformed_json_returns_none() {
    let dir = test_dir("malformed");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(CATALOG_CACHE_FILE), b"not valid json { ").unwrap();
    let result = load_catalog_cache(&dir);
    let _ = fs::remove_dir_all(&dir);
    assert!(result.is_none());
}

#[test]
fn fresh_metadata_is_not_stale() {
    let meta = CacheMetadata { saved_at_secs:
                                   SystemTime::now().duration_since(UNIX_EPOCH)
                                                    .unwrap()
                                                    .as_secs(),
                               item_count:                      10,
                               schema_version:                  CACHE_SCHEMA_VERSION,
                               last_item_check_batch_secs:      None,
                               last_fresh_install_request_secs: None, };
    assert!(!meta.is_stale());
}

#[test]
fn old_metadata_is_stale() {
    let meta = CacheMetadata { saved_at_secs:                   0, // epoch — very old
                               item_count:                      10,
                               schema_version:                  CACHE_SCHEMA_VERSION,
                               last_item_check_batch_secs:      None,
                               last_fresh_install_request_secs: None, };
    assert!(meta.is_stale());
}

#[test]
fn fresh_metadata_with_stale_schema_version_is_stale() {
    // Regression: a cache saved before `cover_url` was populated
    // correctly must not be trusted as fresh just because it's recent —
    // it silently disabled thumbnail loading for up to 7 days otherwise.
    let meta = CacheMetadata { saved_at_secs:
                                   SystemTime::now().duration_since(UNIX_EPOCH)
                                                    .unwrap()
                                                    .as_secs(),
                               item_count:                      10,
                               schema_version:                  CACHE_SCHEMA_VERSION - 1,
                               last_item_check_batch_secs:      None,
                               last_fresh_install_request_secs: None, };
    assert!(meta.is_stale());
}

#[test]
fn metadata_without_schema_version_field_defaults_to_stale() {
    // Regression: metadata JSON written before `schema_version` existed
    // has no such key; `#[serde(default)]` must decode it as `0`, which
    // never matches `CACHE_SCHEMA_VERSION` and is always stale.
    let json = r#"{"saved_at_secs": 9999999999, "item_count": 10}"#;
    let meta: CacheMetadata = serde_json::from_str(json).unwrap();
    assert_eq!(meta.schema_version, 0);
    assert!(meta.is_stale());
}

#[test]
fn missing_metadata_returns_none() {
    let dir = std::path::PathBuf::from("/nonexistent/dtrpg_meta_surely_missing");
    assert!(load_cache_metadata(&dir).is_none());
}

#[test]
fn save_cache_writes_metadata() {
    let dir = test_dir("metadata_roundtrip");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let items = vec![make_item("b1"), make_item("b2"), make_item("b3")];
    save_catalog_cache(&dir, &items).unwrap();

    let meta = load_cache_metadata(&dir);
    let _ = fs::remove_dir_all(&dir);

    let meta = meta.unwrap();
    assert_eq!(meta.item_count, 3);
    assert!(!meta.is_stale());
}

#[test]
fn ensure_cache_metadata_exists_writes_a_stale_default_when_missing() {
    let dir = test_dir("ensure_metadata_missing");
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&dir);

    ensure_cache_metadata_exists(&dir);
    let meta = load_cache_metadata(&dir);
    let _ = fs::remove_dir_all(&dir);

    let meta = meta.unwrap();
    assert_eq!(meta.item_count, 0);
    assert!(meta.is_stale());
}

#[test]
fn ensure_cache_metadata_exists_leaves_a_real_file_untouched() {
    let dir = test_dir("ensure_metadata_present");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let items = vec![make_item("b1")];
    save_catalog_cache(&dir, &items).unwrap();

    ensure_cache_metadata_exists(&dir);
    let meta = load_cache_metadata(&dir);
    let _ = fs::remove_dir_all(&dir);

    let meta = meta.unwrap();
    assert_eq!(meta.item_count, 1);
    assert!(!meta.is_stale());
}

#[test]
fn save_then_load_round_trips() {
    let dir = test_dir("roundtrip");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let items = vec![make_item("a1"), make_item("a2")];
    save_catalog_cache(&dir, &items).unwrap();

    let loaded = load_catalog_cache(&dir);
    let _ = fs::remove_dir_all(&dir);

    let loaded = loaded.unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].id.as_ref(), "a1");
    assert_eq!(loaded[1].id.as_ref(), "a2");
}

#[test]
fn load_dedupes_stale_repeated_file_records() {
    // Regression: a cache written before file records were deduplicated
    // by id (see `LibraryItem::dedupe_files`) can have the same file
    // repeated in `files`; loading it must clean this up rather than
    // making the detail tab's item list select every row at once.
    use crate::data::library::LibraryItemFile;

    let dir = test_dir("dedupe_on_load");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut item = make_item("b1");
    item.files = vec![LibraryItemFile { id:         "1234".into(),
                                        index:      0,
                                        name:       "Moria Rulebook".into(),
                                        format:     "PDF".into(),
                                        size_mb:    1.0,
                                        downloaded: false,
                                        offloaded:  false, },
                      LibraryItemFile { id:         "1234".into(),
                                        index:      0,
                                        name:       "Moria Rulebook".into(),
                                        format:     "PDF".into(),
                                        size_mb:    1.0,
                                        downloaded: false,
                                        offloaded:  false, },];
    // Bypass `save_catalog_cache` (which would go through the current,
    // already-deduped write path) to simulate JSON written by an older
    // build that had not yet deduplicated file records.
    fs::write(dir.join(CATALOG_CACHE_FILE),
              serde_json::to_string(&vec![item]).unwrap()).unwrap();

    let loaded = load_catalog_cache(&dir);
    let _ = fs::remove_dir_all(&dir);

    let loaded = loaded.unwrap();
    assert_eq!(loaded[0].files.len(), 1);
    assert!(!loaded[0].is_multi_item());
}

#[test]
fn load_dedupes_whole_items_repeated_verbatim() {
    // Regression: a catalog-sync bug re-appended the same cached page
    // into an already-populated in-memory catalog on every load, which
    // then got written straight back to disk — ballooning a 2,000-item
    // library into tens of thousands of duplicate entries across
    // repeated launches. Loading an already-bloated cache must collapse
    // it back down rather than requiring a manual cache wipe.
    let dir = test_dir("dedupe_whole_items_on_load");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let item = make_item("c1");
    fs::write(dir.join(CATALOG_CACHE_FILE),
              serde_json::to_string(&vec![item.clone(), item.clone(), item]).unwrap()).unwrap();

    let loaded = load_catalog_cache(&dir);
    let _ = fs::remove_dir_all(&dir);

    let loaded = loaded.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id.as_ref(), "c1");
}

#[test]
fn load_dedupes_republished_items_by_product_id() {
    // A republish mints a new `id` but keeps the same `product_id` (the
    // catalog SKU) — see the matching comment on `reconcile_catalog` in
    // `controllers::library`. An already-corrupted cache holding both
    // the stale and the new id for the same product must collapse to
    // one entry, keeping the first occurrence.
    let dir = test_dir("dedupe_by_product_id_on_load");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut old_item = make_item("old-id");
    old_item.product_id = 42;
    let mut new_item = make_item("new-id");
    new_item.product_id = 42;
    fs::write(dir.join(CATALOG_CACHE_FILE),
              serde_json::to_string(&vec![old_item, new_item]).unwrap()).unwrap();

    let loaded = load_catalog_cache(&dir);
    let _ = fs::remove_dir_all(&dir);

    let loaded = loaded.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id.as_ref(), "old-id");
}

#[test]
fn load_does_not_dedupe_distinct_items_with_unknown_product_id() {
    // `product_id == 0` is a "SKU unknown" sentinel, not a real shared
    // identity — two distinct items that both happen to have it must
    // not collapse into one just because their product ids match.
    let dir = test_dir("no_dedupe_unknown_product_id");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let item_a = make_item("a1");
    let item_b = make_item("a2");
    assert_eq!(item_a.product_id, 0);
    assert_eq!(item_b.product_id, 0);
    fs::write(dir.join(CATALOG_CACHE_FILE),
              serde_json::to_string(&vec![item_a, item_b]).unwrap()).unwrap();

    let loaded = load_catalog_cache(&dir);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(loaded.unwrap().len(), 2);
}
