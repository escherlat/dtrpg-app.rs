//! `Library::invalidate_cache` and the render-path accessors that read what
//! it computed.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::{Arc, PoisonError};

use super::ViewState;
use crate::data::enums::GroupBy;
use crate::data::index::ItemFilter;
use crate::data::library::{LibraryItem, SectionCounts};
use crate::library::Library;
use crate::util::publisher::{ItemGroup, PublisherEntry, split_by_publisher, split_by_tag};
use crate::util::search::SearchQuery;

impl Library {
    /// Builds an `ItemFilter` representing `view`'s sidebar filter, before
    /// the search query is applied.
    fn item_filter<'a>(view: &'a ViewState, now_secs: i64) -> ItemFilter<'a> {
        use crate::util::filter::SidebarFilter;
        match &view.filter {
            SidebarFilter::AllTitles => ItemFilter::default(),
            SidebarFilter::Tag { name, .. } => ItemFilter { tag_name: Some(name),
                                                            ..Default::default() },
            SidebarFilter::OnDevice => {
                ItemFilter { status: Some(crate::data::enums::ItemStatus::Downloaded),
                             ..Default::default() }
            }
            SidebarFilter::InCloud => ItemFilter { status:
                                                       Some(crate::data::enums::ItemStatus::Cloud),
                                                   ..Default::default() },
            SidebarFilter::RecentlyUpdated => {
                ItemFilter { recently_updated: Some((now_secs,
                                                     view.recently_updated_window_days)),
                             ..Default::default() }
            }
            SidebarFilter::Publisher(name) => ItemFilter { publisher: Some(name),
                                                           ..Default::default() },
            SidebarFilter::Collection(..) => ItemFilter { collection_member_ids:
                                                              Some(&view.collection_members),
                                                          ..Default::default() },
        }
    }

    /// Recomputes the filtered, sorted item cache (and its grouped view, the
    /// sidebar facet counts, and the catalog's product-id set) from `view`.
    ///
    /// Called by the controller at every mutation site that changes the
    /// filter, search query, sort, or group-by setting, so render-path
    /// accessors below never re-scan the catalog. `untagged_label` is the
    /// already-localized "Untagged" string for `GroupBy::Tag` — `Library`
    /// has no `gpui`/i18n dependency, so the caller resolves it.
    pub fn invalidate_cache(&self, view: &ViewState, now_secs: i64, untagged_label: &str) {
        self.refresh_sidebar_facets(now_secs, view.recently_updated_window_days);

        // The toolbar's `filter_count` means "section size with search
        // ignored," so the count runs before `search` joins the filter. A
        // failed count keeps the previous number — the same
        // keep-stale-on-failure policy as the facet refresh.
        let base_filter = Self::item_filter(view, now_secs);
        let count = self.index
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .count_items_filtered(&base_filter);

        let query = SearchQuery::parse(&view.search_query);
        let mut search_filter = Self::item_filter(view, now_secs);
        search_filter.search = Some(&query);
        let matched = self.index
                          .lock()
                          .unwrap_or_else(PoisonError::into_inner)
                          .list_items_filtered(&search_filter, view.sort, view.sort_direction);

        let mut cache = self.query_cache
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner);
        match count {
            Ok(count) => cache.filter_count = count,
            Err(error) => tracing::warn!(%error, "could not count the filtered catalog"),
        }
        let matched = match matched {
            Ok(items) => items,
            Err(error) => {
                // Both toolbar numbers describe the (empty) table that
                // follows; a stale count over an empty result would
                // contradict it.
                tracing::warn!(%error, "could not query the filtered catalog");
                cache.filter_count = 0;
                Vec::new()
            }
        };
        drop(cache);

        let items = self.overlaid_items(matched);
        let grouped = self.build_grouped_cache(view, now_secs, untagged_label);
        let catalog_ids = self.compute_catalog_ids();

        let mut cache = self.query_cache
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner);
        cache.items = Some(Arc::new(items));
        cache.grouped = grouped;
        cache.catalog_ids = Arc::new(catalog_ids);
    }

    /// Builds the grouped-items cache for `view.group_by`, or `None` when
    /// grouping is disabled. Queries the index so grouping shares the same
    /// source-of-truth as filtering and search.
    fn build_grouped_cache(&self, view: &ViewState, now_secs: i64, untagged_label: &str)
                           -> Option<Arc<Vec<ItemGroup>>> {
        let mut filter = Self::item_filter(view, now_secs);
        let query = SearchQuery::parse(&view.search_query);
        filter.search = Some(&query);
        let index = self.index.lock().unwrap_or_else(PoisonError::into_inner);

        match view.group_by {
            GroupBy::None => None,
            GroupBy::Publisher => {
                let items = index.list_items_for_grouping_by_publisher(&filter,
                                                                        view.sort,
                                                                        view.sort_direction)
                                 .inspect_err(|error| {
                                     tracing::warn!(%error, "could not group by publisher")
                                 })
                                 .ok()?;
                Some(Arc::new(split_by_publisher(items)))
            }
            GroupBy::Tag => {
                let rows =
                    index.list_items_for_grouping_by_tag(&filter, view.sort, view.sort_direction)
                         .inspect_err(|error| tracing::warn!(%error, "could not group by tag"))
                         .ok()?;
                Some(Arc::new(split_by_tag(rows, untagged_label)))
            }
        }
    }

    /// Recomputes the sidebar's section counts from the index facet query,
    /// replacing a whole-catalog scan.
    ///
    /// On query failure the previous counts are kept: a failed count leaves
    /// stale-but-plausible numbers in the sidebar rather than zeroes in the
    /// middle of a load that otherwise succeeds.
    pub fn refresh_section_counts(&self, now_secs: i64, window_days: u32) {
        let result = self.index
                         .lock()
                         .unwrap_or_else(PoisonError::into_inner)
                         .section_counts(now_secs, window_days);
        match result {
            Ok(counts) => {
                self.query_cache
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .section_counts = counts;
            }
            Err(error) => tracing::warn!(%error, "could not count the catalog sections"),
        }
    }

    /// Refreshes both sidebar facet lists (section counts and publishers),
    /// and returns whether the publisher list was refreshed successfully.
    /// Callers that revalidate a `Publisher` filter against the new list
    /// should skip it when this returns `false`: revalidating stale numbers
    /// could drop a filter the catalog still satisfies.
    pub fn refresh_sidebar_facets(&self, now_secs: i64, window_days: u32) -> bool {
        self.refresh_section_counts(now_secs, window_days);
        let result = self.index
                         .lock()
                         .unwrap_or_else(PoisonError::into_inner)
                         .publisher_counts();
        match result {
            Ok(counts) => {
                let publishers = counts.into_iter()
                                       .map(|(name, count)| PublisherEntry { name, count })
                                       .collect();
                self.query_cache
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .publishers = publishers;
                true
            }
            Err(error) => {
                tracing::warn!(%error, "could not count the catalog publishers");
                false
            }
        }
    }

    /// Builds the set of all product IDs that can match a collection's
    /// `member_ids`, by a full catalog scan.
    ///
    /// Includes both `order_product_id` and `product_id` since the product
    /// list items API returns `productId` (not `orderProductId`) in its
    /// response. Prefer [`Self::cached_catalog_ids`] on a render path — this
    /// recomputes from a fresh `all_items()` scan every call.
    #[must_use]
    pub fn catalog_ids(&self) -> HashSet<u64> {
        self.compute_catalog_ids()
    }

    fn compute_catalog_ids(&self) -> HashSet<u64> {
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

    /// The product-id set computed by the most recent `invalidate_cache`
    /// call. Cheap: a render path should use this, not [`Self::catalog_ids`].
    #[must_use]
    pub fn cached_catalog_ids(&self) -> Arc<HashSet<u64>> {
        Arc::clone(&self.query_cache
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .catalog_ids)
    }

    /// The sidebar's section counts, from the most recent `invalidate_cache`
    /// call.
    #[must_use]
    pub fn section_counts(&self) -> SectionCounts {
        self.query_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .section_counts
    }

    /// The publisher list, from the most recent `invalidate_cache` call.
    #[must_use]
    pub fn publishers(&self) -> Vec<PublisherEntry> {
        self.query_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .publishers
            .clone()
    }

    /// Returns the filtered, sorted result set for the most recent
    /// `invalidate_cache` call, without copying it.
    #[must_use]
    pub fn visible_items(&self) -> Arc<Vec<LibraryItem>> {
        self.query_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .items
            .clone()
            .unwrap_or_default()
    }

    /// Returns the number of items in the filtered, sorted result set.
    ///
    /// Cheap: it does not copy the result set.
    #[must_use]
    pub fn visible_items_count(&self) -> usize {
        self.query_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .items
            .as_deref()
            .map_or(0, Vec::len)
    }

    /// Returns a slice of the filtered, sorted result set.
    ///
    /// `range` is 0-based relative to the start of the full result set.
    #[must_use]
    pub fn visible_items_slice(&self, range: Range<usize>) -> Vec<LibraryItem> {
        self.query_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .items
            .as_deref()
            .and_then(|items| items.get(range))
            .map(<[LibraryItem]>::to_vec)
            .unwrap_or_default()
    }

    /// Returns the grouped result set for the most recent `invalidate_cache`
    /// call.
    #[must_use]
    pub fn visible_items_grouped(&self) -> Arc<Vec<ItemGroup>> {
        self.query_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .grouped
            .clone()
            .unwrap_or_default()
    }

    /// Returns how many catalog items the sidebar filter alone matches
    /// (search ignored), from the most recent `invalidate_cache` call.
    #[must_use]
    pub fn filter_count(&self) -> usize {
        self.query_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .filter_count
    }
}
