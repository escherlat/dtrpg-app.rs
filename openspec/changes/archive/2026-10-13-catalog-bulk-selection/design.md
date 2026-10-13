## Context

The catalog currently has no notion of a selection set. Each item is acted on individually via its context menu or detail panel. The existing `LibraryController` owns all catalog state; `LibrarySnapshot` carries render-time data to views. The toolbar renders a single row of controls. Catalog views (list/thumb/grid) render items from `LibrarySnapshot.items` without any selection affordance.

`Selection` already exists as a data type (`data/selection.rs`) tracking a single selected item for detail-panel display. The new multi-selection feature is distinct and must not conflict with that type.

## Goals / Non-Goals

**Goals:**
- Add multi-selection to `LibraryController` without changing single-item selection (detail panel) behavior.
- Keep selection mode strictly opt-in — zero visual overhead when inactive.
- All bulk actions route through the existing activity + library service layer.
- Collection picker reuses the existing collections data already in `LibrarySnapshot`.

**Non-Goals:**
- Drag-to-select or range-select via Shift+Click (keyboard/range selection deferred).
- Persistent selection across restarts.
- Selection inside the grouped list view (too complex for initial cut; ungrouped list, thumb, and grid only).

## Decisions

### Selection state lives in `LibraryController`

**Decision**: Add `selection_mode: bool` and `selected_ids: HashSet<Arc<str>>` directly to `LibraryController`, propagated via `LibrarySnapshot`.

**Rationale**: All other catalog state (filter, sort, page) lives there. Keeping selection co-located means filter/search/page changes can correctly invalidate out-of-range selections. A separate controller would require cross-entity coordination with no clear benefit.

**Alternative considered**: A separate `SelectionController` entity. Rejected — it would need to observe `LibraryChanged` events to trim stale IDs, which adds coupling without encapsulation.

### Bulk-action bar is part of the toolbar area, not a floating overlay

**Decision**: Render the bulk-action bar as a second row beneath the normal toolbar, visible whenever `selection_mode` is on (not gated on a non-empty selection — manual review found users expect the bar as soon as they enter selection mode, not only after picking an item). Included conditionally via `Option::then(...)`.

**Rationale**: A floating overlay competes with the catalog content and complicates hit-testing. A toolbar row keeps the catalog area clean, uses available horizontal space efficiently, and is trivially dismissed by deselecting all or toggling off selection mode.

**Alternative considered**: A bottom action bar (like iOS share-sheet). Rejected — the window chrome is top-anchored and a bottom bar would require layout restructuring.

### Collection picker uses a `DropdownMenu` (same pattern as sort picker)

**Decision**: Add to Collection and Remove from Collection use `Button::dropdown_menu` to show an inline popup list of collections, identical to the sort/group picker pattern already in `toolbar_view.rs`.

**Rationale**: Reuses proven interaction pattern. No need for a modal dialog or a separate `Popover` state entity.

**Alternative considered**: A modal sheet. Rejected — `gpui-component` sheets require a `Root` entity update and are heavier than the task warrants.

### Pattern-match control: text input + field dropdown in the bulk-action bar

**Decision**: The pattern-match control is an `InputState` + a `Button::dropdown_menu` for field selection (Title / Publisher / System), rendered inline in the bulk-action bar. Matching is triggered by pressing Enter or clicking a "Select Matches" button.

**Rationale**: Keeps the control self-contained in the toolbar row without a separate popover state entity. The InputState lifecycle matches the toolbar render cycle.

**Alternative considered**: A dedicated pattern-match popover. Rejected — adds a popover entity and dismiss logic for what is a simple two-field form.

### Bulk actions clear selection and deactivate mode on dispatch

**Decision**: Every bulk action that dispatches work (Download, Remove Download, etc.) clears `selected_ids` and sets `selection_mode = false` after enqueuing.

**Rationale**: After dispatching, the selection is stale (items' states will change). Clearing gives a clean reset. Users can re-enter selection mode if they need to act again.

### Grouped list view excluded from selection

**Decision**: Checkboxes are only rendered in ungrouped list, thumbnail, and grid views. Grouped list items do not show checkboxes.

**Rationale**: The grouped list uses non-virtualized custom rows that don't share the same render path. Adding selection there doubles the scope and the grouped view is less common. The ungrouped list (DataTable) is the primary bulk-workflow surface.

## Risks / Trade-offs

- [Performance] `selected_ids: HashSet<Arc<str>>` lookup during render is O(1) per item but `Arc<str>` clone per row. For 1000+ items in thumb/grid view, this is negligible. → No mitigation needed.
- [UX] Select All selects only the _current page_ of results, not the full library. Users selecting across pages must paginate. → The count label clearly states "N selected" so users see the scope. Documented non-goal for now.
- [Service layer] Bulk Remove Download calls `reveal_in_file_manager` path internally; actual file deletion needs a new `LibraryService` method or direct `std::fs::remove_dir_all`. → Design calls for direct filesystem removal (same as existing reveal logic) rather than routing through the service layer, avoiding SDK dependency.

## Open Questions

- Should Select All span all pages or only the current page? Current design: current page only. If the user wants all, they can set page size to 200 and select all.
- Should Bulk Open open files sequentially or simultaneously? Current decision: simultaneously (one `open` syscall per item), consistent with how single-item reveal works.
