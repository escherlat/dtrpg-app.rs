# Proposal

## Why

`LibraryController` filters, searches, sorts, and groups the catalog by scanning an
in-memory `Vec<LibraryItem>` on every `invalidate_cache()` call (`controllers/library/catalog.rs`,
`filtering.rs`), even though the catalog has been durably indexed in SQLite with FTS5 since
ADR 0003. This duplicates the source of truth (Vec vs. index) and leaves the index underused:
today it only serves `search()`, `count_all`, `count_by_status`, and `count_recently_updated`.
claude-2's `ARCHITECTURE-REVIEW.md` delivery sequence calls this out as step 2 ("make local
state authoritative for presentation") — the step after the catalog/index already exist, before
multi-root downloads and the expanded inspector build further on top of it.

## What Changes

- Add index query methods for everything the UI currently computes in-memory: filtering by
  publisher, tag, and collection membership; sorting by every `SortMethod` variant in either
  direction; paginated/sliced reads; and publisher/tag aggregation with counts (today's
  `publisher_entries()`/`group_by_publisher()`/`group_by_tag()`).
- Migrate `LibraryController`'s filter/search/sort/group/count call sites from the in-memory
  `Vec` scan to these index queries, one call site at a time, keeping output identical at each
  step (same items, same order, same counts) so behavior is unchanged to the user.
- Keep the `invalidate_cache()`-triggered refresh model: index reads happen on filter/search/sort
  change, never on every render frame or keystroke, preserving this codebase's "no I/O on the
  render path" rule (`AGENTS.md`).
- The in-memory `Vec<LibraryItem>` catalog field itself is not necessarily removed in this
  change — call sites still needing raw items (thumbnail state, collection-membership IDs,
  download dispatch) are evaluated case by case in `design.md`; what's committed here is that
  filter/search/sort/group/count read paths stop scanning it.

## Capabilities

### New Capabilities

(none — this changes implementation, not observable behavior)

### Modified Capabilities

(none — every existing catalog filter/sort/search/count/group requirement in
`rust-library-ui-implementation`, `catalog-auto-load-policy`, `sidebar-section-counts`,
`alphabetical-publisher-list`, etc. must continue to hold exactly as specified; this change
is how the index answers them, not a change to what they require)

## Impact

- `crates/dtrpg-core/src/data/index/` — new query methods (filter/sort/paginate/aggregate),
  likely a new `items.rs` submodule section or sibling file per the size-check convention.
- `crates/dtrpg-ui/src/controllers/library/catalog.rs`, `filtering.rs` — call sites migrated
  from Vec scan to index queries.
- `crates/dtrpg-ui/src/util/publisher.rs` — `publisher_entries()`/`group_by_publisher()`/
  `group_by_tag()` migrate to (or are replaced by) index-side aggregation.
- No API-facing or on-disk schema change: the index schema from ADR 0003 already has the
  columns needed (publisher, tags via `item_tags`, collection membership, status, timestamps);
  this adds query methods against it, not new tables.
