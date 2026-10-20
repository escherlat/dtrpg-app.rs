# Design

## Context

`LibraryController` (`crates/dtrpg-ui/src/controllers/library/`) holds the catalog as
`Vec<LibraryItem>`. `invalidate_cache()` (`catalog.rs`) is the one place that turns filter +
search + sort state into the `items_cache: Arc<[LibraryItem]>` the render path reads
(`filtering.rs`'s `visible_items()`/`visible_items_shared()`/`visible_items_slice()`); it runs
on every `set_filter`/`set_search_query`/`set_sort`/`set_sort_direction` call, not on every
render frame. That refresh boundary is the integration point this design slots into — nothing
changes about *when* a scan happens, only *where* (SQL query vs. `Vec::retain`/`sort_by`).

The index (`crates/dtrpg-core/src/data/index/`, ADR 0003) already has every column this needs:
`items.publisher`, `items.status`, `items.date_added`/`date_updated`, `item_tags`/`tags`,
`collection_membership`. `SidebarFilter` (`dtrpg_core::util::filter`) has six variants:
`AllTitles`, `RecentlyUpdated`, `OnDevice`, `InCloud`, `Publisher(Arc<str>)`,
`Collection(u64, Arc<str>)`, and `Tag { name, items: Arc<HashSet<Arc<str>>> }` — the last
already carries a pre-resolved id set rather than a tag reference, so it needs no join.
`SortMethod` (`dtrpg_core::util::sort`) has `Title`, `Publisher`, `DateAdded`, `PageCount`, and
`Custom { col_key: &'static str }`; `col_key` is open-ended in the type but closed in practice —
`sort_items()`'s own match only recognizes `"publisher"`, `"system"`, `"pages"`, `"size"`,
`"added"`, falling back to title sort for anything else.

## Goals / Non-Goals

**Goals:**
- Every index query this change adds returns results identical (same items, same order, same
  counts) to what the in-memory scan it replaces produced for the same filter/sort/search state —
  this changes the backing store, not observable behavior.
- Index reads happen only where the Vec scan used to happen (on `invalidate_cache()` and the
  controller's other catalog mutation/read points), never added to the render path.
- **Revised mid-implementation, explicit owner override:** `catalog: Vec<LibraryItem>` is removed
  from `LibraryController` entirely — the library index is the catalog's only storage, with no
  fallback and no dual bookkeeping. The two fields the index can't persist
  (`thumbnail_last_attempted`, `availability_last_checked`) move to two session-only
  `HashMap<Arc<str>, SystemTime>` fields on the controller, overlaid onto every item fetched from
  the index so the UI-facing `LibraryItem` shape is unchanged. This superseded the original
  incremental-migration-with-fallback goal below.

**Non-Goals:**
- Changing the on-disk schema. Every column/table this needs already exists from ADR 0003.
- Changing `SidebarFilter`/`SortMethod`'s shape, or the search box's matching semantics.
- Pushing the search box's matching into SQL. `SearchQuery::matches()` (`util/search.rs`) does
  substring-anywhere matching over title/publisher/**line**/tags plus a `tag:`/`-tag:`
  inclusion/exclusion mini-syntax — not what `search()`'s FTS5 query does (word-token match over
  title/publisher/description/file_names, no tags, no line, no `tag:` operator, no substring).
  Replicating `SearchQuery`'s exact semantics in SQL is its own project, tracked separately as
  dtrpg-app.rs#353; it is explicitly not attempted here.
- Server-side (API/MCP) exposure of these queries. This is `LibraryController`-internal.

## Decisions

**Filter and search compose, but search stays in-memory, run over the index-filtered set.**
`invalidate_cache()` (`controllers/library/catalog.rs`) applies the sidebar filter and the search
query together: `filter_count` is the count after the filter alone, and the final `items_cache`
additionally requires `SearchQuery::matches()` to pass. Since `SearchQuery`'s semantics don't map
to SQL without the separate project noted above, `list_items_filtered` returns the
sidebar-filtered, sorted set only; `SearchQuery::matches()` still runs in Rust afterward, exactly
as today, just over that (usually much smaller) set instead of the full catalog. `filter_count`
comes directly from the index query's result length — no behavior change, and no second query
needed for it.

**One parameterized query function per concern, not a generic filter-builder.**
`list_items_filtered(status: Option<ItemStatus>, publisher: Option<&str>,
collection_member_ids: Option<&HashSet<u64>>, recently_updated_since: Option<i64>, sort:
SortMethod, direction: SortDirection) -> Vec<LibraryItem>` on `LibraryIndex`, built as one
`WHERE`/`ORDER BY` clause assembled from `Option`s that are each `None` for `AllTitles`. Rejected
a generic `QueryBuilder`/criteria-object API: `SidebarFilter` only ever sets one sidebar
dimension at a time today (publisher *or* collection *or* recency *or* status, never combined),
so a single function with optional parameters covers every real call shape without the
indirection of a builder nobody composes multiple ways yet (YAGNI).

**`SortMethod::Custom`'s `col_key` maps via the same closed match `sort_items()` already uses.**
`sort_items()`'s fallback arm (`_ => sort by title`) is preserved as the query function's fallback
`ORDER BY`, so an unrecognized `col_key` behaves identically index-side and in-memory.

**`LibraryController` holds one persistent `LibraryIndex`, opened once, not per query.**
`load_catalog_cache`/`save_catalog_cache` each call `LibraryIndex::open_and_migrate` fresh per
invocation today, which is fine for their call frequency (once per catalog load/save) but wrong
for `invalidate_cache()`, which runs on every search-box keystroke — reopening a SQLite
connection (plus its migration check) that often is a real regression, not a neutral swap.
`LibraryController::build()` opens the index once and keeps it as `index: Option<LibraryIndex>`
(`None` on open failure, mirroring `load_catalog_cache`'s own tolerate-and-fall-back-to-None
convention rather than panicking or making `build()` fallible). A missing index is not just a
migration-period scaffold to delete later — it is this controller's permanent, intentional
degradation path alongside every other `Option`-returning disk operation in this codebase, so
`invalidate_cache()`'s `Some`/`None` branch on `self.index` stays, even after every filter this
change covers has an index-backed query.

**Tag filtering stays `HashSet`-based, not index-joined, in this change.**
`SidebarFilter::Tag` already carries its resolved id set; replacing that with a SQL join
(`item_tags` + `tags`) would require re-deriving what the set is for (a snapshot taken when the
tag filter was selected, deliberately not recomputed per the type's own doc comment) and is a
separate behavioral question, not a scan-location change. The id set itself was already not a
`Vec` scan.

**Publisher/tag aggregation (`publisher_entries()`, `group_by_publisher()`, `group_by_tag()`)
gets its own `GROUP BY` query (`list_publishers_with_counts()`), added after the filter/sort
queries land**, since the sidebar counts depend on the same filtered set those queries produce
and are easiest to get right once that's settled.

**Pagination is a `LIMIT`/`OFFSET` parameter on the same query function, added last.**
`visible_items_slice()` is the only caller that needs it (viewport virtualization); every other
caller wants the full filtered/sorted result. Until pagination lands, the query function returns
the full set and `visible_items_slice()` keeps slicing the `Vec` it gets back — still removes the
Vec *scan* (filter+sort), just not the slice.

## Risks / Trade-offs

- [Collection membership match is dual-keyed (`order_product_id` *or* `product_id`), per
  `catalog_ids()`'s existing fallback] → the `Collection` filter query must `WHERE member_id IN
  (?, ?)` against both columns, not just one, or it silently drops items the in-memory version
  would have matched. Call out explicitly in the task for that query.
- [The controller's `collection_members: HashSet<u64>` is updated optimistically on add/remove
  (`collection_membership.rs`) between full collection-list reloads, but the index's own
  `collection_membership` table is only ever replaced wholesale (`replace_all_collections`) — a
  `Collection` filter query joined against the index table directly could briefly disagree with
  an just-applied optimistic edit] → task 2.1's query must take the resolved member-id set as
  input (same `collection_members`/`member_ids` source the controller already has), not
  re-derive membership from the index's `collection_membership` table.
- [`filter_count` must still equal "items matching the sidebar filter alone," not the
  search-narrowed count — easy to swap by accident once two numbers (filter count, final cache
  length) come from two different computations again] → task 1.2's test asserts `filter_count`
  equals the index query's result length even when a search string further narrows
  `items_cache`.
- [A regression here is silent, not a crash — wrong sort order or a dropped item just looks like
  a minor display bug] → every migrated call site needs a test asserting index output matches a
  known-good fixture for each `SidebarFilter`/`SortMethod` variant before cutover, not just "it
  compiles."

## Migration Plan

Incremental, one query + one call site at a time, each landing as its own commit with its own
tests passing before the next starts:

1. `list_items_filtered` for `AllTitles`/`OnDevice`/`InCloud`/status-only filters + every
   `SortMethod` variant, no search, no pagination.
2. `Publisher`/`Collection` filter variants (resolves the dual-key risk above).
3. `publisher_entries()`/`list_publishers_with_counts()` and the sidebar's publisher counts.
4. `group_by_publisher()`/`group_by_tag()` for the grouped list/grid presentations.
5. Pagination (`LIMIT`/`OFFSET`) for `visible_items_slice()`.

Rollback is per-step: each call site keeps its in-memory fallback path reachable (not deleted)
until its replacement has run clean in manual testing, so a bad step reverts by restoring one
call site's old line, not by reverting the whole change.

## Open Questions

- Whether `section_counts()`'s three aggregate numbers (all/status/recent) should become one
  combined query or stay three index calls — doesn't change the approach or task breakdown,
  decide at implementation time based on measured cost.
