//! Storage configuration: download roots, placement rules, the storage
//! path, concurrency, and the regenerable on-disk caches.

use super::*;

impl SettingsController {
    /// Returns the resolved storage root path.
    pub fn storage_root_path(&self) -> PathBuf {
        self.storage.root_path()
    }

    /// Returns all configured download roots, with the default root first.
    pub fn storage_roots(&self) -> Vec<StorageRoot> {
        self.storage.roots()
    }

    /// Adds a named download root after validating that it is writable.
    pub fn add_storage_root(&mut self, name: String, path: PathBuf, cx: &mut Context<Self>)
                            -> Result<u32, StorageError> {
        let id = self.storage.add_root(name, path)?;
        cx.emit(SettingsChanged);
        Ok(id)
    }

    /// Removes a non-default download root. The default root is never removed.
    pub fn remove_storage_root(&mut self, id: u32, cx: &mut Context<Self>) {
        self.storage.remove_root(id);
        cx.emit(SettingsChanged);
    }

    /// Removes a placement rule by its ordered index.
    pub fn remove_storage_rule(&mut self, index: usize, cx: &mut Context<Self>) {
        self.storage.remove_rule(index);
        cx.emit(SettingsChanged);
    }

    /// Moves a placement rule earlier or later in evaluation order.
    pub fn move_storage_rule(&mut self, index: usize, down: bool, cx: &mut Context<Self>) {
        if down {
            self.storage.move_rule_down(index);
        }
        else {
            self.storage.move_rule_up(index);
        }
        cx.emit(SettingsChanged);
    }

    /// Adds a publisher rule targeting the most recently configured root.
    pub fn add_publisher_rule(&mut self, publisher: String, cx: &mut Context<Self>) {
        let publisher = publisher.trim();
        if publisher.is_empty() {
            return;
        }
        self.storage
            .add_rule(PlacementCondition::Publisher(publisher.to_owned()),
                      self.rule_target_root_id);
        cx.emit(SettingsChanged);
    }

    /// Selects the root targeted by newly created placement rules.
    pub fn set_rule_target_root(&mut self, id: u32, cx: &mut Context<Self>) {
        if self.storage.root(id).is_some() {
            self.rule_target_root_id = id;
            cx.emit(SettingsChanged);
        }
    }

    /// Selects the placement condition type shown in the rule editor.
    pub fn set_rule_editor_type(&mut self, kind: RuleEditorType, cx: &mut Context<Self>) {
        self.rule_editor_type = kind;
        cx.emit(SettingsChanged);
    }

    /// Adds a collection-membership placement rule from the numeric ID input.
    pub fn add_collection_rule(&mut self, value: String, cx: &mut Context<Self>) {
        if let Ok(id) = value.trim().parse::<u64>() {
            self.storage
                .add_rule(PlacementCondition::Collection(id), self.rule_target_root_id);
            cx.emit(SettingsChanged);
        }
    }

    /// Adds a minimum-size placement rule from the megabytes input.
    pub fn add_size_rule(&mut self, value: String, cx: &mut Context<Self>) {
        if let Ok(size) = value.trim().parse::<f64>()
           && size >= 0.0
        {
            self.storage
                .add_rule(PlacementCondition::SizeAtLeastMb(size),
                          self.rule_target_root_id);
            cx.emit(SettingsChanged);
        }
    }

    /// Validates `path` for writability and saves it as the new storage root.
    ///
    /// Emits [`SettingsChanged`] on success. Returns the validation error on
    /// failure so the caller can surface it to the user.
    ///
    /// # Errors
    ///
    /// Returns a [`StorageError`] if `path` is missing, unwritable, or on an
    /// unavailable volume.
    pub fn apply_storage_path(&mut self, path: PathBuf, cx: &mut Context<Self>)
                              -> Result<(), StorageError> {
        validate_writable(&path)?;
        self.storage.set_root_path(path.clone());
        self.storage_unavailable = false;
        self.check_storage_path_exists(path, cx);
        cx.emit(SettingsChanged);
        Ok(())
    }

    /// Saves `n` as the new shared thumbnail/download concurrency limit.
    ///
    /// Emits [`SettingsChanged`] so the library controller picks up the new
    /// limit on its next drain.
    pub fn set_max_concurrent_downloads(&mut self, n: usize, cx: &mut Context<Self>) {
        self.storage.set_max_concurrent_downloads(n);
        cx.emit(SettingsChanged);
    }

    /// Saves `enabled` as the new "Create collections" storage setting.
    ///
    /// Emits [`SettingsChanged`] so the library controller picks up the new
    /// setting on its next download completion.
    pub fn set_create_collections(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.storage.set_create_collections(enabled);
        cx.emit(SettingsChanged);
    }

