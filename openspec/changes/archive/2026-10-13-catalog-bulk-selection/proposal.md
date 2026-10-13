## Why

The catalog currently supports single-item interactions only. Users with large libraries need to act on groups of items at once — downloading a whole publisher's catalogue, removing locally cached files to free space, or batch-assigning titles to a collection. Without selection, each of these tasks requires tedious one-at-a-time repetition.

## What Changes

- Introduce a selection mode that can be toggled on/off from the toolbar; when inactive, the catalog behaves exactly as it does today (no visual clutter).
- Add a selection set to `LibraryController` that tracks which item IDs are currently selected.
- Expose Select All, Deselect All, and pattern-match selection (by title, publisher, or system) as controller actions.
- Add a bulk-actions bar that appears only when selection mode is active and at least one item is selected; it surfaces: Download, Remove Download, Fetch Thumbnail, Add to Collection, Remove from Collection, and Open.
- Collection membership actions require a collection picker popover.
- Add per-item selection affordance (checkbox) in list, thumb, and grid views.

## Capabilities

### New Capabilities

- `catalog-selection`: Selection mode toggle, selection set management (select/deselect individual items, Select All, Deselect All, pattern-match select by title/publisher/system), and per-item checkbox rendering in all three catalog views.
- `catalog-bulk-actions`: Bulk-action bar rendered when selection is non-empty; actions: Download, Remove Download, Fetch Thumbnail, Add to Collection (with picker), Remove from Collection (with picker), Open. Each action operates on the full selection set, shows progress via the activity panel, and clears selection on completion.

### Modified Capabilities

- `rust-library-ui-implementation`: Toolbar gains a selection mode toggle; catalog views gain per-item checkboxes; toolbar/catalog layout adapts when selection mode is active.

## Impact

- `LibraryController`: new `selection_mode: bool`, `selected_ids: HashSet<Arc<str>>`, and methods for toggling mode, selecting/deselecting, Select All, Deselect All, pattern-match select, and bulk-action dispatch.
- `LibrarySnapshot`: propagates `selection_mode`, `selected_ids`, and `selection_count`.
- `catalog_view.rs`: per-item checkbox overlay in all three views; consumes `LibrarySnapshot` fields.
- `toolbar_view.rs`: selection mode toggle button; bulk-action bar when mode is active.
- `data/events.rs`: `SelectionChanged` event on `LibraryController`.
- No new dependencies required; bulk actions route through the existing activity + library service layer.
