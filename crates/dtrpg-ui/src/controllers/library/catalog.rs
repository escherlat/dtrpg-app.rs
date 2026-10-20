//! Catalog apply/reload logic, publisher-filter validation, and the render
//! snapshot.
//!
//! The library index (`self.index`) is the catalog's only storage — there is
//! no in-memory `Vec<LibraryItem>` mirror of it. Every method here that used
//! to scan or mutate one now reads from or writes to `self.index` directly.
//! `thumbnail_last_attempted`/`availability_last_checked` are the two
//! exceptions: they're session-only state the index never persists, kept in
//! `self.thumbnail_attempts`/`self.availability_checks` and overlaid onto an
//! item right after it's fetched from the index (see
//! [`Self::overlay_ephemeral_state`]), so the UI-facing `LibraryItem` API is
//! unchanged.

use super::*;

impl LibraryController {
    /// Sets `item.thumbnail_last_attempted`/`availability_last_checked` from
    /// the controller's session-only tracking maps, since the index never
    /// persists either (see this module's doc comment).
    pub(super) fn overlay_ephemeral_state(&self, item: &mut LibraryItem) {
        item.thumbnail_last_attempted = self.thumbnail_attempts.get(&item.id).copied();
        item.availability_last_checked = self.availability_checks.get(&item.id).copied();
    }

    fn overlaid_items(&self, mut items: Vec<LibraryItem>) -> Vec<LibraryItem> {
        for item in &mut items {
            self.overlay_ephemeral_state(item);
        }
        items
    }

    /// Returns every item in the catalog, with ephemeral state overlaid.
    ///
    /// The one place every other catalog-reading method in this controller
    /// should go through, so none of them forget the overlay.
    pub(super) fn all_items(&self) -> Vec<LibraryItem> {
        self.overlaid_items(self.index.list_items().unwrap_or_default())
    }

    /// Looks up a catalog item by id, independent of the current popover
    /// selection, with ephemeral state overlaid.
    ///
    /// Used to resolve the full item for an open expanded detail tab, which
    /// tracks only the item id, not a full `LibraryItem` snapshot.
    #[must_use]
    pub fn item_by_id(&self, id: &str) -> Option<LibraryItem> {
        let mut item = self.index
                           .get_item(id)
                           .map_err(|e| tracing::warn!(error = %e, "library index query failed"))
                           .ok()
                           .flatten()?;
        self.overlay_ephemeral_state(&mut item);
        Some(item)
    }

    /// Recomputes the sidebar's section counts from the index facet query,
    /// replacing the whole-catalog `all_items()` scan (issue #355).
    ///
    /// On query failure the previous counts are kept: a failed count leaves
    /// stale-but-plausible numbers in the sidebar rather than zeroes in the
    /// middle of a load that otherwise succeeds. Status changes never move
    /// an item between publishers, so callers reacting to one item's status
    /// only need this — see [`Self::refresh_sidebar_facets`] for the full
    /// load-completion refresh.
    pub(super) fn refresh_section_counts(&mut self) {
        let now = now_secs();
        match self.index
                  .section_counts(now, self.recently_updated_window_days)
        {
            Ok(counts) => self.section_counts = counts,
            Err(error) => tracing::warn!(%error, "could not count the catalog sections"),
        }
    }

    /// Refreshes both sidebar facet lists (section counts and publishers)
    /// after a whole-catalog load, and returns whether the publisher list
    /// was refreshed successfully. Callers that revalidate a `Publisher`
    /// filter against the new list should skip it when this returns `false`:
    /// revalidating stale numbers could drop a filter the catalog still
    /// satisfies.
    pub(super) fn refresh_sidebar_facets(&mut self) -> bool {
        self.refresh_section_counts();
        match self.index.publisher_counts() {
            Ok(counts) => {
                self.publishers = counts.into_iter()
                                        .map(|(name, count)| PublisherEntry { name, count })
                                        .collect();
                true
            }
            Err(error) => {
                tracing::warn!(%error, "could not count the catalog publishers");
                false
            }
        }
    }

