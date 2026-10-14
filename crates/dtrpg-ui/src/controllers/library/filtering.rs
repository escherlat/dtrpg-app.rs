//! Sidebar filtering, search, sorting, grouping, and view-preference
//! persistence.

use super::*;

impl LibraryController {
    /// Returns the filtered, sorted result set for the current state.
    #[must_use]
    pub fn visible_items(&self) -> Vec<LibraryItem> {
        self.items_cache.clone().unwrap_or_default()
    }

    /// Returns the number of items in the filtered, sorted result set.
    ///
    /// Cheaper than cloning the full `Vec` when only the count is needed.
    #[must_use]
    pub fn visible_items_count(&self) -> usize {
        self.visible_items().len()
    }

    /// Returns a slice of the filtered, sorted result set.
    ///
    /// `range` is 0-based relative to the start of the full result set. Used
    /// by `uniform_list` render closures.
    #[must_use]
    pub fn visible_items_slice(&self, range: std::ops::Range<usize>) -> Vec<LibraryItem> {
        let items = self.visible_items();
        items
            .get(range)
            .map(<[LibraryItem]>::to_vec)
            .unwrap_or_default()
    }

    // ── Sidebar filter mutations
    // ──────────────────────────────────────────────

    /// Sets the active sidebar filter.
    ///
    /// When the new filter is `Collection(id, _)`, `collection_members` is
    /// populated from the matching entry's `member_ids`. For all other
    /// filters, `collection_members` is cleared.
    pub fn set_filter(&mut self, filter: SidebarFilter, cx: &mut Context<Self>) {
        if let SidebarFilter::Collection(id, _) = &filter {
            self.collection_members = self
                .collections
                .iter()
                .find(|c| c.id == *id)
                .map(|c| c.member_ids.iter().copied().collect())
                .unwrap_or_default();
        } else {
            self.collection_members.clear();
        }
        self.filter = filter;
        self.selection = Selection::None;
        self.invalidate_cache();
        self.save_view_prefs();
        cx.emit(LibraryChanged);
    }

    // ── Search mutations
    // ──────────────────────────────────────────────────────

    /// Updates the text search query.
    pub fn set_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.search_query = query;
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Clears the text search query.
    pub fn clear_search_query(&mut self, cx: &mut Context<Self>) {
        self.search_query.clear();
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    // ── Sidebar section search mutations
    // ──────────────────────────────────────
    //
    // These filter the publishers/collections sidebar lists only. They never
    // touch the catalog cache and are intentionally session-only: closing a
    // section's search bar clears its query so reopening it always starts
    // fresh.

    /// Toggles the publishers section's inline search bar, clearing its query
    /// on close.
    pub fn toggle_publisher_search(&mut self, cx: &mut Context<Self>) {
        self.publisher_search_open = !self.publisher_search_open;
        if !self.publisher_search_open {
            self.publisher_search_query.clear();
        }
        cx.emit(LibraryChanged);
    }

    /// Updates the publishers section search filter text.
    pub fn set_publisher_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.publisher_search_query = query;
        cx.emit(LibraryChanged);
    }

    /// Toggles the collections section's inline search bar, clearing its query
    /// on close.
    pub fn toggle_collection_search(&mut self, cx: &mut Context<Self>) {
        self.collection_search_open = !self.collection_search_open;
        if !self.collection_search_open {
            self.collection_search_query.clear();
        }
        cx.emit(LibraryChanged);
    }

    /// Updates the collections section search filter text.
    pub fn set_collection_search_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.collection_search_query = query;
        cx.emit(LibraryChanged);
    }

    // ── Sort mutations
    // ────────────────────────────────────────────────────────

    /// Sets the sort method.
    pub fn set_sort(&mut self, sort: SortMethod, cx: &mut Context<Self>) {
        self.sort = sort;
        self.invalidate_cache();
        self.save_view_prefs();
        cx.emit(LibraryChanged);
    }

    /// Sets the sort direction.
    pub fn set_sort_direction(&mut self, direction: SortDirection, cx: &mut Context<Self>) {
        self.sort_direction = direction;
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Sets the sidebar Collections sort method, persists it, and re-sorts
    /// `self.collections` in place.
    pub fn set_collection_sort(&mut self, method: CollectionSortMethod, cx: &mut Context<Self>) {
        self.collection_sort = method;
        let catalog_ids = self.catalog_ids();
        sort_collections(
            &mut self.collections,
            method,
            self.collection_sort_direction,
            &catalog_ids,
        );
        crate::data::ui_preferences::UiPreferences::load()
            .save_collection_sort(method, self.collection_sort_direction);
        cx.emit(LibraryChanged);
    }

    /// Sets the sidebar Collections sort direction, persists it, and
    /// re-sorts `self.collections` in place.
    pub fn set_collection_sort_direction(
        &mut self, direction: SortDirection, cx: &mut Context<Self>,
    ) {
        self.collection_sort_direction = direction;
        let catalog_ids = self.catalog_ids();
        sort_collections(
            &mut self.collections,
            self.collection_sort,
            direction,
            &catalog_ids,
        );
        crate::data::ui_preferences::UiPreferences::load()
            .save_collection_sort(self.collection_sort, direction);
        cx.emit(LibraryChanged);
    }

    // ── Grouping / presentation mutations ────────────────────────────────────

    /// Toggles publisher grouping on or off.
    pub fn set_grouped(&mut self, grouped: bool, cx: &mut Context<Self>) {
        self.grouped = grouped;
        self.save_view_prefs();
        cx.emit(LibraryChanged);
    }

    /// Switches the catalog presentation mode.
    pub fn set_presentation(&mut self, mode: CatalogPresentation, cx: &mut Context<Self>) {
        self.presentation = mode;
        self.save_view_prefs();
        cx.emit(LibraryChanged);
    }

    /// Persists the current sidebar filter, sort, grouped, and presentation
    /// state to disk. Best-effort — I/O failures are logged and ignored (see
    /// [`crate::data::file_openers::CatalogViewPrefs::save`]).
    pub(super) fn save_view_prefs(&self) {
        crate::data::file_openers::CatalogViewPrefs::from_state(
            &self.filter,
            self.sort,
            self.grouped,
            self.presentation,
        )
        .save();
    }
}
