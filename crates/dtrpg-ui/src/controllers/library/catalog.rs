//! Catalog apply/reload logic, publisher-filter validation, and the render
//! snapshot.
//!
//! The library index is the catalog's only storage, and
//! `dtrpg_core::library::Library` is where the catalog-sync state machine
//! (Slice 2) and the filtered/grouped query cache (Slice 3) both live. This
//! controller reads through `self.library`'s accessor methods and owns only
//! view state (the active filter/search/sort/group settings themselves,
//! not their computed results).

use super::*;

impl LibraryController {
    /// Looks up a catalog item by id, independent of the current popover
    /// selection, with ephemeral state overlaid.
    ///
    /// Used to resolve the full item for an open expanded detail tab, which
    /// tracks only the item id, not a full `LibraryItem` snapshot.
    #[must_use]
    pub fn item_by_id(&self, id: &str) -> Option<LibraryItem> {
        self.library.item_by_id(id)
    }

    /// Returns every item in the catalog, with ephemeral state overlaid.
    pub(super) fn all_items(&self) -> Vec<LibraryItem> {
        self.library.all_items()
    }

    /// Reacts to new items a catalog-sync run (`dtrpg_core::library::Library`)
    /// just wrote to the index as a *load-completing* step: enqueues their
    /// thumbnails, refreshes the sidebar facets (revalidating a restored
    /// `Publisher` filter against the refreshed publisher list), invalidates
    /// the filtered-result cache, and emits [`LibraryChanged`].
    ///
    /// `Library` already wrote `items` to the index; this only updates this
    /// controller's own view state in reaction (see ADR 0002 Slice 2). Used
    /// by every `LibraryEvent` handler except the incremental
    /// pre-population ones (see [`Self::react_to_prepopulated_items`]),
    /// which must not revalidate the filter against a still-partial
    /// publisher list.
    pub(super) fn react_to_new_catalog_items(&mut self, items: &[LibraryItem],
                                             cx: &mut Context<Self>) {
        self.enqueue_thumbnails(items, cx);
        if self.refresh_sidebar_facets() {
            self.revalidate_publisher_filter();
        }
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Reacts to items appended incrementally during cache pre-population or
    /// the live fetch's catalog-was-empty fast path: enqueues their
    /// thumbnails, refreshes the sidebar facets *without* revalidating the
    /// `Publisher` filter (the publisher list computed from a still-partial
    /// catalog isn't authoritative enough to decide a filter is now
    /// invalid), invalidates the filtered-result cache, and emits
    /// [`LibraryChanged`].
    pub(super) fn react_to_prepopulated_items(&mut self, items: &[LibraryItem],
                                              cx: &mut Context<Self>) {
        self.enqueue_thumbnails(items, cx);
        let _ = self.refresh_sidebar_facets();
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Refreshes both sidebar facet lists (section counts and publishers)
    /// in `Library`, and returns whether the publisher list was refreshed
    /// successfully. Callers that revalidate a `Publisher` filter against
    /// the new list should skip it when this returns `false`: revalidating
    /// stale numbers could drop a filter the catalog still satisfies.
    pub(super) fn refresh_sidebar_facets(&mut self) -> bool {
        self.library
            .refresh_sidebar_facets(now_secs(), self.recently_updated_window_days)
    }

    /// Refreshes only the sidebar's section counts in `Library`, cheaper
    /// than [`Self::refresh_sidebar_facets`] for callers reacting to one
    /// item's status change — status changes never move an item between
    /// publishers, so there's no need to also refresh the publisher list.
    pub(super) fn refresh_section_counts(&mut self) {
        self.library
            .refresh_section_counts(now_secs(), self.recently_updated_window_days);
    }

    /// Resets `self.filter` to [`SidebarFilter::AllTitles`] if it is a
    /// `Publisher` filter whose publisher is not present in
    /// `known_publishers`.
    ///
    /// Called once the catalog-load's item list is available, since a
    /// `Publisher` filter restored from disk at [`Self::new`] (before any
    /// items have loaded) can't be validated synchronously. Does not save
    /// the reset — the persisted preference is left untouched, so a launch
    /// where the publisher reappears restores it.
    pub fn validate_publisher_filter(&mut self, known_publishers: &[Arc<str>]) {
        if let SidebarFilter::Publisher(name) = &self.filter
           && !known_publishers.contains(name)
        {
            self.filter = SidebarFilter::AllTitles;
        }
    }

    /// Convenience wrapper around [`Self::validate_publisher_filter`] using
    /// `Library`'s just-recomputed publisher list as the known set.
    ///
    /// Every catalog-load completion path that refreshes the publisher list
    /// must call this — see [`Self::react_to_new_catalog_items`], which every
    /// load-completing `LibraryEvent` handler goes through.
    pub(super) fn revalidate_publisher_filter(&mut self) {
        let known_publishers: Vec<Arc<str>> = self.library
                                                  .publishers()
                                                  .iter()
                                                  .map(|p| Arc::clone(&p.name))
                                                  .collect();
        self.validate_publisher_filter(&known_publishers);
    }

    /// Invokes the catalog auto-load freshness policy, running a full live
    /// fetch only when the policy determines one is needed.
    ///
    /// Used by the "Catalog > Reload" menu action. Gated by
    /// [`reload_cooldown_active`]: if the on-disk cache was written more
    /// recently than `FORCE_RELOAD_COOLDOWN_SECS`, this is a silent no-op —
    /// no network fetch, no change to `catalog_loading` or the catalog.
    pub fn reload_catalog(&mut self, cx: &mut Context<Self>) {
        let meta = load_cache_metadata(&cache_dir());
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                                              .map(|d| d.as_secs())
                                              .unwrap_or_default();
        if reload_cooldown_active(meta.as_ref(), now) {
            return;
        }
        self.catalog_loading = true;
        cx.emit(LibraryChanged);
        self.start_load_inner(cx, false);
    }

    /// Drops the catalog and collections, then forces a full live fetch.
    ///
    /// Used after the on-disk app cache has been cleared, so stale content
    /// disappears from the UI immediately instead of lingering until an
    /// unrelated reload repopulates it from what is now a missing cache file.
    ///
    /// Any catalog load already in flight is superseded: `Library::start_load`
    /// bumps its own generation (or queues this as a follow-up if one is
    /// already running — see ADR 0002 Slice 2), so an older run's completion
    /// is discarded rather than clobbering the fresh reload triggered here.
    /// Queued-but-not-yet-started thumbnail fetches are dropped too; a
    /// fetch already in flight still completes and populates `CoverCache`,
    /// which is harmless since it only caches an image for a URL that may
    /// no longer be visible.
    pub fn clear_and_reload(&mut self, cx: &mut Context<Self>) {
        self.library.clear_catalog();
        self.collections.clear();
        self.collection_members.clear();
        self.invalidate_cache();
        // Queued availability checks reference items that no longer exist in
        // the now-cleared catalog; an in-flight check (if any) still
        // completes normally and harmlessly finds nothing to update.
        self.cancel_pending_checks();
        self.cancel_pending_thumbnails(cx);
        // If nothing is currently in flight, no fetch completion will ever
        // complete the aggregated activity item — without this, it would be
        // left showing "in progress" indefinitely.
        if self.library.thumbnail_jobs_is_empty()
           && let Some(id) = self.thumbnail_activity_id.take()
        {
            self.thumbnail_processed = 0;
            self.activity.update(cx, |a, cx| a.complete(id, cx));
        }
        self.reload_catalog(cx);
    }

    /// Emits `LibraryChanged` to trigger a UI re-render without modifying
    /// state.
    ///
    /// Used by sidebar section headers to force a re-render after persisting UI
    /// prefs.
    pub fn notify_ui_change(&mut self, cx: &mut Context<Self>) {
        cx.emit(LibraryChanged);
    }

    /// Expands `section` of the sidebar if it is collapsed, and collapses it if
    /// it is expanded, and remembers which for the next launch.
    pub fn toggle_sidebar_section(&mut self, section: SidebarSection, cx: &mut Context<Self>) {
        let open = &mut self.sidebar_open;
        let mut saved = dtrpg_core::data::ui_state::UiState::load();
        match section {
            SidebarSection::Publishers => {
                open.publishers = !open.publishers;
                saved.save_publishers_open(open.publishers);
            }
            SidebarSection::Collections => {
                open.collections = !open.collections;
                saved.save_collections_open(open.collections);
            }
            SidebarSection::Tags => {
                open.tags = !open.tags;
                saved.save_tags_open(open.tags);
            }
        }
        cx.emit(LibraryChanged);
    }

    // ── Snapshot
    // ──────────────────────────────────────────────────────────────

    /// Returns all data needed by the root view for one render pass.
    pub fn snapshot(&self) -> LibrarySnapshot {
        let items = self.visible_items_shared();
        let matched_count = items.len();
        let selected_item = self.selected_item();
        let counts = self.library.section_counts();
        LibrarySnapshot { filter: self.filter.clone(),
                          counts,
                          publishers: self.library.publishers(),
                          collections: self.collections.clone(),
                          tags: self.tag_entries(),
                          collections_loaded: self.collections_loaded,
                          catalog_ids: self.library.cached_catalog_ids(),
                          total_count: counts.all,
                          total_mb: self.total_size_mb(),
                          filter_count: self.library.filter_count(),
                          matched_count,
                          search_query: self.search_query.clone(),
                          sort: self.sort,
                          sort_direction: self.sort_direction,
                          collection_sort: self.collection_sort,
                          collection_sort_direction: self.collection_sort_direction,
                          group_by: self.group_by,
                          sidebar_open: self.sidebar_open,
                          list_columns: self.list_columns.clone(),
                          presentation: self.presentation,
                          selected_item,
                          items,
                          catalog_loading: self.catalog_loading,
                          publisher_search_open: self.publisher_search_open,
                          publisher_search_query: self.publisher_search_query.clone(),
                          collection_search_open: self.collection_search_open,
                          collection_search_query: self.collection_search_query.clone(),
                          detail_panel_width: self.detail_panel_width,
                          bulk_selection_mode: self.bulk_selection_mode,
                          bulk_selected_ids: self.bulk_selected_ids.clone(),
                          bulk_selection_count: self.bulk_selected_ids.len() }
    }

    /// Returns true while the initial catalog fetch is still in flight and no
    /// items have arrived.
    pub fn is_loading(&self) -> bool {
        self.catalog_loading && self.index().count_all().unwrap_or(0) == 0
    }

    /// Builds the set of all product IDs that can match a collection's
    /// `member_ids`, by a full catalog scan.
    pub(super) fn catalog_ids(&self) -> HashSet<u64> {
        self.library.catalog_ids()
    }

    /// Records that a thumbnail fetch for item `id` was attempted at `at`.
    pub(super) fn note_thumbnail_attempt(&mut self, id: &Arc<str>, at: std::time::SystemTime) {
        self.library.note_thumbnail_attempt(id, at);
    }

    // ── Filtered result set
    // ───────────────────────────────────────────────────

    /// Recomputes `Library`'s filtered, sorted item cache from the
    /// controller's current filter, search query, sort, and group-by
    /// settings.
    ///
    /// Called eagerly at every mutation site that changes any of those
    /// inputs so render-path accessors never re-scan the catalog.
    pub(super) fn invalidate_cache(&mut self) {
        let view = ViewState { filter:                       self.filter.clone(),
                               collection_members:           self.collection_members.clone(),
                               search_query:                 self.search_query.clone(),
                               sort:                         self.sort,
                               sort_direction:               self.sort_direction,
                               group_by:                     self.group_by,
                               recently_updated_window_days: self.recently_updated_window_days, };
        self.library
            .invalidate_cache(&view, now_secs(), &t!("catalog.untagged"));
    }
}
