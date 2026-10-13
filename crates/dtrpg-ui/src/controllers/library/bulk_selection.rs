//! Bulk-selection mode and bulk actions over the current selection set (see
//! `catalog-selection` and `catalog-bulk-actions`).

use std::path::PathBuf;

use super::*;
use crate::data::enums::SelectionField;
use crate::util::item_opener::ItemOpener;
use crate::util::matching::collection_member_id;

impl LibraryController {
    // ── Selection mode ────────────────────────────────────────────────────

    /// Toggles bulk-selection mode. Turning it off clears the selection set.
    pub fn toggle_bulk_selection_mode(&mut self, cx: &mut Context<Self>) {
        self.bulk_selection_mode = !self.bulk_selection_mode;
        if !self.bulk_selection_mode {
            self.bulk_selected_ids.clear();
        }
        cx.emit(LibraryChanged);
    }

    /// Returns `true` if bulk-selection mode is active.
    #[must_use]
    pub fn bulk_selection_mode(&self) -> bool {
        self.bulk_selection_mode
    }

    /// Returns `true` if `id` is in the current bulk selection.
    #[must_use]
    pub fn is_bulk_selected(&self, id: &str) -> bool {
        self.bulk_selected_ids.contains(id)
    }

    /// Number of items currently selected for bulk action.
    #[must_use]
    pub fn bulk_selection_count(&self) -> usize {
        self.bulk_selected_ids.len()
    }

    // ── Selection set mutation ────────────────────────────────────────────

    /// Adds `id` to the bulk selection.
    pub fn bulk_select_item(&mut self, id: Arc<str>, cx: &mut Context<Self>) {
        self.bulk_selected_ids.insert(id);
        cx.emit(LibraryChanged);
    }

    /// Removes `id` from the bulk selection.
    pub fn bulk_deselect_item(&mut self, id: &str, cx: &mut Context<Self>) {
        self.bulk_selected_ids.remove(id);
        cx.emit(LibraryChanged);
    }

    /// Selects every item currently visible (matching the active filter and
    /// search query).
    pub fn bulk_select_all(&mut self, cx: &mut Context<Self>) {
        self.bulk_selected_ids
            .extend(self.visible_items().into_iter().map(|i| i.id));
        cx.emit(LibraryChanged);
    }

    /// Clears the bulk selection without leaving selection mode.
    pub fn bulk_deselect_all(&mut self, cx: &mut Context<Self>) {
        self.bulk_selected_ids.clear();
        cx.emit(LibraryChanged);
    }

    /// Adds every visible item whose `field` contains `pattern`
    /// (case-insensitive) to the bulk selection. No-op for an empty pattern.
    pub fn bulk_select_by_pattern(&mut self, pattern: &str, field: SelectionField,
                                  cx: &mut Context<Self>) {
        if pattern.is_empty() {
            return;
        }
        let needle = pattern.to_lowercase();
        let matches =
            self.visible_items().into_iter().filter_map(|item| {
                                                let haystack = match field {
                                                    SelectionField::Title => &item.title,
                                                    SelectionField::Publisher => &item.publisher,
                                                    SelectionField::System => &item.line,
                                                };
                                                haystack.to_lowercase()
                                                        .contains(&needle)
                                                        .then_some(item.id)
                                            });
        self.bulk_selected_ids.extend(matches);
        cx.emit(LibraryChanged);
    }

    /// Clears the bulk selection and turns off selection mode. Called by
    /// every bulk action on completion.
    pub(super) fn clear_bulk_selection_and_exit_mode(&mut self, cx: &mut Context<Self>) {
        self.bulk_selected_ids.clear();
        self.bulk_selection_mode = false;
        cx.emit(LibraryChanged);
    }

    // ── Bulk actions ──────────────────────────────────────────────────────

