//! Download location configuration: root path preference and per-item path
//! derivation.
//!
//! This module governs only where downloaded catalog files are stored on disk.
//! It is intentionally decoupled from application cache/metadata (see
//! [`crate::data::paths::cache_dir`]) and from preferences (see
//! [`crate::data::paths::app_preferences_dir`]) — those live in fixed,
//! non-user-facing locations regardless of where the user chooses to store
//! downloads.
//!
//! Multi-root storage and placement: the original single configured root
//! (`override_path`, or the platform default download directory when unset)
//! is reserved as root id `0`, named "Default", and keeps its original
//! behavior and persisted format unchanged -- an existing `storage.toml`
//! with no `extra_roots`/`rules` keys deserializes and behaves exactly as it
//! did before this module supported more than one root. Additional named
//! roots (`extra_roots`) and an ordered list of placement rules
//! (`rules`) are purely additive: see [`placement`] and [`roots`].

pub mod placement;
pub mod roots;

use std::path::{Path, PathBuf};

use placement::{PlacementCondition, PlacementRule};
use roots::StorageRoot;
use serde::{Deserialize, Serialize};

use crate::data::constants::{
    RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS, RECENTLY_UPDATED_WINDOW_MAX_DAYS,
    RECENTLY_UPDATED_WINDOW_MIN_DAYS,
};
use crate::data::paths::{app_preferences_dir, default_download_dir};

/// Reserved id for the original single root (`override_path`/platform
/// default), always present and never reassignable to an `extra_roots`
/// entry.
pub const DEFAULT_ROOT_ID: u32 = 0;

// ── StorageError
// ──────────────────────────────────────────────────────────────

/// Errors that can occur when validating or applying a storage path.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The specified path does not exist on disk.
    #[error("path does not exist: {0}")]
    PathDoesNotExist(PathBuf),
    /// The application cannot write to the specified path.
    #[error("path is not writable: {0}")]
    NotWritable(PathBuf),
    /// The volume containing the path appears to be unavailable.
    #[error("volume may be unavailable: {0}")]
    VolumeUnavailable(PathBuf),
}

// ── validate_writable
// ─────────────────────────────────────────────────────────

/// Validates `path` for writability by attempting a probe write.
///
/// Creates a temporary file inside `path` and immediately removes it.
/// This catches permission issues, read-only mounts, and full volumes.
///
/// # Errors
///
/// Returns [`StorageError::PathDoesNotExist`] if the directory is absent,
/// [`StorageError::VolumeUnavailable`] if the path cannot be stat'd, or
/// [`StorageError::NotWritable`] if the probe write fails.
pub fn validate_writable(path: &Path) -> Result<(), StorageError> {
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Err(StorageError::PathDoesNotExist(path.to_path_buf())),
        Err(_) => return Err(StorageError::VolumeUnavailable(path.to_path_buf())),
    }
    let probe = path.join(".dtrpg_write_probe");
    std::fs::write(&probe, b"probe").map_err(|_| StorageError::NotWritable(path.to_path_buf()))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

// ── StorageConfig
// ─────────────────────────────────────────────────────────────

/// Default number of concurrent thumbnail/download fetches when no override
/// has been saved.
pub const DEFAULT_MAX_CONCURRENT_DOWNLOADS: usize = 3;

fn default_max_concurrent_downloads() -> usize {
    DEFAULT_MAX_CONCURRENT_DOWNLOADS
}

fn default_recently_updated_window_days() -> u32 {
    RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS
}

/// Clamps `days` to `[RECENTLY_UPDATED_WINDOW_MIN_DAYS,
/// RECENTLY_UPDATED_WINDOW_MAX_DAYS]`.
fn clamp_recently_updated_window_days(days: u32) -> u32 {
    days.clamp(RECENTLY_UPDATED_WINDOW_MIN_DAYS,
               RECENTLY_UPDATED_WINDOW_MAX_DAYS)
}

