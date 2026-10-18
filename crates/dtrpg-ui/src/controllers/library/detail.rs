//! Detail-panel state: ZIP previews, other-details disclosure, panel width.

use super::*;

impl LibraryController {
    /// Returns the active v2 inspector tab for `entry_id`.
    #[must_use]
    pub fn inspector_tab_for(&self, entry_id: &str) -> InspectorTab {
        self.inspector_tabs.get(entry_id).copied().unwrap_or_default()
    }

    /// Changes the active v2 inspector tab for `entry_id`.
    pub fn set_inspector_tab(&mut self, entry_id: Arc<str>, tab: InspectorTab,
                             cx: &mut Context<Self>) {
        if self.inspector_tab_for(&entry_id) == tab {
            return;
        }
        self.inspector_tabs.insert(entry_id, tab);
        cx.emit(LibraryChanged);
    }
    /// Returns the current archive contents state for `entry_id`.
    #[must_use]
    pub fn inspector_contents_for(&self, entry_id: &str) -> InspectorContentsState {
        self.inspector_contents
            .get(entry_id)
            .cloned()
            .unwrap_or(InspectorContentsState::Unavailable)
    }

    /// Returns whether an archive entry is selected for extraction.
    #[must_use]
    pub fn is_inspector_entry_selected(&self, entry_id: &str, name: &str) -> bool {
        self.inspector_selected_entries
            .get(entry_id)
            .is_some_and(|entries| entries.contains(name))
    }

    /// Returns the number of selected archive entries for extraction.
    #[must_use]
    pub fn selected_inspector_entry_count(&self, entry_id: &str) -> usize {
        self.inspector_selected_entries.get(entry_id).map_or(0, HashSet::len)
    }

    /// Returns the current preview state for an archive entry.
    #[must_use]
    pub fn inspector_preview_for(&self, entry_id: &str) -> InspectorPreviewState {
        self.inspector_preview.get(entry_id).cloned().unwrap_or(InspectorPreviewState::None)
    }

    /// Clears the current archive entry preview for `entry_id`.
    pub fn clear_inspector_preview(&mut self, entry_id: Arc<str>,
                                   cx: &mut Context<Self>) {
        if self.inspector_preview.remove(&entry_id).is_some() {
            cx.emit(LibraryChanged);
        }
    }

    /// Loads one archive entry for inline text preview off the render path.
    pub fn preview_inspector_entry(&mut self, entry_id: Arc<str>, name: String,
                                   cx: &mut Context<Self>) {
        let Some(item) = self.item_by_id(&entry_id).cloned() else { return; };
        let Some(file) = item.files.iter().find(|file| {
            matches!(file.format.to_ascii_uppercase().as_str(),
                     "ZIP" | "TAR" | "TAR.GZ" | "TGZ" | "TAR.BZ2" | "TBZ2" | "TAR.XZ" | "TXZ")
        }) else { return; };
        let path = resolved_item_dir(&StorageConfig::load(), &item).join(file.name.as_ref());
        self.inspector_preview.insert(Arc::clone(&entry_id), InspectorPreviewState::Loading(name.clone()));
        cx.emit(LibraryChanged);
        cx.spawn(async move |this, async_cx| {
              let read_name = name.clone();
              let result = async_cx.background_executor().spawn(async move {
                  let reader = dtrpg_core::util::container::open(&path)?;
                  reader.read_entry(&read_name)
              }).await;
              this.update(async_cx, |controller, cx| {
                     let state = match result {
                         Ok(bytes) => match String::from_utf8(bytes) {
                             Ok(contents) => InspectorPreviewState::Text { name, contents },
                             Err(_) => InspectorPreviewState::Unavailable(name),
                         },
                         Err(_) => InspectorPreviewState::Unavailable(name),
                     };
                     controller.inspector_preview.insert(entry_id, state);
                     cx.emit(LibraryChanged);
                 }).ok();
          }).detach();
    }

