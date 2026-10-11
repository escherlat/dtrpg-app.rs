## 1. Constants and Filter Enum

- [ ] 1.1 In `crates/dtrpg-ui/src/data/constants.rs`, remove
      `RECENTLY_ADDED_THRESHOLD` and add
      `RECENTLY_UPDATED_WINDOW_SECS: i64 = 30 * 24 * 60 * 60`
- [ ] 1.2 In `crates/dtrpg-ui/src/util/filter.rs`, rename
      `SidebarFilter::RecentlyAdded` to `RecentlyUpdated` (including its
      `PartialEq` arm)

## 2. Filter Predicate

- [ ] 2.1 In `crates/dtrpg-ui/src/util/matching.rs`, add
      `item_recently_updated(item: &LibraryItem, now_secs: i64) -> bool`
      that compares `item.date_updated.or(item.date_added)` (falling back to
      `date_added` when there's no update timestamp) against
      `now_secs - RECENTLY_UPDATED_WINDOW_SECS`; returns `false` when both
      are `None`
- [ ] 2.2 Update `item_matches_filter`'s `SidebarFilter::RecentlyUpdated` arm
      to call `item_recently_updated(item, now_secs)`, threading a `now_secs:
      i64` parameter through `item_matches_filter`'s signature and its call
      sites

## 3. Section Counts

- [ ] 3.1 In `crates/dtrpg-ui/src/data/library.rs`, rename
      `SectionCounts::recently_added` to `recently_updated`
- [ ] 3.2 Update `section_counts` to take a `now_secs: i64` parameter and use
      `item_recently_updated` (from `util::matching`) for the
      `recently_updated` count instead of the `added_order` check; update
      `section_counts`'s call sites to pass the current time

## 4. UI Views

- [ ] 4.1 In `crates/dtrpg-ui/src/ui/views/toolbar_view.rs`, update the
      `SidebarFilter::RecentlyAdded` match arm (title label lookup) and the
      filter-construction call site to use `RecentlyUpdated`
- [ ] 4.2 In `crates/dtrpg-ui/src/ui/views/sidebar_view.rs`, update the nav
      item's filter reference, `counts.recently_added` field access, and
      active-state comparison to use `RecentlyUpdated` / `recently_updated`

## 5. Localization

- [ ] 5.1 Rename the `sidebar.recently_added` key to `sidebar.recently_updated`
      in `crates/dtrpg-ui/i18n/en.yaml`, updating its value to "Recently
      Updated"
- [ ] 5.2 Apply the same key rename in `de.yaml` and `fr.yaml`, updating
      values to their translated equivalents ("Kürzlich aktualisiert" /
      "Mises à jour récentes")

## 6. Tests

- [ ] 6.1 Unit test: an item whose `date_updated` is within the last 30 days
      matches `item_recently_updated`
- [ ] 6.2 Unit test: an item whose `date_updated` is `None` but whose
      `date_added` is within the last 30 days matches
      `item_recently_updated`
- [ ] 6.3 Unit test: an item whose most recent timestamp is exactly 30 days
      plus one second ago does not match `item_recently_updated`
- [ ] 6.4 Unit test: an item with both `date_added` and `date_updated` as
      `None` does not match `item_recently_updated`
- [ ] 6.5 Unit test: `item_matches_filter` with `SidebarFilter::RecentlyUpdated`
      delegates correctly for a matching and a non-matching item
- [ ] 6.6 Unit test: `section_counts`'s `recently_updated` count reflects only
      items within the 30-day window

## 7. Build and Quality

- [ ] 7.1 `cargo check --workspace`
- [ ] 7.2 `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] 7.3 `cargo test --workspace`
- [ ] 7.4 `cargo +nightly fmt --all -- --check`

## 8. Manual Verification

- [ ] 8.1 Confirm the sidebar shows "Recently Updated" (not "Recently Added")
      and its badge count matches the number of items added or updated in
      the last 30 days
- [ ] 8.2 Confirm the toolbar title shows "Recently Updated" when that
      filter is active
