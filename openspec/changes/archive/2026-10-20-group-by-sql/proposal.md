# Proposal

## Why

Issue pilgrimagesoftware/dtrpg-app.rs#361. `group_by_publisher` and
`group_by_tag` (`dtrpg-ui/src/util/publisher.rs`) still partition the
post-search cached result set in Rust. #355 deliberately kept them in Rust
because they needed #353 (search-box semantics in SQL) first; #353 has now
landed. This change moves the grouping ordering into the index query and
caches the grouped result in the controller, so the render path does no
new SQLite I/O.

## What Changes

- Move `ItemGroup` from `dtrpg-ui/src/util/publisher.rs` to `dtrpg-core`
  (it is pure data: a title plus a `Vec<LibraryItem>`).
- Add `LibraryIndex::list_items_for_grouping(..., group_by: GroupBy)`.
  The query applies the same filter/search/sort dimensions as
  `list_items_filtered`, then orders the result so Rust can split it into
  groups with a single linear pass:
  - `GroupBy::Publisher`: `ORDER BY ulower(publisher), <user_sort>, id`.
  - `GroupBy::Tag`: `SELECT items.*, tg.name AS group_name FROM items
    JOIN item_tags ... JOIN tags ... ORDER BY ulower(group_name), <user_sort>,
    items.id`, plus a second query for untagged items.
- `LibraryController` gains a `grouped_items_cache: Option<Arc<Vec<ItemGroup>>>`
  built in `invalidate_cache()` whenever the current `group_by` is not
  `None`. `set_group_by` now calls `invalidate_cache()` so a group-mode
  change rebuilds the cache exactly once, not on the next render.
- `CatalogView::grouped_items` stops computing groups and reads the
  controller cache instead. Its existing view-level `grouped_cache` is
  removed or becomes a simple mirror.
- The old `group_by_publisher`/`group_by_tag` Rust partitioners are kept as
  the specification the SQL-ordered split is tested against.

## Outcomes

Grouping follows the same index source-of-truth as search and tag filters;
the render path reads only cached data; and the observable grouped view is
byte-identical to the current Rust partitioner.
