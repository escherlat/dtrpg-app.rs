## Why

The sidebar's "Recently Added" filter (`SidebarFilter::RecentlyAdded`) doesn't
filter by date at all: `item_matches_filter` and `section_counts` both test
`item.added_order <= RECENTLY_ADDED_THRESHOLD` (`RECENTLY_ADDED_THRESHOLD =
90`), where `added_order` is a stable rank position, not a timestamp. The
section always shows the 90 most-recently-added items regardless of how long
ago they were added — a user with a small library sees their entire catalog
in this filter forever, and a user who adds nothing for months still sees the
same 90 items indefinitely. It also only reflects an item's *addition* to the
library, never a later update (e.g. a re-download or metadata refresh).

## What Changes

- `SidebarFilter::RecentlyAdded` is renamed to `SidebarFilter::RecentlyUpdated`
  (and `SectionCounts::recently_added` to `SectionCounts::recently_updated`).
- The filter predicate changes from an `added_order` rank cutoff to a real
  30-day date window: an item matches if `max(date_added, date_updated)` is
  within the last 30 days of the current time.
- The sidebar and toolbar label changes from "Recently Added" to "Recently
  Updated" (`i18n` key `sidebar.recently_added` renamed to
  `sidebar.recently_updated`, value updated in all three locales).
- `RECENTLY_ADDED_THRESHOLD` (rank-based) is removed and replaced with a
  duration constant for the 30-day window.

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `libri-sidebar`: The "Recently Added" smart section is renamed "Recently
  Updated" and its requirement changes from an implicit rank-based cutoff to
  an explicit 30-day recency window keyed off `date_added`/`date_updated`.
- `string-catalog`: The sidebar navigation label scenario's literal label set
  changes from "Recently Added" to "Recently Updated".

## Impact

- `crates/dtrpg-ui/src/util/filter.rs`: `SidebarFilter::RecentlyAdded` →
  `RecentlyUpdated`.
- `crates/dtrpg-ui/src/util/matching.rs`: `item_matches_filter`'s predicate for
  this variant switches from `added_order` to a date-window check.
- `crates/dtrpg-ui/src/data/library.rs`: `SectionCounts::recently_added` →
  `recently_updated`; `section_counts`'s corresponding filter updated to match.
- `crates/dtrpg-ui/src/data/constants.rs`: `RECENTLY_ADDED_THRESHOLD` removed,
  replaced with a 30-day duration constant.
- `crates/dtrpg-ui/src/ui/views/toolbar_view.rs`,
  `crates/dtrpg-ui/src/ui/views/sidebar_view.rs`: references to
  `SidebarFilter::RecentlyAdded` and the `sidebar.recently_added` i18n key
  updated to the renamed variant/key.
- `crates/dtrpg-ui/i18n/en.yaml`, `de.yaml`, `fr.yaml`: `recently_added` key
  renamed to `recently_updated`, values updated to "Recently Updated" /
  translated equivalents.
- Other capabilities that merely list "Recently Added" as one of several
  filter names in passing (`sidebar-nav`, `publisher-filter-title`) aren't
  listed as modified since their requirements don't depend on the specific
  label text or cutoff semantics, but their prose will read stale until a
  later cosmetic pass updates the label mentioned there too.