    /// Toggles one archive entry's extraction selection.
    pub fn toggle_inspector_entry(&mut self, entry_id: Arc<str>, name: String,
                                  cx: &mut Context<Self>) {
        let selected = self.inspector_selected_entries.entry(entry_id).or_default();
        if !selected.insert(name.clone()) {
            selected.remove(&name);
        }
        cx.emit(LibraryChanged);
    }

    /// Returns whether an extraction task is active for an inspector entry.
    #[must_use]
    pub fn is_inspector_extracting(&self, entry_id: &str) -> bool {
        self.inspector_extractions.get(entry_id).copied().unwrap_or(false)
    }

    /// Opens a destination picker and extracts the selected archive entries off the render path.
    pub fn extract_inspector_entries(&mut self, entry_id: Arc<str>, cx: &mut Context<Self>) {
        if self.is_inspector_extracting(&entry_id) {
            return;
        }
        let names: Vec<String> = self.inspector_selected_entries
            .get(&entry_id)
            .into_iter()
            .flat_map(|entries| entries.iter().cloned())
            .collect();
        if names.is_empty() {
            return;
        }
        let Some(item) = self.item_by_id(&entry_id).cloned() else { return; };
        let Some(file) = item.files.iter().find(|file| {
            matches!(file.format.to_ascii_uppercase().as_str(),
                     "ZIP" | "TAR" | "TAR.GZ" | "TGZ" | "TAR.BZ2" | "TBZ2" | "TAR.XZ" | "TXZ")
        }) else { return; };
        let archive_path = resolved_item_dir(&StorageConfig::load(), &item).join(file.name.as_ref());
        self.inspector_extractions.insert(Arc::clone(&entry_id), true);
        cx.emit(LibraryChanged);
        cx.spawn(async move |this, async_cx| {
              let destination = rfd::AsyncFileDialog::new().pick_folder().await;
              if let Some(destination) = destination {
                  let destination_path = destination.path().to_path_buf();
                  let result = async_cx.background_executor().spawn(async move {
                      let reader = dtrpg_core::util::container::open(&archive_path)?;
                      let names_ref: Vec<&str> = names.iter().map(String::as_str).collect();
                      reader.extract_entries(&names_ref, &destination_path)
                  }).await;
                  if let Err(error) = result {
                      tracing::warn!(%error, "inspector archive extraction failed");
                  }
              }
              this.update(async_cx, |controller, cx| {
                     controller.inspector_extractions.remove(&entry_id);
                     cx.emit(LibraryChanged);
                 }).ok();
          }).detach();
    }

    /// Loads the first container file for an entry off the render path.
    pub fn load_inspector_contents(&mut self, entry_id: Arc<str>, cx: &mut Context<Self>) {
        if matches!(self.inspector_contents.get(&entry_id), Some(InspectorContentsState::Loading)) {
            return;
        }
        let Some(item) = self.item_by_id(&entry_id).cloned()
        else {
            return;
        };
        let Some(file) = item.files.iter().find(|file| {
            matches!(file.format.to_ascii_uppercase().as_str(),
                     "ZIP" | "TAR" | "TAR.GZ" | "TGZ" | "TAR.BZ2" | "TBZ2" | "TAR.XZ" | "TXZ")
        })
        else {
            self.inspector_contents.insert(entry_id, InspectorContentsState::Unavailable);
            cx.emit(LibraryChanged);
            return;
        };
        let path = resolved_item_dir(&StorageConfig::load(), &item).join(file.name.as_ref());
        self.inspector_contents.insert(Arc::clone(&entry_id), InspectorContentsState::Loading);
        cx.emit(LibraryChanged);
        cx.spawn(async move |this, async_cx| {
              let result = async_cx.background_executor()
                                   .spawn(async move { dtrpg_core::util::container::list_entries(&path) })
                                   .await;
              this.update(async_cx, |controller, cx| {
                     let state = match result {
                         Ok(entries) => InspectorContentsState::Loaded(entries),
                         Err(_) => InspectorContentsState::Unavailable,
                     };
                     controller.inspector_contents.insert(entry_id, state);
                     cx.emit(LibraryChanged);
                 })
                 .ok();
          })
          .detach();
    }

