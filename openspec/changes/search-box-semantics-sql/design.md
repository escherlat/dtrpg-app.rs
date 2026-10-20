# Design

## Why `instr(ulower(col), ?)`, not the alternatives

- **FTS5 `MATCH`** (the existing `LibraryIndex::search`): word-token
  matching with prefix semantics — `moria` would not match `Dungeon of the
  Moria'im`-style substrings the way `contains` does, and it tokenizes, so
  `tag:`-free substring searches like `ia o` behave nothing like the box.
  Rejected: wrong semantics, not a detail.
- **FTS5 trigram tokenizer**: substring-capable, but 1-2 character queries
  raise or fall back to LIKE anyway, the fold is not Rust's
  `to_lowercase()`, and it needs a new virtual table plus write-through
  sync for a win that only matters at catalog sizes where the plain scan is
  already the old behavior. Rejected under YAGNI; revisit if invalidate
  measurements show the scan dominating.
- **`LIKE '%text%' ESCAPE`**: SQLite's `LIKE` case folding is ASCII-only, so
  `Éowyn` typed as `éowyn` misses — exactly the Unicode bug the issue's
  verification bar forbids. Also needs escaping of `%`/`_` in query text.
- **`instr(ulower(col), ?)`**: one bound parameter per predicate, no
  escaping surface (no wildcards), and — because `ulower` is literally
  `str::to_lowercase()` — equality with the Rust matcher holds by
  construction on both sides of the comparison. Chosen.

Cost shape: `instr` over `items` is a full-table scan, but the query already
was a full scan before this change (`list_items_filtered` has no index that
sidebar filters use for text); what gets *smaller* is the Rust side — no
second pass, and only survivors get `row_to_item` + per-item file loads.

## Custom function plumbing

`rusqlite::Connection::create_scalar_function("ulower", 1, /* deterministic */
true, …)` registered in a new `data/index/functions.rs`, called from both
open paths (`open_and_migrate`, test-only `open_in_memory`) immediately
after `ensure_schema`. NULL inputs: every column the predicate touches is
`NOT NULL` in the schema, so the function simply requires a text argument;
an error surfaces as `IndexError::Sqlite`, never a silent miss.

## `Tag` filter as SQL: EXISTS by name, not an id-set `IN`

`SidebarFilter::Tag { name, items }` carries a pre-computed `HashSet` of
member ids. Two ways to express it in the query:

- `id IN (…)`: variable count grows with members; at SQLITE default limits
  (~32k parameters) a mega-tag would fail the query — and
  `invalidate_cache`'s existing `unwrap_or_default` error policy would
  silently show an empty catalog. Rejected.
- `EXISTS (SELECT 1 FROM item_tags it JOIN tags tg ON tg.id = it.tag_id
  WHERE it.item_id = items.id AND ulower(tg.name) = ?)`, binding
  `name.to_lowercase()` (computed Rust-side, same fold): bounded params,
  and it intersects with `items` rows the way the old Rust pass's
  `contains(&item.id)` did — ids that carry a tag but no longer exist in
  the catalog are excluded by both paths. Chosen.

Known divergence, accepted and flagged: the old passes matched against the
controller's `TagLibrary` snapshot; SQL reads the `tags`/`item_tags` tables
directly. During the async window between an index tag edit and the
snapshot re-read (`load_tags` → `show_tags` → `invalidate_cache`), a search
typed in that window sees the fresher index state instead of the stale
snapshot. The index has been the catalog's single source of truth since
#354; search following it is the consistent direction, and both paths
converge as soon as the snapshot reloads (which tag edits already trigger).
The same window can make "download all in this tag section"
(`filter_download_targets` → `item_matches_filter`, unchanged by this
change) target the snapshot set while the visible list shows the index set.
The rule is general: any remaining `TagLibrary` snapshot reader —
"download all in section" (`item_matches_filter`, the only reader of
`SidebarFilter::Tag.items`), the sidebar's per-tag counts — can show the
stale snapshot beside an index-driven list until `load_tags` completes.
Those paths also still ignore the search box, exactly as before; all are
out of scope here.

## Order stability

`list_items_filtered` sorts ascending in SQL and `reverse()`s for descending
(existing behavior, mirroring `sort_items`). With search now inside the
WHERE, items that tie on the sort key can come out in a different relative
order than the old post-hoc filter produced — ties were never ordered
differently *by design* (both orders came from the same comparator), and the
parity tests use fixtures with unique sort keys per query so the assertion
is exact. Documented rather than engineered.

## `count_items_filtered`

Same `build_where` as the list query, minus ORDER BY, `SELECT COUNT(*)`,
called with `search: None`. The toolbar (`count_label_for`) treats
`filter_count` as "the section size ignoring search" and `matched_count` as
"what survived search", so keeping those labels correct after the merge
requires the second query; the controller issues it before setting
`search`, and on error keeps the previous `filter_count_cache` (same
keep-stale-on-failure policy as #355's facet refresh).