#[derive(Serialize, Deserialize)]
struct StorageConfigFile {
    root_path:                    Option<String>,
    #[serde(default = "default_max_concurrent_downloads")]
    max_concurrent_downloads:     usize,
    #[serde(default)]
    create_collections:           bool,
    #[serde(default = "default_recently_updated_window_days")]
    recently_updated_window_days: u32,
    /// Named storage roots beyond the default (`root_path`). Empty for
    /// every config file written before multi-root support existed.
    #[serde(default)]
    extra_roots:                  Vec<StorageRoot>,
    /// Ordered placement rules. Empty for every config file written
    /// before placement rules existed.
    #[serde(default)]
    rules:                        Vec<PlacementRule>,
    /// Next id to assign to an added root (`extra_roots` ids start at 1;
    /// `0` is reserved for the default root). Persisted so a removed
    /// root's id is never reassigned to a new one -- a stale rule
    /// referencing a removed root's id simply stops matching, rather than
    /// matching whatever new root later reuses that id.
    #[serde(default = "default_next_root_id")]
    next_root_id:                 u32,
}

fn default_next_root_id() -> u32 {
    1
}

/// Manages the root directory where downloaded catalog files are stored, and
/// the shared thumbnail/download concurrency limit.
///
/// Persists the user's chosen overrides in
/// `{app_preferences_dir}/storage.toml`. Falls back to the platform default
/// download directory (e.g. `~/Downloads/dtrpg`) when no root path override is
/// set, to [`DEFAULT_MAX_CONCURRENT_DOWNLOADS`] when no concurrency override
/// is set, and to [`RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS`] when no
/// "Recently Updated window" override is set.
pub struct StorageConfig {
    override_path:                Option<PathBuf>,
    max_concurrent_downloads:     usize,
    create_collections:           bool,
    recently_updated_window_days: u32,
    /// Named storage roots beyond the default (id [`DEFAULT_ROOT_ID`]).
    extra_roots:                  Vec<StorageRoot>,
    /// Ordered placement rules; see [`placement::resolve_root_id`].
    rules:                        Vec<PlacementRule>,
    /// Next id to assign to an added root.
    next_root_id:                 u32,
}

impl StorageConfig {
    /// Loads the storage config from disk. Returns a default-path config on any
    /// error.
    ///
    /// If no config file exists yet (first run), writes the resolved
    /// defaults to disk immediately, so `storage.toml` always exists after
    /// the app has started once rather than only appearing the first time a
    /// setting is changed. A file that exists but fails to parse is left
    /// untouched — that's a different, more cautious case than "never
    /// written," and silently overwriting it could destroy whatever is
    /// there.
    pub fn load() -> Self {
        let path = config_path();
        let file_existed = path.as_ref().is_some_and(|p| p.exists());
        let file = path.as_ref()
                       .and_then(|p| std::fs::read_to_string(p).ok())
                       .and_then(|text| toml::from_str::<StorageConfigFile>(&text).ok());
        let override_path = file.as_ref()
                                .and_then(|cfg| cfg.root_path.clone())
                                .map(PathBuf::from);
        let max_concurrent_downloads = file.as_ref().map_or(DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                                                            |cfg| cfg.max_concurrent_downloads);
        let recently_updated_window_days = file.as_ref()
                                               .map_or(RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS, |cfg| {
                                                   cfg.recently_updated_window_days
                                               });
        let create_collections = file.as_ref().is_some_and(|cfg| cfg.create_collections);
        let extra_roots = file.as_ref()
                              .map(|cfg| cfg.extra_roots.clone())
                              .unwrap_or_default();
        let rules = file.as_ref()
                        .map(|cfg| cfg.rules.clone())
                        .unwrap_or_default();
        let next_root_id = file.as_ref().map_or(1, |cfg| cfg.next_root_id);
        let config = Self { override_path,
                            max_concurrent_downloads,
                            create_collections,
                            recently_updated_window_days,
                            extra_roots,
                            rules,
                            next_root_id };
        if !file_existed {
            config.save();
        }
        config
    }

