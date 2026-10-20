# Tasks

**Scope change mid-implementation:** tasks 1.2/1.3 were written for an incremental migration with
`index: Option<LibraryIndex>` and a permanent `Vec` fallback. The owner overrode that explicitly:
no fallback, no dual bookkeeping — remove `catalog: Vec<LibraryItem>` outright and make the index
the controller's only storage. What's checked off below reflects the rip-and-replace that was
actually built, not the incremental plan originally written.

## 1. Core filter/sort query

- [x] 1.1 Added `LibraryIndex::list_items_filtered` (`ItemFilter` struct: `status`, `publisher`,
      `collection_member_ids`, `recently_updated`) in
      `crates/dtrpg-core/src/data/index/filtered_items.rs`, covering every `SidebarFilter` variant
      except `Tag` (resolved in Rust against its pre-computed id set, per design.md) and every
      `SortMethod` including `Custom`'s closed `col_key` set. No search parameter — search stays
      in-memory, run over the index-filtered set. 13 unit tests (one per filter/sort combination).
- [x] 1.2 Removed `catalog: Vec<LibraryItem>` from `LibraryController` entirely. Added
      `index: LibraryIndex` (non-optional — opened once in `build_with_cache_root()`; `.expect()`s
      if it can't open, since there is no fallback to degrade to) and two session-only maps,
      `thumbnail_attempts`/`availability_checks` (the two `LibraryItem` fields the index never
      persists — restored via `Self::overlay_ephemeral_state`/`item_by_id`/`all_items` right after
      every index fetch, so the UI-facing `LibraryItem` shape is unchanged). `invalidate_cache()`
      now builds an `ItemFilter` from `self.filter` unconditionally and queries the index for
      every variant, including `Publisher`/`Collection` (2.1/2.2's work, done in the same pass
      rather than a later task, since there was no fallback left to lean on).
- [x] 1.3 N/A as written (no dead branch — see scope-change note above). Verified instead that
      `cargo clippy --workspace --all-targets -- -D warnings` is clean and `cargo test -p
      dtrpg-ui` passes with the Vec gone.

## 2. Publisher and collection filters

- [x] 2.1 `ItemFilter::collection_member_ids: Option<&HashSet<u64>>` takes the controller's own
      `collection_members` (kept current by optimistic edits) rather than joining the index's
      `collection_membership` table, and matches both `order_product_id` and `product_id` — done
      as part of 1.2, see above.
- [x] 2.2 `SidebarFilter::Publisher`/`Collection` migrated to the index query in the same
      `invalidate_cache()` rewrite. `cargo test -p dtrpg-ui` passes (339 tests).

## 3. Publisher aggregation (sidebar counts)

- [ ] 3.1 Not done. `publisher_entries()` (`util/publisher.rs`) still takes `&[LibraryItem]`;
      every call site now passes `self.all_items()` (an index fetch) instead of `&self.catalog`,
      but the aggregation itself still runs in Rust, not as a `GROUP BY` query. Scanning an
      already-fetched `Vec` in memory for a count aggregation is a different cost shape than the
      per-keystroke filter/sort scan this change targeted — left as a follow-up, not silently
      dropped.
- [ ] 3.2 Depends on 3.1.

## 4. Grouped presentation (publisher/tag grouping)

- [ ] 4.1 Not done, same reasoning as 3.1 — `group_by_publisher()`/`group_by_tag()` still run in
      Rust over `self.all_items()`.
- [ ] 4.2 Depends on 4.1.

## 5. Pagination

- [ ] 5.1/5.2 Not done. `visible_items_slice()` still slices the fully-materialized
      `items_cache: Arc<[LibraryItem]>` `invalidate_cache()` builds — that cache is still one
      in-memory `Vec` by design (it's the render-path read, and the render path must do zero
      I/O), so `LIMIT`/`OFFSET` on the index query was never actually needed: the index query that
      feeds the cache already runs once per filter/sort/search change, not once per rendered row.

## 6. Cleanup and verification

- [x] 6.1 Every catalog read and write in `controllers/library/` goes through `self.index` or
      `self.all_items()`/`self.item_by_id()`/`self.overlay_ephemeral_state()` now — confirmed via
      `grep -rn "self\.catalog\b"` returning zero production call sites. No call site deliberately
      still reads a `Vec` — there isn't one left to read.
      Two now-redundant background `save_catalog_cache` write-throughs were removed outright
      (`catalog_sync_autoload.rs`, `catalog_sync_fetch.rs`'s final save, and
      `download_dispatch.rs`'s `save_catalog_in_background`), since every mutation already writes
      to `self.index` synchronously at the point of mutation.
- [x] 6.2 Full verification, all clean: `cargo check --workspace --all-targets`, `cargo clippy
      --workspace --all-targets -- -D warnings`, `cargo test --workspace` (339 `dtrpg-ui` tests + 5
      doctests pass; `dtrpg-core` has 22 pre-existing failures unrelated to this change — local
      network/socket-binding tests blocked by this environment's sandbox, confirmed present before
      this change too), `make size-check`, `make fmt` + `make fmt-check`.

## Workflow follow-up

- Tasks 3-5 (publisher/tag aggregation as `GROUP BY` queries, SQL-side pagination) are real,
  scoped-out follow-up work, not abandoned — file a tracking issue before archiving this change if
  they're still wanted, since this change's own tasks.md won't be revisited once archived.
- Archive this change with `openspec-archive-change` once reviewed.
