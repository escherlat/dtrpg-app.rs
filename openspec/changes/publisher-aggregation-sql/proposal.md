# Proposal

## Why

Issue pilgrimagesoftware/dtrpg-app.rs#355 (tasks 3-4 of the
`catalog-reads-from-index` change): after PR #354 made the library index the
catalog's only storage, three aggregations still run in Rust over
`self.all_items()` — a full fetch of every item, materialized just to count
per publisher or per section. The sidebar's publisher list and section counts
are recomputed on every catalog-load completion (`set_catalog`,
`apply_partial_fetch`, `append_catalog_page`) by scanning the whole catalog;
the index already answers the status/recency half of that question with the
facet queries added in ADR 0003 (`count_all`, `count_by_status`,
`count_recently_updated`), which nothing calls yet.

## What Changes

- Add `LibraryIndex::publisher_counts()` — `SELECT publisher, COUNT(*) FROM
  items GROUP BY publisher`, sorted case-insensitively in Rust (SQLite's
  `NOCASE` collation is ASCII-only and would mis-order non-ASCII publisher
  names differently than today's `to_lowercase()` sort).
- Add `LibraryIndex::section_counts(now_secs, window_days)` — composes the
  three existing facet queries into the `SectionCounts` the sidebar renders,
  with an identity test against the pure `data::library::section_counts`
  scan over the same fixture data.
- Replace the three load-completion sites' `publisher_entries(&all)` /
  `section_counts(&all, ..)` calls (and their `self.all_items()` full fetch)
  with the two index queries. Also replace five per-item status-change sites
  that scanned the whole catalog for `section_counts` alone (`downloads`,
  `checks` ×2, `download_dispatch`, `concurrency`): `ItemStatus` has exactly
  `Downloaded`/`Cloud` and a status change never moves an item between
  publishers, so those need only the counts, via a dedicated
  `refresh_section_counts()`.
- Keep the `invalidate_cache()`-triggered refresh model: index reads happen
  on load completion or status change, never on the render path (AGENTS.md).
- No capability spec delta (`skip_specs`): the sidebar's observable behavior
  — counts, publisher ordering, filter revalidation — is unchanged; this is a
  storage-mechanism swap, the same rationale as the parent change. The
  identity test pins that equivalence.
- `group_by_publisher()` / `group_by_tag()` stay in Rust, unchanged: they
  partition the *current result set* (post-search, cached), not the catalog.
  Moving them to SQL needs the search-box semantics in SQL (#353) first, and
  querying from `grouped_items()` would put I/O on the render path. This is
  a scope correction to #355, not a silent drop.

## Outcomes

Sidebar aggregation stops materializing the full catalog on every load pass
(~12 ms at 10,000 items avoided per site); the ADR 0003 facet queries get
their first callers.
