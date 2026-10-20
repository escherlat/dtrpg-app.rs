# Design

## Why not a true `GROUP BY`?

SQL `GROUP BY` with aggregation collapses each group to one row, but the
UI needs every item under every group header. A real grouped query would
have to return `(group_key, item)` rows — which is exactly what an
`ORDER BY group_key, ...` query already gives us, with the added benefit
that Rust can split the stream on the fly without materializing a HashMap.
So "SQLizing grouping" here means *ordering for grouping*, not SQL
`GROUP BY` clauses.

## Render-path invariant

`CatalogView::grouped_items` is called during render. Today it computes
`group_by_publisher`/`group_by_tag` over `controller.visible_items()`, but
it also caches the result in `self.grouped_cache`, so the computation only
runs once per `LibraryChanged`. The new design removes even that single
computation from the render path: `invalidate_cache()` builds the grouped
cache, and `grouped_items` merely clones it.

`set_group_by` currently emits `LibraryChanged` without invalidating the
catalog cache. That is fine today because the view recomputes groups lazily,
but it is not fine once grouping needs SQL. We change `set_group_by` to call
`invalidate_cache()`, which is cheap because it reuses the same single query
now extended with ordering.

## Query shapes

Both queries reuse `ItemFilter::build_where` so filter/search/tag
dimensions never drift from the flat list query.

### Publisher

```sql
SELECT * FROM items
WHERE ...
ORDER BY ulower(publisher), <user_sort>
```

`ulower` is the custom Unicode-lower scalar added in #353. Rows with the
same lower-cased publisher are adjacent; the Rust split breaks the stream
when the *exact* `publisher` value changes. This preserves the current
behavior where `"Paizo"` and `"paizo"` are separate groups even though they
sort next to each other.

Items within a publisher group stay in the user's chosen sort order because
`<user_sort>` is the secondary key.

### Tag

```sql
SELECT items.*, tg.name AS group_name
FROM items
JOIN item_tags it ON it.item_id = items.id
JOIN tags tg ON tg.id = it.tag_id
WHERE ...
ORDER BY ulower(tg.name), <user_sort>, items.id
```

An item with multiple tags appears once per tag, adjacent to other items
with the same lower-cased tag. The Rust split breaks on exact `group_name`
change. To match the current `TagLibrary::entry` case-insensitive lookup,
the split key is the lower-cased tag, and the group title is the first
original `tg.name` encountered for that lower-cased key.

Untagged items are fetched separately:

```sql
SELECT * FROM items
WHERE ...
  AND NOT EXISTS (SELECT 1 FROM item_tags WHERE item_id = items.id)
ORDER BY <user_sort>
```

and appended as the final group titled by the localized `catalog.untagged`
string.

## Moving `ItemGroup` to core

`ItemGroup` is pure data: `title: Arc<str>` + `items: Vec<LibraryItem>`.
Moving it to `dtrpg-core` lets the index return groups directly without a
UI→core dependency cycle. `dtrpg-ui` re-exports or uses it via
`dtrpg_core::util::publisher::ItemGroup`.

## Controller cache

- `grouped_items_cache: Option<Arc<Vec<ItemGroup>>>` is added to
  `LibraryController`.
- `invalidate_cache()` runs the flat list query as today, then — if
  `self.group_by != GroupBy::None` — runs the grouping query and splits
  the rows into `Vec<ItemGroup>`.
- `visible_items_grouped()` returns `Arc::clone(&self.grouped_items_cache)`.
- `CatalogView::grouped_items` uses that method and removes its own
  computation + `grouped_cache` field.

## Parity bar

A new test file seeds items with mixed-case publishers/tags, multi-tag
items, untagged items, and Unicode names. It runs:

1. the old `group_by_publisher(items)` / `group_by_tag(items, tags, ...)`;
2. the new SQL-ordered path over the same index state;

and asserts identical group titles, group order, and item-id order within
each group. The old functions are kept as the oracle.

## Known divergence

Tag grouping now reads the index's `tags`/`item_tags` tables instead of the
controller's `TagLibrary` snapshot — the same freshness window already
accepted in #353's design.md. Both converge after `load_tags` completes.
