## ADDED Requirements

### Requirement: Catalog supports a toggleable selection mode
The system SHALL provide a selection mode that is off by default. When off, catalog interaction is unchanged. When on, the catalog enters a multi-select state.

#### Scenario: Selection mode starts inactive
- **WHEN** the application launches
- **THEN** selection mode is off and no checkboxes are visible

#### Scenario: User activates selection mode
- **WHEN** the user clicks the selection mode toggle in the toolbar
- **THEN** selection mode becomes active and per-item checkboxes appear in all catalog views

#### Scenario: User deactivates selection mode
- **WHEN** selection mode is active and the user clicks the toggle again
- **THEN** selection mode is deactivated, all selections are cleared, and checkboxes disappear

### Requirement: Each catalog item renders a selection checkbox in selection mode
In selection mode, every item in list, thumbnail, and grid views SHALL display a checkbox affordance. Outside selection mode, no checkbox is rendered.

#### Scenario: Checkbox visible in list view during selection mode
- **WHEN** selection mode is active and the catalog is in list presentation
- **THEN** each row shows a checkbox at the leading edge

#### Scenario: Checkbox visible in thumb view during selection mode
- **WHEN** selection mode is active and the catalog is in thumbnail presentation
- **THEN** each thumbnail card shows a checkbox overlay

#### Scenario: Checkbox visible in grid view during selection mode
- **WHEN** selection mode is active and the catalog is in grid presentation
- **THEN** each grid card shows a checkbox overlay

#### Scenario: Item is selected via checkbox
- **WHEN** the user clicks a checkbox for an unselected item
- **THEN** that item's ID is added to the selection set and the checkbox appears checked

#### Scenario: Item is deselected via checkbox
- **WHEN** the user clicks a checkbox for a selected item
- **THEN** that item's ID is removed from the selection set and the checkbox appears unchecked

### Requirement: Select All adds every visible item to the selection set
The system SHALL provide a Select All action that selects every item currently visible in the catalog (respecting active filter, search, and pagination).

#### Scenario: Select All selects all visible items
- **WHEN** the user invokes Select All
- **THEN** all item IDs in the current visible result set are added to the selection set

#### Scenario: Select All is a no-op when already fully selected
- **WHEN** Select All is invoked and every visible item is already selected
- **THEN** the selection set is unchanged

### Requirement: Deselect All clears the selection set
The system SHALL provide a Deselect All action that removes all items from the selection set.

#### Scenario: Deselect All clears the selection
- **WHEN** the user invokes Deselect All with a non-empty selection set
- **THEN** the selection set becomes empty

#### Scenario: Deselect All is a no-op on empty selection
- **WHEN** Deselect All is invoked with no items selected
- **THEN** the selection set remains empty

### Requirement: Pattern-match selection adds items matching a predicate
The system SHALL provide a pattern-match selection mechanism that adds all items whose title, publisher, or system/game-line field contains a user-supplied string (case-insensitive).

#### Scenario: Pattern match selects matching items by title
- **WHEN** the user enters a pattern and selects "Title" as the field
- **THEN** all visible items whose title contains the pattern (case-insensitive) are added to the selection set

#### Scenario: Pattern match selects matching items by publisher
- **WHEN** the user enters a pattern and selects "Publisher" as the field
- **THEN** all visible items whose publisher contains the pattern are added to the selection set

#### Scenario: Pattern match selects matching items by system
- **WHEN** the user enters a pattern and selects "System" as the field
- **THEN** all visible items whose game system/line contains the pattern are added to the selection set

#### Scenario: Pattern match on empty input is a no-op
- **WHEN** the user invokes pattern-match selection with an empty string
- **THEN** the selection set is unchanged

### Requirement: Selection count is reflected in the UI
The system SHALL display the number of currently selected items whenever selection mode is active.

#### Scenario: Selection count updates on change
- **WHEN** the selection set changes (item added, removed, Select All, Deselect All, or pattern match)
- **THEN** the displayed count updates to reflect the new selection size

#### Scenario: Selection count shows zero when empty
- **WHEN** selection mode is active and no items are selected
- **THEN** the count display shows "0 selected" or equivalent
