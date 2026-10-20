# Tasks

## 1. `ulower` custom SQL function

- [x] 1.1 `crates/dtrpg-core/Cargo.toml`: rusqlite gains the `functions`
      feature (kept `default-features = false, features = ["bundled"]` +
      `functions`).
- [x] 1.2 `data/index/functions.rs`: `register_functions` attaches a
      deterministic, UTF-8 `ulower(text)` scalar = Rust `str::to_lowercase()`.
      Registered on both `open_and_migrate` and the test-only
      `open_in_memory` connection, right after `schema::ensure_schema`
      (before any `ulower` query can be prepared).

## 2. Search/`Tag` dimensions in the query

- [x] 2.1 `SearchQuery` gained `pub(crate)` accessors (`free_text`,
      `required_tags`, `excluded_tags`) — parsing stays in Rust; SQL receives
      the parsed parts.
- [x] 2.2 Restructured `filtered_items.rs` (334 lines, over the new dimensions'
      addition) into `filtered_items/`: `mod.rs` (declares + re-exports only),
      `clause.rs` (`ItemFilter` + shared `build_where`), `query.rs`
      (`list_items_filtered`/`count_items_filtered`/`order_by_clause`, with the
      existing 13 filter/sort tests moved), `parity_tests.rs` (the equivalence
      bar). AGENTS.md: no `.rs` over 500 lines, `mod.rs` declares only.
- [x] 2.3 `ItemFilter` gained `tag_name: Option<&str>` (an `EXISTS` join on
      `item_tags`/`tags`, `ulower(tg.name) = <lowercased name>`) and
      `search: Option<&SearchQuery>` (free text via
      `instr(ulower(col), ?) > 0` across title/publisher/line plus a tag-name
      `EXISTS`; each `tag:` an `EXISTS`, each `-tag:` a `NOT EXISTS`).
      `ItemFilter` now derives `Debug` for assertion messages.

## 3. Count query + controller rewiring

- [x] 3.1 `count_items_filtered` (`query.rs`) + `LibraryIndex` facade method:
      shares `build_where` with the list query, `SELECT COUNT(*)`, no ORDER BY.
- [x] 3.2 `invalidate_cache()` (`dtrpg-ui/.../catalog.rs`) drops both Rust
      passes: it counts with the sidebar dimensions only (search ignored — the
      toolbar's `filter_count`), then lists with `search` set. `SidebarFilter::
      Tag` now feeds `tag_name`, not a post-query id-set filter. Count failure
      keeps the previous number (same policy as #355's facet refresh). The
      enum's pre-computed id set stays: `item_matches_filter` still reads it,
      serving "download all in section" (`filter_download_targets`), the
      change's one remaining `Tag.items` consumer (see design.md).

## 4. Verification

- [x] 4.1 Equivalence: `parity_tests.rs` runs a 23-case battery (word-crossing
      substrings, Unicode `É`/`ö` folds, every `tag:`/`-tag:` form, text×tag,
      `Tag` section, status×tag×search) asserting the SQL result == the old
      pipeline (unsearched SQL → Rust tag-id-set pass → `SearchQuery::matches`
      pass) survivor-for-survivor and order-for-order. A second test pins the
      fixture so a regression can't pass by matching nothing.
- [x] 4.2 `cargo test --workspace` (523 core incl. 4 new, 339 ui, 8 mcp,
      doctests, all pass; the 22 `dtrpg-core` network failures from the prior
      handoff did not recur in this sandbox run), `make lint`
      (`clippy --workspace --all-targets -D warnings`) clean, `make fmt-check`
      (pinned nightly) clean, `make size-check` clean, UI-suite seed check
      (SEED 1/2/7/13) all pass.

## Workflow follow-up

- Implements issue #355's predecessor #353. `group_by_publisher`/`group_by_tag`
  can now be revisited for SQLization (they partition the post-search set, and
  search is a query dimension as of this change) — that's separate, not in
  scope here.
