//! Catalog apply/reload logic, publisher-filter validation, and the render
//! snapshot.

use super::*;

impl LibraryController {
    /// Atomically replaces the catalog with a complete dataset and recomputes
    /// derived state.
    ///
    /// Used after all live SDK pages have been collected so the UI transitions
    /// from cached data to the full live set in one render pass.
    /// `reconcile` selects between two behaviors, per `catalog-live-data-swap`:
    /// `false` replaces `self.catalog` outright with `items` (used when there
    /// was no local baseline to reconcile against — first launch or after
    /// `clear_and_reload`); `true` merges `items` into the existing catalog
    /// by id via `reconcile_catalog` (items missing from `items` are kept,
    /// flagged `is_available = false`, rather than dropped).
    pub(super) fn set_catalog(&mut self, items: Vec<LibraryItem>, reconcile: bool,
                              cx: &mut Context<Self>) {
        self.enqueue_thumbnails(&items, cx);
        self.catalog = if reconcile {
            reconcile_catalog(std::mem::take(&mut self.catalog), items)
        }
        else {
            items
        };
        self.catalog_loading = false;
        self.section_counts =
            section_counts(&self.catalog, now_secs(), self.recently_updated_window_days);
        self.publishers = publisher_entries(&self.catalog);
        self.revalidate_publisher_filter();
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
        self.catalog = merge_partial_fetch(std::mem::take(&mut self.catalog), partial);
        self.catalog_loading = false;
        self.section_counts =
            section_counts(&self.catalog, now_secs(), self.recently_updated_window_days);
        self.publishers = publisher_entries(&self.catalog);
        self.revalidate_publisher_filter();
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
        self.catalog.extend(items);
        self.section_counts =
            section_counts(&self.catalog, now_secs(), self.recently_updated_window_days);
        self.publishers = publisher_entries(&self.catalog);
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

    /// Drops the in-memory catalog and collections, then forces a full live
    /// fetch.
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
        self.catalog.clear();
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

    // ── Snapshot
    // ──────────────────────────────────────────────────────────────

    /// Returns all data needed by the root view for one render pass.
    pub fn snapshot(&self) -> LibrarySnapshot {
        let now_secs = now_secs();
        let window_days = self.recently_updated_window_days;
        let filter_count = self.catalog
                               .iter()
                               .filter(|i| {
                                   item_matches_filter(i,
                                                       &self.filter,
                                                       &self.collection_members,
                                                       now_secs,
                                                       window_days)
                               })
                               .count();
        let items = self.visible_items();
        let matched_count = items.len();
        let selected_item = self.selected_item().cloned();
        let catalog_ids = self.catalog_ids();
        LibrarySnapshot { filter: self.filter.clone(),
                          counts: self.section_counts,
                          publishers: self.publishers.clone(),
                          collections: self.collections.clone(),
                          tags: self.tag_entries(),
                          collections_loaded: self.collections_loaded,
                          catalog_ids,
                          total_count: self.section_counts.all,
                          total_mb: self.total_size_mb(),
                          filter_count,
                          matched_count,
                          search_query: self.search_query.clone(),
                          sort: self.sort,
                          sort_direction: self.sort_direction,
                          collection_sort: self.collection_sort,
                          collection_sort_direction: self.collection_sort_direction,
                          group_by: self.group_by,
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
        self.catalog_loading && self.catalog.is_empty()
    }

    /// Builds the set of all product IDs that can match a collection's
    /// `member_ids`.
    ///
    /// Includes both `order_product_id` and `product_id` since the product
    /// list items API returns `productId` (not `orderProductId`) in its
    /// response.
    pub(super) fn catalog_ids(&self) -> HashSet<u64> {
        self.catalog
            .iter()
            .flat_map(|i| {
                let mut ids = Vec::with_capacity(2);
                if i.order_product_id > 0 {
                    ids.push(i.order_product_id);
                }
                if i.product_id > 0 {
                    ids.push(i.product_id);
                }
                ids
            })
            .collect()
    }

    // ── Filtered result set
    // ───────────────────────────────────────────────────

    /// Recomputes the filtered, sorted item cache from the current catalog,
    /// filter, search query, and sort settings.
    ///
    /// Called eagerly at every mutation site that changes any of those inputs
    /// so render-path accessors never re-scan the catalog.
    pub(super) fn invalidate_cache(&mut self) {
        let now_secs = now_secs();
        let window_days = self.recently_updated_window_days;
        let query = SearchQuery::parse(&self.search_query);
        let mut items: Vec<LibraryItem> = self.catalog
                                              .iter()
                                              .filter(|i| {
                                                  item_matches_filter(i,
                                                                      &self.filter,
                                                                      &self.collection_members,
                                                                      now_secs,
                                                                      window_days)
                                                  && query.matches(i, self.tags.tag_names_of(&i.id))
                                              })
                                              .cloned()
                                              .collect();
        sort_items(&mut items, self.sort, self.sort_direction);
        self.items_cache = Some(items);
    }
}