    /// Saves `days` (clamped to `[RECENTLY_UPDATED_WINDOW_MIN_DAYS,
    /// RECENTLY_UPDATED_WINDOW_MAX_DAYS]`) as the new "Recently Updated
    /// window", returning the clamped value actually stored.
    ///
    /// Emits [`SettingsChanged`] so the library controller picks up the new
    /// window immediately. Callers driving an editable number field (see
    /// `root_view.rs`) use the returned value to write the clamped result
    /// back into the field's own displayed text, since the field's typed
    /// text isn't otherwise corrected when a value outside the bounds is
    /// committed before the field loses focus.
    pub fn set_recently_updated_window_days(&mut self, days: u32, cx: &mut Context<Self>) -> u32 {
        self.storage.set_recently_updated_window_days(days);
        cx.emit(SettingsChanged);
        self.storage.recently_updated_window_days()
    }

    /// Spawns a background task to check whether `path` exists on disk.
    ///
    /// Writes the result back to `storage_path_exists` and notifies the UI.
    pub(super) fn check_storage_path_exists(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, async_cx| {
              let exists = async_cx.background_executor()
                                   .spawn(async move { path.exists() })
                                   .await;
              this.update(async_cx, |ctrl, cx| {
                      ctrl.storage_path_exists = exists;
                      cx.notify();
                  })
                  .ok();
          })
          .detach();
    }

    /// Opens the OS file manager at the configured storage root, creating the
    /// directory first if it does not exist.
    pub fn reveal_storage_location(&self) {
        let path = self.storage.root_path();
        if !path.exists() {
            let _ = std::fs::create_dir_all(&path);
        }
        if let Err(e) = crate::util::reveal::reveal_in_file_manager(&path) {
            tracing::warn!("reveal_in_file_manager failed: {e}");
        }
    }

    /// Deletes all regenerable app cache data (catalog/collections metadata
    /// cache and the cached avatar image) from disk, then emits
    /// [`CacheCleared`] so the library view drops its in-memory catalog and
    /// re-fetches live.
    ///
    /// Does not touch downloaded content, credentials, or preferences.
    pub fn clear_cache(&mut self, cx: &mut Context<Self>) {
        let dir = app_cache_dir();
        if let Err(e) = std::fs::remove_dir_all(&dir)
           && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("clear cache: failed to remove {}: {e}", dir.display());
        }
        self.refresh_cache_counts();
        cx.emit(CacheCleared);
    }

    /// Recomputes the per-type [`CacheCounts`] shown on the Advanced page and
    /// stores them for subsequent [`snapshot`](Self::snapshot) calls.
    ///
    /// The computation touches disk: a cache-metadata sidecar read (no full
    /// catalog parse), one `read_dir` of the covers directory, and one avatar
    /// `exists()` check. It runs on the panel-open, page-change, and
    /// cache-clear paths rather than once per render pass, so the render path
    /// itself performs no disk I/O.
    pub fn refresh_cache_counts(&mut self) {
        self.cache_counts = compute_cache_counts();
    }

    /// Opens the OS file manager at the app cache directory, creating it
    /// first if it does not exist (a fresh install may have no cache yet).
    pub fn open_cache_folder(&self) {
        let dir = app_cache_dir();
        if !dir.exists() {
            let _ = std::fs::create_dir_all(&dir);
        }
        if let Err(e) = crate::util::reveal::reveal_in_file_manager(&dir) {
            tracing::warn!("reveal_in_file_manager failed: {e}");
        }
    }
}

/// Reads the current per-type cache counts from disk.
///
/// Kept free of `SettingsController` so its disk-I/O cost is obvious at the
/// call site and it stays trivially unit-testable — see
/// [`SettingsController::refresh_cache_counts`], its only production caller.
fn compute_cache_counts() -> CacheCounts {
    let metadata = load_cache_metadata(&cache_dir());
    let metadata_items = metadata.as_ref().map_or(0, |m| m.item_count);
    let metadata_saved_at_secs =
        metadata.as_ref()
                .map(|m| i64::try_from(m.saved_at_secs).unwrap_or(i64::MAX));
    let last_item_check_batch_secs = metadata.as_ref()
                                             .and_then(|m| m.last_item_check_batch_secs)
                                             .map(|secs| i64::try_from(secs).unwrap_or(i64::MAX));
    let cover_thumbnails = std::fs::read_dir(covers_dir()).map(Iterator::count)
                                                          .unwrap_or(0);
    CacheCounts { metadata_items,
                  cover_thumbnails,
                  avatar_cached: avatar_cached(),
                  metadata_saved_at_secs,
                  last_item_check_batch_secs }
}
