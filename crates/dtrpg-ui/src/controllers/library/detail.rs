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

    // ── Zip content preview (item tier) ──────────────────────────────────────

    /// Returns the active Zip preview's file row and anchor position for
    /// `entry_id`, if one is open (hovered or pinned).
    #[must_use]
    pub fn zip_preview_for(&self, entry_id: &str) -> Option<(usize, Point<Pixels>, bool)> {
        self.zip_preview
            .as_ref()
            .filter(|p| p.entry_id.as_ref() == entry_id)
            .map(|p| (p.row_ix, p.anchor_pos, p.pinned))
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

    /// Opens (or moves) the hover preview to `entry_id`'s file row `row_ix`,
    /// anchored at `anchor_pos`. No-op while a *different* row is pinned —
    /// pinning takes priority over hover until explicitly dismissed. Also a
    /// no-op if this row is already the active unpinned hover target, so
    /// continuous `on_mouse_move` ticks over the same row don't repeatedly
    /// re-read the archive or re-notify every frame.
    ///
    /// Emits [`LibraryChanged`].
    pub fn hover_zip_preview(&mut self, entry_id: Arc<str>, row_ix: usize,
                             anchor_pos: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(existing) = &self.zip_preview {
            if existing.pinned && (existing.entry_id != entry_id || existing.row_ix != row_ix) {
                return;
            }
            if !existing.pinned && existing.entry_id == entry_id && existing.row_ix == row_ix {
                return;
            }
        }
        self.zip_preview = Some(ZipPreviewState { entry_id,
                                                  row_ix,
                                                  anchor_pos,
                                                  pinned: false });
        cx.emit(LibraryChanged);
    }

    /// Ends the hover preview for `entry_id`'s file row `row_ix` — a no-op if
    /// that row is pinned (pinning survives mouse-out) or if a different row
    /// is the active preview.
    ///
    /// Emits [`LibraryChanged`].
    pub fn clear_hover_zip_preview(&mut self, entry_id: &str, row_ix: usize,
                                   cx: &mut Context<Self>) {
        let should_clear = self.zip_preview.as_ref().is_some_and(|p| {
                                                        !p.pinned
                                                        && p.entry_id.as_ref() == entry_id
                                                        && p.row_ix == row_ix
                                                    });
        if should_clear {
            self.zip_preview = None;
            cx.emit(LibraryChanged);
        }
    }

    /// Toggles the pinned state of `entry_id`'s file row `row_ix`: pins it
    /// open (opening the preview if it wasn't already active) on the first
    /// click, or clears it entirely on a second click of the same row.
    ///
    /// Emits [`LibraryChanged`].
    pub fn toggle_zip_preview_pin(&mut self, entry_id: Arc<str>, row_ix: usize,
                                  anchor_pos: Point<Pixels>, cx: &mut Context<Self>) {
        let already_pinned =
            self.zip_preview
                .as_ref()
                .is_some_and(|p| p.pinned && p.entry_id == entry_id && p.row_ix == row_ix);
        self.zip_preview = if already_pinned {
            None
        }
        else {
            Some(ZipPreviewState { entry_id,
                                   row_ix,
                                   anchor_pos,
                                   pinned: true })
        };
        cx.emit(LibraryChanged);
    }

    /// Closes the Zip preview popover for `entry_id`, whether hovered or
    /// pinned — used by the popover's own close control.
    ///
    /// Emits [`LibraryChanged`].
    pub fn close_zip_preview(&mut self, entry_id: &str, cx: &mut Context<Self>) {
        if self.zip_preview
               .as_ref()
               .is_some_and(|p| p.entry_id.as_ref() == entry_id)
        {
            self.zip_preview = None;
            cx.emit(LibraryChanged);
        }
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
