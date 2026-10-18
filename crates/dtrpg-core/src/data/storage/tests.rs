//! Unit tests for storage configuration.

#![allow(clippy::unwrap_used)] // Test fixtures intentionally use unwrap for concise assertions.

use std::path::{Path, PathBuf};

use super::config::*;
use super::paths::{collection_dir, publisher_dir, validate_writable};
use super::placement::{PlacementCondition, PlacementRule};
use super::roots::StorageRoot;
use crate::data::constants::{
    RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS, RECENTLY_UPDATED_WINDOW_MAX_DAYS,
    RECENTLY_UPDATED_WINDOW_MIN_DAYS,
};

#[cfg(test)]
#[test]
fn default_path_is_non_empty() {
    let cfg = StorageConfig { override_path:                None,
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    let path = cfg.root_path();
    assert!(path.components().count() > 0);
    assert!(path.ends_with("dtrpg"));
}

#[test]
fn override_path_is_returned_when_set() {
    let custom = PathBuf::from("/tmp/custom-storage");
    let cfg = StorageConfig { override_path:                Some(custom.clone()),
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    assert_eq!(cfg.root_path(), custom);
}

#[test]
fn path_for_publisher_is_under_root() {
    let cfg = StorageConfig { override_path:                Some(PathBuf::from("/tmp/dtrpg")),
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    let publisher_path = cfg.path_for_publisher("Chaosium");
    assert_eq!(publisher_path, Path::new("/tmp/dtrpg/items/Chaosium"));
}

#[test]
fn path_for_publisher_sanitizes_a_name_containing_a_path_separator() {
    // A publisher name is untrusted display text from the API; if it ever
    // contained a path separator, `PathBuf::join` would otherwise let it
    // escape `root` (or discard it entirely if the name looked absolute).
    let cfg = StorageConfig { override_path:                Some(PathBuf::from("/tmp/dtrpg")),
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    let publisher_path = cfg.path_for_publisher("/Evil/Publisher");
    assert_eq!(publisher_path, Path::new("/tmp/dtrpg/items/Evil_Publisher"));
}

#[test]
fn path_for_publisher_converts_spaces_to_underscores() {
    let cfg = StorageConfig { override_path:                Some(PathBuf::from("/tmp/dtrpg")),
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    let publisher_path = cfg.path_for_publisher("The Forge Studios");
    assert_eq!(publisher_path,
               Path::new("/tmp/dtrpg/items/The_Forge_Studios"));
}

#[test]
fn publisher_dir_never_escapes_root_for_an_absolute_looking_name() {
    let dir = publisher_dir(Path::new("/tmp/dtrpg"), "/Evil/Publisher");
    assert!(dir.starts_with("/tmp/dtrpg"));
}

#[test]
fn validate_writable_succeeds_on_temp_dir() {
    let dir = std::env::temp_dir();
    assert!(validate_writable(&dir).is_ok());
}

#[test]
fn validate_writable_fails_on_missing_path() {
    let missing = PathBuf::from("/nonexistent/surely/missing/path");
    assert!(matches!(validate_writable(&missing),
                     Err(StorageError::PathDoesNotExist(_))));
}

#[test]
fn is_default_true_without_override() {
    let cfg = StorageConfig { override_path:                None,
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    assert!(cfg.is_default());
}

#[test]
fn is_default_false_with_override() {
    let cfg = StorageConfig { override_path:                Some(PathBuf::from("/tmp/custom-storage")),
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    assert!(!cfg.is_default());
}

#[test]
fn ensure_root_exists_creates_missing_directory() {
    let root = std::env::temp_dir().join(format!("dtrpg-test-ensure-root-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let cfg = StorageConfig { override_path:                Some(root.clone()),
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    assert!(!cfg.is_accessible());
    cfg.ensure_root_exists().unwrap();
    assert!(cfg.is_accessible());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn ensure_root_exists_is_idempotent_on_existing_directory() {
    let dir = std::env::temp_dir();
    let cfg = StorageConfig { override_path:                Some(dir),
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    assert!(cfg.ensure_root_exists().is_ok());
    assert!(cfg.ensure_root_exists().is_ok());
}

#[test]
fn max_concurrent_downloads_defaults_to_three() {
    let cfg = StorageConfig { override_path:                None,
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    assert_eq!(cfg.max_concurrent_downloads(), 3);
}

#[test]
fn missing_max_concurrent_downloads_field_deserializes_to_default() {
    let file: StorageConfigFile = toml::from_str("").unwrap();
    assert_eq!(file.max_concurrent_downloads,
               DEFAULT_MAX_CONCURRENT_DOWNLOADS);
}

#[test]
fn create_collections_defaults_to_false() {
    let cfg = StorageConfig { override_path:                None,
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    assert!(!cfg.create_collections());
}

#[test]
fn missing_create_collections_field_deserializes_to_default() {
    let file: StorageConfigFile = toml::from_str("").unwrap();
    assert!(!file.create_collections);
}

#[test]
fn create_collections_round_trips_through_toml_serialization() {
    let cfg = StorageConfigFile { root_path:                    None,
                                  max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                                  create_collections:           true,
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                                  extra_roots:                  Vec::new(),
                                  rules:                        Vec::new(),
                                  next_root_id:                 1, };
    let text = toml::to_string(&cfg).unwrap();
    let reloaded: StorageConfigFile = toml::from_str(&text).unwrap();
    assert!(reloaded.create_collections);
}

#[test]
fn path_for_collection_is_under_root() {
    let cfg = StorageConfig { override_path:                Some(PathBuf::from("/tmp/dtrpg")),
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           true,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    let collection_path = cfg.path_for_collection("Adventures");
    assert_eq!(collection_path,
               Path::new("/tmp/dtrpg/collections/Adventures"));
}

#[test]
fn collection_dir_sanitizes_a_name_containing_a_path_separator() {
    let dir = collection_dir(Path::new("/tmp/dtrpg"), "/Evil/Collection");
    assert_eq!(dir, Path::new("/tmp/dtrpg/collections/Evil_Collection"));
}

#[test]
fn collection_dir_never_escapes_root_for_an_absolute_looking_name() {
    let dir = collection_dir(Path::new("/tmp/dtrpg"), "/Evil/Collection");
    assert!(dir.starts_with("/tmp/dtrpg"));
}

#[test]
fn recently_updated_window_days_defaults_to_thirty() {
    let cfg = StorageConfig { override_path:                None,
                              max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                              create_collections:           false,
                              recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                              extra_roots:                  Vec::new(),
                              rules:                        Vec::new(),
                              next_root_id:                 1, };
    assert_eq!(cfg.recently_updated_window_days(), 30);
}

#[test]
fn missing_recently_updated_window_days_field_deserializes_to_default() {
    let file: StorageConfigFile = toml::from_str("").unwrap();
    assert_eq!(file.recently_updated_window_days,
               RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS);
}

#[test]
fn clamp_recently_updated_window_days_clamps_below_minimum() {
    assert_eq!(clamp_recently_updated_window_days(1),
               RECENTLY_UPDATED_WINDOW_MIN_DAYS);
}

#[test]
fn clamp_recently_updated_window_days_clamps_above_maximum() {
    assert_eq!(clamp_recently_updated_window_days(365),
               RECENTLY_UPDATED_WINDOW_MAX_DAYS);
}

#[test]
fn clamp_recently_updated_window_days_passes_through_in_range_values() {
    assert_eq!(clamp_recently_updated_window_days(45), 45);
}

fn config_with(extra_roots: Vec<StorageRoot>, rules: Vec<PlacementRule>) -> StorageConfig {
    StorageConfig { override_path: Some(PathBuf::from("/tmp/dtrpg-default")),
                    max_concurrent_downloads: DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                    create_collections: false,
                    recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                    extra_roots,
                    rules,
                    next_root_id: 1 }
}

fn extra_root(id: u32, name: &str) -> StorageRoot {
    StorageRoot { id,
                  name: name.to_string(),
                  path: PathBuf::from(format!("/tmp/{name}")) }
}

#[test]
fn roots_always_includes_the_default_root_first() {
    let cfg = config_with(vec![extra_root(1, "External")], Vec::new());
    let roots = cfg.roots();
    assert_eq!(roots.len(), 2);
    assert_eq!(roots[0].id, DEFAULT_ROOT_ID);
    assert_eq!(roots[0].name, "Default");
    assert_eq!(roots[1].id, 1);
}

#[test]
fn root_finds_an_existing_root_by_id() {
    let cfg = config_with(vec![extra_root(1, "External")], Vec::new());
    assert_eq!(cfg.root(1).unwrap().name, "External");
    assert_eq!(cfg.root(DEFAULT_ROOT_ID).unwrap().name, "Default");
}

#[test]
fn root_returns_none_for_an_unknown_id() {
    let cfg = config_with(Vec::new(), Vec::new());
    assert!(cfg.root(99).is_none());
}

#[test]
fn resolve_root_for_falls_back_to_default_with_no_rules() {
    let cfg = config_with(vec![extra_root(1, "External")], Vec::new());
    let root = cfg.resolve_root_for("Free League", &[], 10.0);
    assert_eq!(root.id, DEFAULT_ROOT_ID);
}

#[test]
fn resolve_root_for_honors_a_matching_publisher_rule() {
    let cfg = config_with(
                          vec![extra_root(1, "External")],
                          vec![
        PlacementRule { condition:      PlacementCondition::Publisher("Free League".to_string()),
                        target_root_id: 1, },
    ],
    );
    let root = cfg.resolve_root_for("Free League", &[], 10.0);
    assert_eq!(root.id, 1);
    let fallback = cfg.resolve_root_for("Chaosium", &[], 10.0);
    assert_eq!(fallback.id, DEFAULT_ROOT_ID);
}

#[test]
fn resolve_root_for_falls_back_to_default_when_rule_targets_a_removed_root() {
    // Simulates a rule left pointing at a root id that no longer exists
    // (e.g. the root was removed): the rule still "matches" the
    // condition, but its target doesn't resolve to a configured root,
    // so resolution falls back to the default rather than panicking or
    // silently matching a different root that reused the id.
    let cfg = config_with(
                          Vec::new(),
                          vec![
        PlacementRule { condition:      PlacementCondition::Publisher("Free League".to_string()),
                        target_root_id: 7, },
    ],
    );
    let root = cfg.resolve_root_for("Free League", &[], 10.0);
    assert_eq!(root.id, DEFAULT_ROOT_ID);
}

#[test]
fn path_for_publisher_in_derives_under_the_given_root() {
    let root = extra_root(1, "External");
    let path = StorageConfig::path_for_publisher_in(&root, "Chaosium");
    assert_eq!(path, Path::new("/tmp/External/items/Chaosium"));
}
