# catalog-bulk-actions Specification

## Purpose
Bulk actions (Download, Remove Download, Fetch Thumbnail, Add/Remove Collection, Open) dispatched over the current catalog selection set, with a confirmation gate for large selections.

## Requirements

### Requirement: Bulk-action bar appears as soon as selection mode is active
The system SHALL render a bulk-action bar whenever selection mode is active, regardless of selection count. Gating the bar on a non-empty selection was tried and rejected during review — users expect it immediately on entering selection mode, not only after picking an item.

#### Scenario: Bar appears immediately on entering selection mode
- **WHEN** the user toggles selection mode on
- **THEN** the bulk-action bar becomes visible, showing a count of zero

#### Scenario: Bar disappears when selection mode is turned off
- **WHEN** selection mode is deactivated
- **THEN** the bulk-action bar is hidden

### Requirement: Bulk actions on a large selection require confirmation
The system SHALL ask for confirmation before dispatching a bulk action whose selection count exceeds a threshold (20 items), naming the action and the count. A selection at or below the threshold dispatches immediately.

#### Scenario: Small selection dispatches without confirmation
- **WHEN** the user invokes a bulk action with a selection of 20 items or fewer
- **THEN** the action runs immediately with no confirmation prompt

#### Scenario: Large selection requires confirmation
- **WHEN** the user invokes a bulk action with a selection larger than 20 items
- **THEN** a confirmation dialog appears naming the action and the selection count before the action runs

#### Scenario: Declining confirmation cancels the action
- **WHEN** the user dismisses or cancels the confirmation dialog
- **THEN** the bulk action does not run and the selection is preserved

### Requirement: Bulk Download queues selected items for download
The system SHALL provide a Bulk Download action that enqueues each selected item that is not already downloaded.

#### Scenario: Bulk Download enqueues undownloaded items
- **WHEN** the user invokes Bulk Download with items selected
- **THEN** each selected item that is not yet locally present is queued for download via the activity system

#### Scenario: Bulk Download skips already-downloaded items
- **WHEN** a selected item is already present locally
- **THEN** that item is not re-enqueued

#### Scenario: Bulk Download clears selection on dispatch
- **WHEN** the Bulk Download action is dispatched
- **THEN** the selection set is cleared and selection mode is deactivated

### Requirement: Bulk Remove Download reverts selected items to cloud-only status
The system SHALL provide a Bulk Remove Download action that reverts each selected downloaded item to `Cloud` status, reusing the same per-item removal path as the single-item "Remove Download" action.

#### Scenario: Bulk Remove Download reverts downloaded items
- **WHEN** the user invokes Bulk Remove Download with downloaded items selected
- **THEN** each selected item's files are marked not-downloaded and its status becomes `Cloud`

#### Scenario: Bulk Remove Download skips cloud-only items
- **WHEN** a selected item has no local download
- **THEN** that item is skipped silently

#### Scenario: Bulk Remove Download clears selection on dispatch
- **WHEN** the Bulk Remove Download action is dispatched
- **THEN** the selection set is cleared and selection mode is deactivated

### Requirement: Bulk Fetch Thumbnail queues thumbnail retrieval for selected items
The system SHALL provide a Bulk Fetch Thumbnail action that re-triggers thumbnail retrieval for each selected item.

#### Scenario: Bulk Fetch Thumbnail queues selected items
- **WHEN** the user invokes Bulk Fetch Thumbnail with items selected
- **THEN** each selected item is queued for thumbnail fetch

#### Scenario: Bulk Fetch Thumbnail clears selection on dispatch
- **WHEN** the Bulk Fetch Thumbnail action is dispatched
- **THEN** the selection set is cleared and selection mode is deactivated

### Requirement: Bulk Add to Collection assigns selected items to a chosen collection
The system SHALL provide a Bulk Add to Collection action. When invoked, a collection picker popover lists all available collections; the user selects one and the selected catalog items are assigned to that collection.

#### Scenario: Collection picker opens on action invocation
- **WHEN** the user invokes Bulk Add to Collection
- **THEN** a collection picker popover appears listing all available collections

#### Scenario: Items are added to the chosen collection
- **WHEN** the user selects a collection in the picker
- **THEN** each selected item is added to that collection and the picker closes

#### Scenario: Picker dismissal cancels the action
- **WHEN** the user dismisses the picker without selecting a collection
- **THEN** no collection assignment is made and the selection is preserved

#### Scenario: Bulk Add to Collection clears selection on dispatch
- **WHEN** a collection is chosen and the action is dispatched
- **THEN** the selection set is cleared and selection mode is deactivated

### Requirement: Bulk Remove from Collection removes selected items from a chosen collection
The system SHALL provide a Bulk Remove from Collection action. When invoked, a collection picker lists collections that contain at least one selected item; choosing a collection removes the selected items from it.

#### Scenario: Collection picker shows only relevant collections
- **WHEN** the user invokes Bulk Remove from Collection
- **THEN** the picker lists only collections that contain at least one currently selected item

#### Scenario: Items are removed from the chosen collection
- **WHEN** the user selects a collection in the picker
- **THEN** each selected item that belongs to that collection is removed from it

#### Scenario: Bulk Remove from Collection clears selection on dispatch
- **WHEN** a collection is chosen and the action is dispatched
- **THEN** the selection set is cleared and selection mode is deactivated

### Requirement: Bulk Open opens each selected item with its default application
The system SHALL provide a Bulk Open action that opens each selected downloaded item's file with the platform default application. Items without a local download, and multi-file entries requiring explicit file selection, are skipped.

#### Scenario: Bulk Open opens downloaded items
- **WHEN** the user invokes Bulk Open with downloaded items selected
- **THEN** each downloaded item's file is opened with its default application

#### Scenario: Bulk Open skips cloud-only items
- **WHEN** a selected item has no local file
- **THEN** that item is skipped silently

#### Scenario: Bulk Open clears selection on dispatch
- **WHEN** the Bulk Open action is dispatched
- **THEN** the selection set is cleared and selection mode is deactivated