    /// Builds an in-memory config pinned to `root`, without touching the real
    /// on-disk `storage.toml` — neither reading nor writing it.
    ///
    /// For tests only. [`load`](Self::load) followed by
    /// [`set_root_path`](Self::set_root_path) both reads and persists to the
    /// real, shared config file at [`config_path`], which corrupts a
    /// developer's actual settings the moment `cargo test` runs; this
    /// constructor exists so tests can exercise path-derivation logic without
    /// that side effect.
    ///
    /// Not `#[cfg(test)]`-gated: dtrpg-ui's own tests construct a
    /// `StorageConfig` across the crate boundary, and `cfg(test)` only
    /// applies when the crate it's written in is itself under test.
    pub fn for_test(root: PathBuf) -> Self {
        Self { override_path:                Some(root),
               max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
               create_collections:           false,
               recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
               extra_roots:                  Vec::new(),
               rules:                        Vec::new(),
               next_root_id:                 1, }
    }

    /// Returns the resolved download root (saved override, or platform
    /// default).
    pub fn root_path(&self) -> PathBuf {
        self.override_path
            .clone()
            .unwrap_or_else(default_download_dir)
    }

    /// Returns `true` when the download root has been verified accessible on
    /// disk.
    pub fn is_accessible(&self) -> bool {
        self.root_path().try_exists().unwrap_or(false)
    }

