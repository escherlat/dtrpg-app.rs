# rust-library-ui-implementation Specification

## Purpose
TBD - created by archiving change implement-rust-library-baseline-ui-stubs. Update Purpose after archive.
## Requirements
### Requirement: Rust frontend baseline implementation MUST satisfy shared desktop library layout behavior
The Rust frontend MUST implement the shared desktop library baseline layout and interaction behavior defined by the app meta-repository.

#### Scenario: Rendering baseline library layout in Rust
- **WHEN** the Rust app renders the library screen in baseline mode
- **THEN** it presents the shared top-level layout regions and interactions defined by shared app specs

### Requirement: Rust baseline implementation MUST use stubbed backend adapters
The Rust frontend baseline implementation MUST keep backend communication stubbed while exercising list/detail/filter/refresh flows.

#### Scenario: Loading library data in Rust baseline mode
- **WHEN** the Rust frontend loads or refreshes library content in baseline phase
- **THEN** it uses stubbed adapters and no live backend SDK calls

### Requirement: Catalog load activity entry SHALL display a progress bar when total is known
When the estimated total item count is available, the catalog load activity entry SHALL carry a numeric progress value so the activity panel can render a progress bar rather than an indeterminate spinner.

#### Scenario: Progress bar visible during catalog load with known total
- **WHEN** the catalog load activity entry has a non-None progress value
- **THEN** the activity panel renders a progress bar for that entry reflecting the current fraction complete

#### Scenario: Spinner visible during catalog load with unknown total
- **WHEN** the catalog load activity entry has a None progress value
- **THEN** the activity panel renders an indeterminate spinner for that entry (existing behavior, no change)

### Requirement: Detail tab layout places the thumbnail left of item information

The expanded detail tab SHALL render the cover thumbnail in a fixed-width left column,
inset from the tab's left edge, and the item's publisher, title, description, actions,
and metadata in an independently scrolling right column.

#### Scenario: Detail tab layout

- **WHEN** a detail tab is open
- **THEN** the cover renders in a fixed-width left column with left padding separating it
  from the tab's edge, and item information renders in a scrollable right column

#### Scenario: Scrolling long content

- **WHEN** the user scrolls a long description or metadata list in the right column
- **THEN** the cover in the left column remains fixed in place

### Requirement: Sidebar renders dividers between section groups
The sidebar SHALL display a thin horizontal divider between the smart-filter group and the Publishers group, and a second divider between the Publishers group and the Collections group.

#### Scenario: Divider appears between smart filters and publishers
- **WHEN** the sidebar is rendered and both the smart-filter items and the Publishers section are visible
- **THEN** a thin horizontal rule is visible between the last smart-filter item and the Publishers section header

#### Scenario: Divider appears between publishers and collections
- **WHEN** the sidebar is rendered and both the Publishers section and the Collections section are visible
- **THEN** a thin horizontal rule is visible between the Publishers section and the Collections section header

### Requirement: Toolbar exposes a selection mode toggle button
The Rust UI toolbar SHALL include a button that toggles catalog selection mode. The button SHALL reflect the current selection mode state (active vs. inactive) via its visual variant.

#### Scenario: Toggle button shown in toolbar
- **WHEN** the library screen is rendered
- **THEN** a selection mode toggle button is present in the toolbar alongside the existing layout and settings controls

#### Scenario: Toggle button reflects active state
- **WHEN** selection mode is active
- **THEN** the toggle button is rendered in its active/selected visual state

#### Scenario: Toggle button reflects inactive state
- **WHEN** selection mode is inactive
- **THEN** the toggle button is rendered in its default/ghost visual state

### Requirement: Toolbar shows a bulk-action bar whenever selection mode is active
Whenever selection mode is active, the Rust UI toolbar area SHALL display a bulk-action bar below the normal toolbar row, regardless of selection count. The bar SHALL contain: a selection count label, Select All, Deselect All, a pattern-match selection control, and the six bulk-action buttons (Download, Remove Download, Fetch Thumbnail, Add to Collection, Remove from Collection, Open).

#### Scenario: Bulk-action bar appears as soon as selection mode is entered
- **WHEN** selection mode is toggled on
- **THEN** the bulk-action bar is visible immediately, with all six action buttons present, even before any item is selected

#### Scenario: Bulk-action bar hidden outside selection mode
- **WHEN** selection mode is off
- **THEN** the bulk-action bar is not rendered

### Requirement: Pattern-match selection control is accessible from the bulk-action bar
The bar SHALL include a pattern-match control (text input + field selector) that allows users to add items to the selection by matching on title, publisher, or system.

#### Scenario: Pattern-match control is present in bar
- **WHEN** the bulk-action bar is visible
- **THEN** a text input and a field-selector dropdown are present for pattern-match selection

#### Scenario: Invoking pattern-match from bar adds matching items
- **WHEN** the user enters text and selects a field in the pattern-match control
- **THEN** items matching the pattern in the chosen field are added to the selection set

