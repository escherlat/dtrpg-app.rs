# Tasks

## 1. Index facet queries (`dtrpg-core`)

- [x] 1.1 Added `data/index/facets.rs`: `publisher_counts()` — `GROUP BY
      publisher ORDER BY publisher` query (the `ORDER BY` pins the pre-sort
      order so names differing only by case have a deterministic tie), sorted
      case-insensitively in Rust over the grouped rows; returns
      `Vec<(Arc<str>, usize)>`. Not `COLLATE NOCASE`: SQLite folds ASCII only,
      the Rust scan folds Unicode. Unit tests: grouping, counts,
      case-insensitive ordering, empty index.
- [x] 1.2 `section_counts(now_secs, window_days)` in `facets.rs` composing
      `count_all`/`count_by_status`/`count_recently_updated` into a
      `SectionCounts` (derive gained `PartialEq, Eq` for the assert). Identity
      test against the pure `data::library::section_counts` scan over the same
      fixture, which exercises both recency paths (`date_updated` priority via
      `COALESCE`/`.or()`, `date_added` fallback, no-timestamps) and asserts
      nonzero recency first so a degenerate 0-vs-0 pass can't hide a wrong
      rule.
- [x] 1.3 Both exposed through the `LibraryIndex` facade in `mod.rs` (thin
      delegating wrappers, same pattern as `count_all`/`list_items_filtered`).

## 2. Controller rewiring (`dtrpg-ui`)

- [x] 2.1 `refresh_sidebar_facets()` (counts + publishers, returns whether the
      publisher list was refreshed so callers can skip
      `revalidate_publisher_filter()` against stale numbers) replaces the
      `all_items()` + `section_counts(&all)` + `publisher_entries(&all)` trio
      at `set_catalog`, `apply_partial_fetch`, `append_catalog_page`. On query
      failure previous values are kept, logged via `tracing::warn`.
- [x] 2.1b (beyond the three load sites; found in review) five per-item
      status-change sites scanned the whole catalog for `section_counts` only:
      `downloads.rs`, `checks.rs` (file-tag sync + availability result),
      `download_dispatch.rs`, `concurrency.rs` (window-days settings change).
      `ItemStatus` has exactly `Downloaded`/`Cloud`, and status changes never
      move an item between publishers, so these use a counts-only
      `refresh_section_counts()`. The pure `data::library::section_counts`
      scan now has no production callers; it stays as the identity reference
      for the SQL composition.
- [x] 2.2 Kept `group_by_publisher()`/`group_by_tag()` in Rust (see proposal
      scope note); `util/publisher.rs` and its tests untouched.

## 3. Verification

- [x] 3.1 `cargo check --workspace --all-targets`, `make lint` (clippy
      `-D warnings`, all targets), `cargo test --workspace` (523 core tests
      incl. 4 new facet tests, 339 ui, 8 mcp, doctests, all pass),
      `make fmt-check`, `make size-check` — all clean.
- [x] 3.2 UI-suite seed check: `for s in 1 2 7 13; do SEED=$s cargo test -p
      dtrpg-ui; done` — all four seeds pass (NOTES.md scheduler-seed gotcha).

## Workflow follow-up

- This change implements issue #355, the tracking issue filed from
  `catalog-reads-from-index`'s tasks 3-4 at archive time; close #355 on merge.
  The parent's tasks.md §3.1-4.2 deferred entries are what this change
  resolves. Tag grouping's SQL side and SQL-side search remain tracked by
  #353.