    /// Queues a download for every not-yet-downloaded file across the
    /// selection, then clears the selection and exits selection mode.
    pub fn bulk_download(&mut self, cx: &mut Context<Self>) {
        let targets: Vec<(Arc<str>, String)> =
            self.bulk_selected_ids
                .iter()
                .filter_map(|id| {
                    self.catalog
                        .iter()
                        .find(|i| &i.id == id && i.status != ItemStatus::Downloaded)
                        .map(|i| (Arc::clone(&i.id), i.title.to_string()))
                })
                .collect();
        for (id, title) in targets {
            self.enqueue_download(&id, title, cx);
        }
        self.clear_bulk_selection_and_exit_mode(cx);
    }

    /// Reverts every downloaded item in the selection to `Cloud` status,
    /// then clears the selection and exits selection mode.
    pub fn bulk_remove_download(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<Arc<str>> = self.bulk_selected_ids.iter().cloned().collect();
        for id in ids {
            self.remove_download(&id, cx);
        }
        self.clear_bulk_selection_and_exit_mode(cx);
    }

    /// Enqueues a thumbnail fetch for every selected item that has a cover
    /// URL, then clears the selection and exits selection mode.
    pub fn bulk_fetch_thumbnail(&mut self, cx: &mut Context<Self>) {
        let cover_urls: Vec<Arc<str>> = self.bulk_selected_ids
                                            .iter()
                                            .filter_map(|id| {
                                                self.catalog
                                                    .iter()
                                                    .find(|i| &i.id == id)
                                                    .and_then(|i| i.cover_url.clone())
                                            })
                                            .collect();
        for url in cover_urls {
            self.load_thumbnail(url, cx);
        }
        self.clear_bulk_selection_and_exit_mode(cx);
    }

    /// Adds every selected item to `collection_id`, then clears the
    /// selection and exits selection mode.
    pub fn bulk_add_to_collection(&mut self, collection_id: u64, cx: &mut Context<Self>) {
        let targets: Vec<(u64, u64)> = self.bulk_selected_ids
                                           .iter()
                                           .filter_map(|id| {
                                               self.catalog
                                                   .iter()
                                                   .find(|i| &i.id == id)
                                                   .map(|i| (collection_member_id(i), i.product_id))
                                           })
                                           .collect();
        for (member_id, product_id) in targets {
            self.add_item_to_collection(collection_id, member_id, product_id, cx);
        }
        self.clear_bulk_selection_and_exit_mode(cx);
    }

    /// Removes every selected item from `collection_id` that belongs to it,
    /// then clears the selection and exits selection mode.
    pub fn bulk_remove_from_collection(&mut self, collection_id: u64, cx: &mut Context<Self>) {
        let targets: Vec<(u64, u64)> = self.bulk_selected_ids
                                           .iter()
                                           .filter_map(|id| {
                                               self.catalog
                                                   .iter()
                                                   .find(|i| &i.id == id)
                                                   .map(|i| (collection_member_id(i), i.product_id))
                                           })
                                           .collect();
        for (member_id, product_id) in targets {
            self.remove_item_from_collection(collection_id, member_id, product_id, cx);
        }
        self.clear_bulk_selection_and_exit_mode(cx);
    }

    /// Opens every selected item that has a local download with its default
    /// application; cloud-only items and items requiring file-selection
    /// (ambiguous multi-item entries) are skipped. Clears the selection and
    /// exits selection mode.
    pub fn bulk_open(&mut self, cx: &mut Context<Self>) {
        let storage = StorageConfig::load();
        let targets: Vec<(PathBuf, Vec<LibraryItemFile>)> = self.bulk_selected_ids
                                                                .iter()
                                                                .filter_map(|id| {
                                                                    self.catalog
                    .iter()
                    .find(|i| &i.id == id && i.status == ItemStatus::Downloaded)
                    .map(|i| (storage.path_for_publisher(&i.publisher), i.files.clone()))
                                                                })
                                                                .collect();
        for (entry_dir, files) in targets {
            if let Err(e) = ItemOpener::open_item(&entry_dir, &files) {
                tracing::warn!("bulk_open: skipping item: {e:?}");
            }
        }
        self.clear_bulk_selection_and_exit_mode(cx);
    }
}
