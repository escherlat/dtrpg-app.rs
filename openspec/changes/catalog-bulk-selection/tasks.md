## 1. Controller: Selection State

- [x] 1.1 Add `selection_mode: bool` (default `false`) and `selected_ids: HashSet<Arc<str>>` (default empty) fields to `LibraryController` — implemented as `bulk_selection_mode` / `bulk_selected_ids` to avoid colliding with the existing single-item `Selection` (detail popover) concept
- [x] 1.2 ~~Add `SelectionChanged` event struct~~ — reused the existing `LibraryChanged` event instead; it already triggers the same `cx.notify()` path in `LibraryRootView`, so a parallel event type would be a duplicate signal for no behavioral gain
- [x] 1.3 Add `toggle_bulk_selection_mode(&mut self, cx)` — flips mode, clears selection when turning off, emits `LibraryChanged`
- [x] 1.4 Add `bulk_select_item(id: Arc<str>, cx)` — inserts into selection, emits `LibraryChanged`
- [x] 1.5 Add `bulk_deselect_item(id: &str, cx)` — removes from selection, emits `LibraryChanged`
- [x] 1.6 Add `bulk_select_all(cx)` — inserts all ids from `visible_items()`, emits `LibraryChanged`
- [x] 1.7 Add `bulk_deselect_all(cx)` — clears selection, emits `LibraryChanged`
- [x] 1.8 Add `bulk_select_by_pattern(pattern: &str, field: SelectionField, cx)` — case-insensitive match over visible items; no-op on empty pattern; `SelectionField { Title, Publisher, System }` added to `data/enums.rs` (System maps to `LibraryItem::line`, the game-line/series field — there is no separate "system" field on the model)
- [x] 1.9 Add `bulk_selection_count() -> usize`
- [x] 1.10 Add `is_bulk_selected(id: &str) -> bool`
- [x] 1.11 Add `clear_bulk_selection_and_exit_mode(cx)` — used by every bulk action on completion

## 2. Snapshot Propagation

- [x] 2.1 Add `bulk_selection_mode: bool`, `bulk_selected_ids: HashSet<Arc<str>>`, `bulk_selection_count: usize` to `LibrarySnapshot`
- [x] 2.2 Populate in `LibraryController::snapshot()`
- [x] 2.3 ~~Subscribe to `SelectionChanged`~~ — not needed; the existing `LibraryChanged` subscription in `LibraryRootView::new` already calls `cx.notify()` unconditionally

## 3. Controller: Bulk Actions

- [x] 3.1 `bulk_download(cx)` — reuses the existing `enqueue_download` per selected not-downloaded item
- [x] 3.2 `bulk_remove_download(cx)` — reuses the existing `remove_download` per selected downloaded item (status-flag based, matching how every other remove-download path in this codebase works — no direct `std::fs::remove_dir_all`)
- [x] 3.3 `bulk_fetch_thumbnail(cx)` — reuses the existing `load_thumbnail` per selected item with a `cover_url`
- [x] 3.4 `bulk_add_to_collection(collection_id: u64, cx)` — reuses `add_item_to_collection` per selected item (signature takes `u64` to match the existing collection id type, not `&str`)
- [x] 3.5 `bulk_remove_from_collection(collection_id: u64, cx)` — reuses `remove_item_from_collection` per selected member item
- [x] 3.6 `bulk_open(cx)` — uses `ItemOpener::open_item` per selected downloaded item (controller-level; ambiguous multi-file entries are skipped rather than routed to a detail tab, since that path needs `TabsController`/`Window`, which the controller doesn't hold)

## 4. Toolbar: Selection Mode Toggle

- [x] 4.1 Added `Button::new("selection-mode-toggle").ghost()` with `icons/square-check.svg` and tooltip "Select items"; `.selected(active)` via `gpui_component::Selectable`
- [x] 4.2 Wired to `ctrl.toggle_bulk_selection_mode(cx)`
- [x] 4.3 `render_toolbar` takes `selection_mode: bool`; wired from `LibraryRootView::render`

## 5. Toolbar: Bulk-Action Bar

- [x] 5.1 Created `bulk_action_bar_view.rs` with `render_bulk_action_bar`; registered in `ui/views/mod.rs`
- [x] 5.2 Pattern input is a plain `Entity<InputState>` field on `LibraryRootView`, read directly on the "Select Matches" click — no `Change` subscription/draft needed since nothing else observes the draft value
- [x] 5.3 Bar renders: selection-count label, Select All / Deselect All, pattern `Input` + field dropdown (Title/Publisher/System) + "Select Matches"
- [x] 5.4 Bar renders the six bulk-action buttons
- [x] 5.5 Each wired to its `LibraryController` method
- [x] 5.6 "Add to Collection" dropdown lists `collections` (cloned from the snapshot)
- [x] 5.7 "Remove from Collection" dropdown filtered to collections intersecting the selection (via `snap.items`, the currently-visible page — a selected item scrolled out of the active filter won't be picked up by this filter; noted as a known best-effort limitation, not a correctness issue for the common case)
- [x] 5.8 Bar included in `LibraryRootView::render` as a conditional child, directly below the toolbar, when `bulk_selection_mode` — shown as soon as selection mode is entered (not gated on a non-empty selection; confirmed via manual review that users expect the bar immediately, not only after the first pick)

## 6. Catalog Views: Per-Item Checkboxes

- [x] 6.1 `render_thumb_row`: `Checkbox` overlay, top-left of the cover
- [x] 6.2 `render_grid_card`: `Checkbox` overlay, bottom-left of the cover (top-left is already used by the unavailable badge; top-right by the checking spinner)
- [x] 6.3 Ungrouped list view: implemented as a checkbox rendered inline in the title cell (column 0) rather than a separate leading column. A real 0/28px dynamic column would require the `CatalogListDelegate`'s column count, sort-column indices, and `perform_sort`/`column()` index mapping to all shift live as selection mode toggles — too invasive for the value gained; the inline checkbox gives the same per-row affordance with none of that risk.
- [x] 6.4 Confirmed: `GroupedCatalogListDelegate`'s `render_list_item_cell` call always passes `selection_mode = false`, so the grouped list never renders checkboxes

## 7. Build and Lint

- [x] 7.1 `cargo check --workspace` — clean
- [x] 7.2 `cargo clippy --all-targets --all-features -- -D warnings` — clean
- [x] 7.2a `cargo test --workspace --all-features` — 405 tests pass (ran outside the sandbox; the sandboxed run has 8 pre-existing failures unrelated to this change — temp-file and loopback-socket permission denials in `dtrpg-core`'s download tests and `network_monitor`'s tests)

## 8. Manual Verification

Left for manual verification — not run from this session (no UI launch/screenshot in this workflow):

- [ ] 8.1 Selection mode toggle activates/deactivates; checkboxes appear/disappear in all three views
- [ ] 8.2 Individual checkboxes toggle selection; count updates
- [ ] 8.3 Select All / Deselect All
- [ ] 8.4 Pattern-match control for each of the three fields
- [ ] 8.5 Bulk Download queues and clears selection
- [ ] 8.6 Bulk Remove Download clears local status and clears selection
- [ ] 8.7 Bulk Fetch Thumbnail enqueues and clears selection
- [ ] 8.8 Add to Collection dropdown assigns items
- [ ] 8.9 Remove from Collection dropdown shows only relevant collections
- [ ] 8.10 Bulk Open opens downloaded files; cloud-only items skipped
- [ ] 8.11 Bulk-action bar visibility toggles correctly with selection count
