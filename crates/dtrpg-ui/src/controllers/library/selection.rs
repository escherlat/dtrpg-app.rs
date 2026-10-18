//! Item and file selection state.

use super::*;

impl LibraryController {
    // ── Selection mutations
    // ───────────────────────────────────────────────────

    /// Opens the single-click item popover for `id` (see `main-window-tabs`).
    ///
    /// Distinct from opening an expanded detail tab, which is a separate,
    /// double-click action handled by `TabsController::open_detail_tab`.
    pub fn select_item(&mut self, id: Arc<str>, cx: &mut Context<Self>) {
        self.maybe_check_item(Arc::clone(&id), cx);
        self.verify_selected_item_download(Arc::clone(&id), cx);
        self.selection = Selection::Item(id);
        cx.emit(LibraryChanged);
    }

    /// Closes the item popover, if one is open.
    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selection = Selection::None;
        cx.emit(LibraryChanged);
    }

    /// Precise on-screen bounds of catalog entry `id`, if it has painted at
    /// least once. Used to anchor the single-click item popover beside the
    /// entry that opened it — see [`Self::set_entry_bounds`].
    ///
    /// Every visible Grid card and Thumbs row reports its bounds continuously
    /// (not just the selected one), so by the time an entry can be clicked it
    /// has already painted and its bounds are available immediately — no
    /// one-frame flash at a fallback position before the popover settles
    /// beside the entry.
    pub fn entry_bounds(&self, id: &str) -> Option<Bounds<Pixels>> {
        self.entry_bounds.get(id).copied()
    }

    /// Records the on-screen bounds of catalog entry `id`, called from that
    /// entry's own render pass once painted (see
    /// `catalog_view::render_grid_card` / `render_thumb_row`).
    ///
    /// Only re-emits [`LibraryChanged`] when the bounds actually changed, so
    /// repeated paints of an entry that hasn't moved don't trigger a
    /// re-render feedback loop.
    pub fn set_entry_bounds(&mut self, id: Arc<str>, bounds: Bounds<Pixels>,
                            cx: &mut Context<Self>) {
        if self.entry_bounds.get(&id) != Some(&bounds) {
            self.entry_bounds.insert(id, bounds);
            cx.emit(LibraryChanged);
        }
    }

    /// Looks up a catalog item by id, independent of the current popover
    /// selection.
    ///
    /// Used to resolve the full item for an open expanded detail tab, which
    /// tracks only the item id, not a full `LibraryItem` snapshot.
    #[must_use]
    pub fn item_by_id(&self, id: &str) -> Option<&LibraryItem> {
        self.catalog.iter().find(|i| i.id.as_ref() == id)
    }

    // ── Item selection (multi-item detail tab) ───────────────────────────────

    /// Returns the row index of the currently selected item file for a
    /// multi-item entry's detail tab, if any item has been selected.
    #[must_use]
    pub fn selected_item_file(&self, entry_id: &str) -> Option<usize> {
        self.selected_item_file.get(entry_id).copied()
    }

    /// Selects the file at `row_ix` as the active item within `entry_id`'s
    /// expanded detail tab, updating the item metadata area in place.
    ///
    /// Clicking the already-selected row deselects it (toggle), returning
    /// the item list to its unselected prompt state.
    ///
    /// Emits [`LibraryChanged`].
    pub fn select_item_file(&mut self, entry_id: Arc<str>, row_ix: usize, cx: &mut Context<Self>) {
        toggle_row_selection(&mut self.selected_item_file, entry_id, row_ix);
        cx.emit(LibraryChanged);
    }

    /// Clears any selected item for `entry_id`'s detail tab, so reopening it
    /// shows no pre-selected item (selection is ephemeral, see
    /// `catalog-entry-detail-view`).
    ///
    /// Emits [`LibraryChanged`].
    pub fn clear_item_selection(&mut self, entry_id: &str, cx: &mut Context<Self>) {
        let had_selection = self.selected_item_file.remove(entry_id).is_some();
        let had_inspector_tab = self.inspector_tabs.remove(entry_id).is_some();
        let had_inspector_contents = self.inspector_contents.remove(entry_id).is_some();
        let had_inspector_selection = self.inspector_selected_entries.remove(entry_id).is_some();
        let had_inspector_extraction = self.inspector_extractions.remove(entry_id).is_some();
        let had_inspector_preview = self.inspector_preview.remove(entry_id).is_some();
        let _had_inspector_archive = self.inspector_selected_archive.remove(entry_id).is_some();
        let had_inspector_dirs = self.inspector_collapsed_dirs.remove(entry_id).is_some();
        let had_inspector_filter = self.inspector_filter_inputs.remove(entry_id).is_some();
        if had_selection
           || had_inspector_tab
           || had_inspector_contents
           || had_inspector_selection
           || had_inspector_extraction
           || had_inspector_preview
           || had_inspector_dirs
           || had_inspector_filter
        {
            cx.emit(LibraryChanged);
        }
    }
}
