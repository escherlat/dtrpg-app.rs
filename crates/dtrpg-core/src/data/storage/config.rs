//! Persistent storage configuration and placement resolution.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::paths::{collection_dir, publisher_dir, validate_writable};
use super::placement::{self, PlacementCondition, PlacementRule};
use super::roots::{RootHealth, StorageRoot, root_health};
use crate::data::constants::{
    RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS, RECENTLY_UPDATED_WINDOW_MAX_DAYS,
    RECENTLY_UPDATED_WINDOW_MIN_DAYS,
};
use crate::data::paths::{app_preferences_dir, default_download_dir};

fn config_path() -> Option<PathBuf> {
    Some(app_preferences_dir().join("storage.toml"))
}

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
pub(crate) fn clamp_recently_updated_window_days(days: u32) -> u32 {
    days.clamp(RECENTLY_UPDATED_WINDOW_MIN_DAYS,
               RECENTLY_UPDATED_WINDOW_MAX_DAYS)
}

#[derive(Serialize, Deserialize)]
pub(crate) struct StorageConfigFile {
    pub(crate) root_path:                    Option<String>,
    #[serde(default = "default_max_concurrent_downloads")]
    pub(crate) max_concurrent_downloads:     usize,
    #[serde(default)]
    pub(crate) create_collections:           bool,
    #[serde(default = "default_recently_updated_window_days")]
    pub(crate) recently_updated_window_days: u32,
    /// Named storage roots beyond the default (`root_path`). Empty for
    /// every config file written before multi-root support existed.
    #[serde(default)]
    pub(crate) extra_roots:                  Vec<StorageRoot>,
    /// Ordered placement rules. Empty for every config file written
    /// before placement rules existed.
    #[serde(default)]
    pub(crate) rules:                        Vec<PlacementRule>,
    /// Next id to assign to an added root (`extra_roots` ids start at 1;
    /// `0` is reserved for the default root). Persisted so a removed
    /// root's id is never reassigned to a new one -- a stale rule
    /// referencing a removed root's id simply stops matching, rather than
    /// matching whatever new root later reuses that id.
    #[serde(default = "default_next_root_id")]
    pub(crate) next_root_id:                 u32,
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
    pub(crate) override_path:                Option<PathBuf>,
    pub(crate) max_concurrent_downloads:     usize,
    pub(crate) create_collections:           bool,
    pub(crate) recently_updated_window_days: u32,
    /// Named storage roots beyond the default (id [`DEFAULT_ROOT_ID`]).
    pub(crate) extra_roots:                  Vec<StorageRoot>,
    /// Ordered placement rules; see [`placement::resolve_root_id`].
    pub(crate) rules:                        Vec<PlacementRule>,
    /// Next id to assign to an added root.
    pub(crate) next_root_id:                 u32,
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
        let recently_updated_window_days =
            file.as_ref()
                .map_or(RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS, |cfg| {
                    clamp_recently_updated_window_days(cfg.recently_updated_window_days)
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

    /// Returns the current health of a configured root.
    #[must_use]
    pub fn root_health(&self, id: u32) -> Option<RootHealth> {
        self.root(id).as_ref().map(root_health)
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