    /// Returns `entry_id`'s file at `row_ix`'s Info column value (PDF page
    /// count, Zip file count, or image dimensions).
    ///
    /// Returns the cached value if present. Otherwise, if the file has been
    /// downloaded to `entry_dir`, extracts and caches its Info value via
    /// [`crate::util::file_info::compute_file_info`]; returns
    /// [`FileInfo::None`] immediately for undownloaded files or an unknown
    /// entry/row without touching the cache.
    pub fn file_info(&mut self, entry_id: &Arc<str>, row_ix: usize, entry_dir: &Path)
                     -> crate::util::file_info::FileInfo {
        let Some((downloaded, path, format)) =
            self.item_by_id(entry_id)
                .and_then(|item| item.files.get(row_ix))
                .map(|file| {
                    (file.downloaded, entry_dir.join(file.name.as_ref()), file.format.clone())
                })
        else {
            return crate::util::file_info::FileInfo::None;
        };
        cached_file_info(&mut self.file_info_cache,
                         entry_id,
                         row_ix,
                         downloaded,
                         &path,
                         &format)
    }

    // ── Other details disclosure ─────────────────────────────────────────────

    /// Returns whether `entry_id`'s "Other details" section is expanded.
    ///
    /// Defaults to `false` (collapsed) for any entry id not yet toggled, per
    /// `catalog-entry-detail-advanced-disclosure`.
    #[must_use]
    pub fn is_other_details_open(&self, entry_id: &str) -> bool {
        self.other_details_open
            .get(entry_id)
            .copied()
            .unwrap_or(false)
    }

    /// Flips `entry_id`'s "Other details" open/collapsed state.
    ///
    /// Ephemeral — never persisted, and independent per entry id.
    ///
    /// Emits [`LibraryChanged`].
    pub fn toggle_other_details(&mut self, entry_id: Arc<str>, cx: &mut Context<Self>) {
        toggle_bool_flag(&mut self.other_details_open, entry_id);
        cx.emit(LibraryChanged);
    }

    /// Returns whether a single file's "Other details" section is expanded
    /// within a multi-item entry's item tier.
    ///
    /// `key` is `"{entry_id}:{row_ix}"` (built by `file_other_details_key` in
    /// `ui::views::detail_panel_view`). Defaults to `false` (collapsed) for
    /// any key not yet toggled.
    #[must_use]
    pub fn is_file_other_details_open(&self, key: &str) -> bool {
        self.file_other_details_open
            .get(key)
            .copied()
            .unwrap_or(false)
    }

    /// Flips a single file's "Other details" open/collapsed state within a
    /// multi-item entry's item tier.
    ///
    /// Ephemeral — never persisted, and independent per `"{entry_id}:{row_ix}"`
    /// key.
    ///
    /// Emits [`LibraryChanged`].
    pub fn toggle_file_other_details(&mut self, key: Arc<str>, cx: &mut Context<Self>) {
        toggle_bool_flag(&mut self.file_other_details_open, key);
        cx.emit(LibraryChanged);
    }

    // ── Detail panel width
    // ────────────────────────────────────────────────────

    /// Returns the current detail panel width, in pixels.
    #[must_use]
    pub fn detail_panel_width(&self) -> f32 {
        self.detail_panel_width
    }

    /// Sets the detail panel width, clamped to
    /// `[DETAIL_PANEL_MIN_WIDTH, DETAIL_PANEL_MAX_WIDTH]`.
    ///
    /// Emits [`LibraryChanged`].
    pub fn set_detail_panel_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.detail_panel_width = width.clamp(dtrpg_core::data::constants::DETAIL_PANEL_MIN_WIDTH,
                                              dtrpg_core::data::constants::DETAIL_PANEL_MAX_WIDTH);
        cx.emit(LibraryChanged);
    }
}