    /// Returns `true` when no user override is set — `root_path()` resolves to
    /// the platform default download directory.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.override_path.is_none()
    }

    /// Creates the resolved root directory (and any missing parents) if it does
    /// not already exist.
    ///
    /// Only meaningful to call unconditionally for the platform default path —
    /// for a user-chosen override, a missing directory more likely means an
    /// unmounted volume than a fresh install, so callers should not blindly
    /// recreate it there.
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error if directory creation fails (e.g.
    /// permissions).
    pub fn ensure_root_exists(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.root_path())
    }

    /// Derives the directory an item's files are stored in, grouped by
    /// publisher.
    ///
    /// Maps to `{root}/items/{sanitized publisher}/` — see [`publisher_dir`].
    pub fn path_for_publisher(&self, publisher: &str) -> PathBuf {
        publisher_dir(&self.root_path(), publisher)
    }

    /// Derives the directory a collection's symlinked files live in.
    ///
    /// Maps to `{root}/collections/{sanitized collection name}/` — see
    /// [`collection_dir`].
    pub fn path_for_collection(&self, collection_name: &str) -> PathBuf {
        collection_dir(&self.root_path(), collection_name)
    }

    // ── Multi-root storage and placement
    // ───────────────────────────────────

    /// Returns every configured storage root: the default root (id
    /// [`DEFAULT_ROOT_ID`], named "Default") first, followed by any
    /// additional roots in the order they were added.
    #[must_use]
    pub fn roots(&self) -> Vec<StorageRoot> {
        let mut all = Vec::with_capacity(1 + self.extra_roots.len());
        all.push(StorageRoot { id:   DEFAULT_ROOT_ID,
                               name: "Default".to_string(),
                               path: self.root_path(), });
        all.extend(self.extra_roots.iter().cloned());
        all
    }

    /// Returns the root with the given id, if one is configured.
    #[must_use]
    pub fn root(&self, id: u32) -> Option<StorageRoot> {
        self.roots().into_iter().find(|r| r.id == id)
    }

    /// Adds a new named storage root at `path` and returns its assigned id.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if `path` fails
    /// [`validate_writable`].
    pub fn add_root(&mut self, name: String, path: PathBuf) -> Result<u32, StorageError> {
        validate_writable(&path)?;
        let id = self.next_root_id;
        self.next_root_id += 1;
        self.extra_roots.push(StorageRoot { id, name, path });
        self.save();
        Ok(id)
    }

    /// Removes the storage root with the given id.
    ///
    /// A no-op if `id` is [`DEFAULT_ROOT_ID`] (the default root cannot be
    /// removed, only its path changed via [`Self::set_root_path`]) or does
    /// not match any configured root. Any placement rule targeting the
    /// removed root is left in place -- it simply stops matching, since
    /// [`Self::resolve_root_for`] falls back to the default root when a
    /// rule's target id no longer resolves to a configured root.
    pub fn remove_root(&mut self, id: u32) {
        if id == DEFAULT_ROOT_ID {
            return;
        }
        let before = self.extra_roots.len();
        self.extra_roots.retain(|r| r.id != id);
        if self.extra_roots.len() != before {
            self.save();
        }
    }

    /// Returns the configured placement rules, in evaluation order.
    #[must_use]
    pub fn rules(&self) -> &[PlacementRule] {
        &self.rules
    }

    /// Appends a new placement rule to the end of the evaluation order.
    pub fn add_rule(&mut self, condition: PlacementCondition, target_root_id: u32) {
        self.rules.push(PlacementRule { condition,
                                        target_root_id });
        self.save();
    }

    /// Removes the rule at `index`, if one exists there.
    pub fn remove_rule(&mut self, index: usize) {
        if index < self.rules.len() {
            self.rules.remove(index);
            self.save();
        }
    }

    /// Moves the rule at `index` earlier in evaluation order, swapping it
    /// with its predecessor. A no-op if `index` is `0` or out of range.
    pub fn move_rule_up(&mut self, index: usize) {
        if index > 0 && index < self.rules.len() {
            self.rules.swap(index, index - 1);
            self.save();
        }
    }

    /// Moves the rule at `index` later in evaluation order, swapping it
    /// with its successor. A no-op if `index` is the last rule or out of
    /// range.
    pub fn move_rule_down(&mut self, index: usize) {
        if index + 1 < self.rules.len() {
            self.rules.swap(index, index + 1);
            self.save();
        }
    }

    /// Resolves which root a file should be placed in: the target of the
    /// first matching placement rule (see [`placement::resolve_root_id`]),
    /// falling back to the default root when no rule matches or a
    /// matching rule's target id no longer resolves to a configured root
    /// (e.g. the root it pointed at was removed).
    #[must_use]
    pub fn resolve_root_for(&self, publisher: &str, collection_ids: &[u64], size_mb: f64)
                            -> StorageRoot {
        let default_root = StorageRoot { id:   DEFAULT_ROOT_ID,
                                         name: "Default".to_string(),
                                         path: self.root_path(), };
        placement::resolve_root_id(&self.rules, publisher, collection_ids, size_mb)
            .and_then(|id| self.root(id))
            .unwrap_or(default_root)
    }

    /// Derives the directory an item's files are stored in under `root`,
    /// grouped by publisher. See [`publisher_dir`].
    #[must_use]
    pub fn path_for_publisher_in(root: &StorageRoot, publisher: &str) -> PathBuf {
        publisher_dir(&root.path, publisher)
    }

    /// Saves `path` as the new storage root override and updates the in-memory
    /// state.
    ///
    /// Creates parent directories as needed. Silently ignores I/O errors during
    /// save (the path is still applied in memory).
    pub fn set_root_path(&mut self, path: PathBuf) {
        self.override_path = Some(path);
        self.save();
    }

    /// Removes the root path override and reverts to the platform default on
    /// next `root_path()` call. The concurrency limit is unaffected.
    pub fn clear_override(&mut self) {
        self.override_path = None;
        self.save();
    }

    /// Returns the configured maximum number of concurrent thumbnail/download
    /// fetches.
    #[must_use]
    pub fn max_concurrent_downloads(&self) -> usize {
        self.max_concurrent_downloads
    }

    /// Saves `n` as the new shared thumbnail/download concurrency limit and
    /// updates the in-memory state.
    pub fn set_max_concurrent_downloads(&mut self, n: usize) {
        self.max_concurrent_downloads = n;
        self.save();
    }

    /// Returns `true` when completing a download should also create a
    /// symlink for the item under `{root}/collections/{collection name}/`
    /// for each collection it belongs to.
    #[must_use]
    pub fn create_collections(&self) -> bool {
        self.create_collections
    }

    /// Saves `enabled` as the new "Create collections" setting and updates
    /// the in-memory state.
    pub fn set_create_collections(&mut self, enabled: bool) {
        self.create_collections = enabled;
        self.save();
    }

    /// Returns the configured "Recently Updated window", in days.
    #[must_use]
    pub fn recently_updated_window_days(&self) -> u32 {
        self.recently_updated_window_days
    }

    /// Saves `days` (clamped to `[RECENTLY_UPDATED_WINDOW_MIN_DAYS,
    /// RECENTLY_UPDATED_WINDOW_MAX_DAYS]`) as the new "Recently Updated
    /// window" and updates the in-memory state.
    ///
    /// Clamped here rather than only in the settings UI's stepper, so a
    /// hand-edited `storage.toml` with an out-of-range value can never
    /// propagate past this setter.
    pub fn set_recently_updated_window_days(&mut self, days: u32) {
        self.recently_updated_window_days = clamp_recently_updated_window_days(days);
        self.save();
    }

    /// Writes the current in-memory state to
    /// `{app_preferences_dir}/storage.toml`. Silently ignores I/O errors
    /// (the state remains applied in memory).
    fn save(&self) {
        let cfg = StorageConfigFile { root_path:
                                          self.override_path
                                              .as_ref()
                                              .map(|p| p.to_string_lossy().into_owned()),
                                      max_concurrent_downloads:     self.max_concurrent_downloads,
                                      create_collections:           self.create_collections,
                                      recently_updated_window_days:
                                          self.recently_updated_window_days,
                                      extra_roots:                  self.extra_roots.clone(),
                                      rules:                        self.rules.clone(),
                                      next_root_id:                 self.next_root_id, };
        let Some(config_file) = config_path()
        else {
            return;
        };
        if let Some(parent) = config_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = toml::to_string(&cfg) {
            let _ = std::fs::write(&config_file, text);
        }
    }
}