    /// Atomically replaces the catalog with a complete dataset and recomputes
    /// derived state.
    ///
    /// Used after all live SDK pages have been collected so the UI transitions
    /// from cached data to the full live set in one render pass.
    /// `reconcile` selects between two behaviors, per `catalog-live-data-swap`:
    /// `false` replaces the catalog outright with `items` (used when there
    /// was no local baseline to reconcile against — first launch or after
    /// `clear_and_reload`); `true` merges `items` into the existing catalog
    /// by id via `reconcile_catalog` (items missing from `items` are kept,
    /// flagged `is_available = false`, rather than dropped).
    pub(super) fn set_catalog(&mut self, items: Vec<LibraryItem>, reconcile: bool,
                              cx: &mut Context<Self>) {
        self.enqueue_thumbnails(&items, cx);
        let merged = if reconcile {
            reconcile_catalog(self.index.list_items().unwrap_or_default(), items)
        }
        else {
            items
        };
        if let Err(e) = self.index.replace_all_items(&merged) {
            tracing::error!(error = %e, "failed to write the catalog to the library index");
        }
        self.catalog_loading = false;
        if self.refresh_sidebar_facets() {
            self.revalidate_publisher_filter();
        }
        self.invalidate_cache();
        cx.emit(LibraryChanged);
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
    /// the just-recomputed `self.publishers` as the known set.
    ///
    /// Every catalog-load completion path that (re)computes `self.publishers`
    /// must call this — not just the full live-fetch path
    /// ([`Self::set_catalog`]) — since the cache-fresh skip-fetch branch in
    /// [`Self::start_load_inner`] and [`Self::apply_partial_fetch`] both also
    /// complete a load without ever calling `set_catalog`. Missing one of
    /// these left a restored `Publisher` filter that no longer matched
    /// permanently un-validated on an ordinary cache-hit cold start.
    pub(super) fn revalidate_publisher_filter(&mut self) {
        let known_publishers: Vec<Arc<str>> = self.publishers
                                                  .iter()
                                                  .map(|p| Arc::clone(&p.name))
                                                  .collect();
        self.validate_publisher_filter(&known_publishers);
    }

    /// Merges a partial date-filtered fetch's `partial` results into the
    /// catalog additively (see [`merge_partial_fetch`]) and performs the
    /// same finalization as [`Self::set_catalog`] (thumbnail enqueue,
    /// derived state recompute, cache invalidation, `LibraryChanged`).
    ///
    /// Unlike a full reconcile, this never flags any existing item
    /// unavailable — a partial response is never a complete listing.
    pub(super) fn apply_partial_fetch(&mut self, partial: Vec<LibraryItem>,
                                      cx: &mut Context<Self>) {
        self.enqueue_thumbnails(&partial, cx);
        let merged = merge_partial_fetch(self.index.list_items().unwrap_or_default(), partial);
        if let Err(e) = self.index.replace_all_items(&merged) {
            tracing::error!(error = %e, "failed to write the catalog to the library index");
        }
        self.catalog_loading = false;
        if self.refresh_sidebar_facets() {
            self.revalidate_publisher_filter();
        }
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Appends a page of items received incrementally from the background load
    /// task.
    ///
    /// Used only for the initial disk-cache pre-population, not for live SDK
    /// pages.
    pub(super) fn append_catalog_page(&mut self, items: Vec<LibraryItem>, cx: &mut Context<Self>) {
        self.enqueue_thumbnails(&items, cx);
        let mut merged = self.index.list_items().unwrap_or_default();
        merged.extend(items);
        if let Err(e) = self.index.replace_all_items(&merged) {
            tracing::error!(error = %e, "failed to write the catalog to the library index");
        }
        self.refresh_sidebar_facets();
        self.invalidate_cache();
        cx.emit(LibraryChanged);
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
    /// Any catalog load already in flight is superseded: `start_load_inner`
    /// bumps `load_generation`, so the older task's completion is discarded
    /// rather than clobbering the fresh reload triggered here.
    /// Queued-but-not-yet-started thumbnail fetches are dropped too; a
    /// fetch already in flight still completes and populates `CoverCache`,
    /// which is harmless since it only caches an image for a URL that may
    /// no longer be visible.
    pub fn clear_and_reload(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = self.index.replace_all_items(&[]) {
            tracing::error!(error = %e, "failed to clear the library index");
        }
        self.collections.clear();
        self.collection_members.clear();
        self.section_counts = SectionCounts::default();
        self.publishers.clear();
        self.invalidate_cache();
        // Queued availability checks reference items that no longer exist in
        // the now-cleared catalog; an in-flight check (if any) still
        // completes normally and harmlessly finds nothing to update.
        self.cancel_pending_checks();
        self.cancel_pending_thumbnails(cx);
        // If nothing is currently in flight, no fetch completion will ever
        // complete the aggregated activity item — without this, it would be
        // left showing "in progress" indefinitely.
        if self.thumbnail_jobs.is_empty()
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
        LibrarySnapshot { filter: self.filter.clone(),
                          counts: self.section_counts,
                          publishers: self.publishers.clone(),
                          collections: self.collections.clone(),
                          tags: self.tag_entries(),
                          collections_loaded: self.collections_loaded,
                          catalog_ids: Arc::clone(&self.catalog_ids_cache),
                          total_count: self.section_counts.all,
                          total_mb: self.total_size_mb(),
                          filter_count: self.filter_count_cache,
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
        self.catalog_loading && self.index.count_all().unwrap_or(0) == 0
    }

    /// Builds the set of all product IDs that can match a collection's
    /// `member_ids`.
    ///
    /// Includes both `order_product_id` and `product_id` since the product
    /// list items API returns `productId` (not `orderProductId`) in its
    /// response.
    pub(super) fn catalog_ids(&self) -> HashSet<u64> {
        let all = self.all_items();
        let mut ids = HashSet::with_capacity(all.len() * 2);
        for item in &all {
            if item.order_product_id > 0 {
                ids.insert(item.order_product_id);
            }
            if item.product_id > 0 {
                ids.insert(item.product_id);
            }
        }
        ids
    }

    /// Records that a thumbnail fetch for item `id` was attempted at `at`, in
    /// the session-only tracking map and in the cached result set.
    ///
    /// Does not recompute the result set, as [`invalidate_cache`] would: the
    /// time is in neither the filter nor the sort, and a cover arrives for
    /// every item scrolled into view.
    ///
    /// [`invalidate_cache`]: Self::invalidate_cache
    pub(super) fn note_thumbnail_attempt(&mut self, id: &Arc<str>, at: std::time::SystemTime) {
        self.thumbnail_attempts.insert(Arc::clone(id), at);
        let Some(cached) = self.items_cache.as_mut()
        else {
            return;
        };
        if let Some(item) = Arc::make_mut(cached).iter_mut().find(|item| item.id == *id) {
            item.thumbnail_last_attempted = Some(at);
        }
    }

    // ── Filtered result set
    // ───────────────────────────────────────────────────

    /// Recomputes the filtered, sorted item cache from the catalog's current
    /// filter, search query, and sort settings.
    ///
    /// Called eagerly at every mutation site that changes any of those inputs
    /// so render-path accessors never re-scan the catalog.
    pub(super) fn invalidate_cache(&mut self) {
        // The toolbar's `filter_count` means "section size with search
        // ignored" (`count_label_for`), so the count runs before `search`
        // joins the WHERE. A failed count keeps the previous number, the same
        // keep-stale-on-failure policy as the facet refreshes (issue #355).
        let count = {
            let item_filter = self.current_item_filter();
            match self.index.count_items_filtered(&item_filter) {
                Ok(count) => Some(count),
                Err(error) => {
                    tracing::warn!(%error, "could not count the filtered catalog");
                    None
                }
            }
        };
        if let Some(count) = count {
            self.filter_count_cache = count;
        }

        let matched = {
            let query = SearchQuery::parse(&self.search_query);
            let mut item_filter = self.current_item_filter();
            item_filter.search = Some(&query);
            match self.index
                      .list_items_filtered(&item_filter, self.sort, self.sort_direction)
            {
                Ok(items) => Some(items),
                Err(error) => {
                    // Both toolbar numbers describe the (empty) table that
                    // follows; a stale count over an empty
                    // result would contradict it.
                    tracing::warn!(%error, "could not query the filtered catalog");
                    None
                }
            }
        };
        let matched = match matched {
            Some(items) => items,
            None => {
                self.filter_count_cache = 0;
                Vec::new()
            }
        };
        let items = self.overlaid_items(matched);

        self.items_cache = Some(Arc::new(items));
        self.grouped_items_cache = self.build_grouped_cache();
        self.catalog_ids_cache = Arc::new(self.catalog_ids());
    }
}
