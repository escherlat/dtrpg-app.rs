# Proposal

## Why

Issue pilgrimagesoftware/dtrpg-app.rs#353. Since PR #354,
`invalidate_cache()` runs one index query for the sidebar filter/sort and then
applies the search box and the `Tag` filter as two in-memory Rust passes over
the fully-materialized result. The search box's real matcher,
`SearchQuery::matches()` (`util/search.rs`), is substring-anywhere
(Unicode-cased) over title/publisher/line/tag names plus a `tag:`/`-tag:`
inclusion/exclusion mini-syntax — none of which maps onto the index's FTS5
`MATCH` (word tokens, ASCII folding). #355 deliberately kept search in-memory;
this change implements the SQL side so the search is a query predicate, which
also unblocks SQLizing `group_by_publisher`/`group_by_tag` later.

## What Changes

- Register a custom SQL scalar function `ulower(text)` (Rust
  `str::to_lowercase()`) on every `LibraryIndex` connection at open. SQLite's
  `lower()`, `LIKE`, and `COLLATE NOCASE` fold ASCII only; the query text is
  produced by Rust `to_lowercase()`. Sharing the exact fold makes the
  substring match identical by construction rather than by approximation.
- Add two dimensions to `ItemFilter`: `search: Option<&SearchQuery>` and
  `tag_name: Option<&str>`. The query itself is still parsed in Rust by
  `SearchQuery::parse` — SQL receives the parsed parts, not the raw string.
  Predicates: free text becomes `instr(ulower(col), ?k) > 0` ORed across
  title/publisher/line plus an `EXISTS` over `item_tags`/`tags`; each `tag:`
  term an `EXISTS (… ulower(tg.name) = ?)`, each `-tag:` a `NOT EXISTS`; the
  `Tag` sidebar filter the same `EXISTS` by tag name (case-insensitively,
  matching `TagLibrary::items_with`).
- Add `LibraryIndex::count_items_filtered(&ItemFilter)` — same WHERE builder,
  `SELECT COUNT(*)`. Required because the toolbar's `filter_count` means
  "items matching the filter alone, search ignored", which the single
  list query can no longer answer once search is one of its predicates.
- `invalidate_cache()` drops both Rust passes: count with the filter minus
  search, then list with search set. `SidebarFilter::Tag`'s pre-computed id
  set no longer filters anything (the enum and its rebuild-on-tag-change
  behavior stay; the set stays populated for now).
- Restructure `filtered_items.rs` (now 334 lines) into a `filtered_items/`
  module: `clause.rs` (the dimensions and the shared WHERE builder),
  `query.rs` (`list_items_filtered`/`count_items_filtered`, existing tests),
  `parity_tests.rs` (the equivalence bar below). `mod.rs` declares only, per
  AGENTS.md.
- No spec delta (`skip_specs`): the search box's observable results are
  required not to change — that equivalence is the change's acceptance test.

## Outcomes

Search runs as part of the single index query (only surviving items get
materialized and file-loaded, instead of the whole pre-search set plus two
in-memory scans), and the `Tag` filter's last Rust pass disappears.