// ── Helpers
// ───────────────────────────────────────────────────────────────────

fn config_path() -> Option<PathBuf> {
    Some(app_preferences_dir().join("storage.toml"))
}

/// Returns free disk space at the current storage root, in megabytes.
///
/// Returns `None` on any I/O error (e.g. an unmounted volume or unsupported
/// filesystem) — callers should treat that identically to "space is
/// sufficient" rather than blocking a download on an unrelated probe
/// failure.
#[must_use]
pub fn available_space_mb() -> Option<f64> {
    fs4::available_space(StorageConfig::load().root_path()).ok()
                                                           .map(|bytes| bytes as f64 / 1_048_576.0)
}

/// Derives the directory a downloaded item's files live in:
/// `{root}/items/{sanitized publisher}/`, grouping every item from the same
/// publisher together under a common `items/` directory.
///
/// Sanitizes `publisher` first so a name containing a path separator can
/// never escape `root` or be (mis)treated as an absolute path component —
/// see [`sanitize_path_component`]. Every call site that needs an item's
/// on-disk directory (downloads, "Open", "Reveal in Finder") must go through
/// this function rather than reimplementing the join inline.
pub fn publisher_dir(root: &Path, publisher: &str) -> PathBuf {
    root.join("items").join(sanitize_path_component(publisher))
}

/// Derives the directory a collection's symlinked files live in:
/// `{root}/collections/{sanitized collection name}/`.
///
/// Sanitizes `collection_name` first so a name containing a path separator
/// can never escape `root` or be (mis)treated as an absolute path
/// component — see [`sanitize_path_component`].
pub fn collection_dir(root: &Path, collection_name: &str) -> PathBuf {
    root.join("collections")
        .join(sanitize_path_component(collection_name))
}

/// Strips a leading path separator, replaces any remaining path separators,
/// and converts spaces to underscores, so the result can never be (or
/// contain) an absolute path component when joined onto another path and
/// reads as a single filesystem-friendly token.
pub(crate) fn sanitize_path_component(value: &str) -> String {
    value.trim_start_matches('/').replace(['/', ' '], "_")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn default_path_is_non_empty() {
        let cfg = StorageConfig { override_path:                None,
                                  max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                                  create_collections:           false,
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                                  extra_roots:                  Vec::new(),
                                  rules:                        Vec::new(),
                                  next_root_id:                 1, };
        assert!(!cfg.is_default());
    }

    #[test]
    fn ensure_root_exists_creates_missing_directory() {
        let root =
            std::env::temp_dir().join(format!("dtrpg-test-ensure-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cfg = StorageConfig { override_path:                Some(root.clone()),
                                  max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                                  create_collections:           false,
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
        let cfg =
            StorageConfigFile { root_path:                    None,
                                max_concurrent_downloads:     DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                                create_collections:           true,
                                recently_updated_window_days: RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
                                  recently_updated_window_days:
                                      RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
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
}
