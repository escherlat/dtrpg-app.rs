## MODIFIED Requirements

### Requirement: Sidebar MUST provide four smart library section items
The sidebar MUST render a "Library" section group containing four nav items: All Titles, Recently Updated, On This Device, and In the Cloud. Each item MUST display a badge showing the count of library items matching that section's filter.

#### Scenario: All Titles shows the total library count
- **WHEN** the library has been loaded
- **THEN** the "All Titles" nav item badge shows the total number of library items

#### Scenario: Recently Updated shows items added or updated within the last 30 days
- **WHEN** the library has been loaded
- **THEN** the "Recently Updated" nav item badge shows the count of items whose `date_added` or `date_updated` (whichever is more recent) falls within the last 30 days of the current time

#### Scenario: On This Device shows downloaded item count
- **WHEN** the library has been loaded
- **THEN** the "On This Device" nav item badge shows the count of items whose status is downloaded

#### Scenario: In the Cloud shows cloud-only item count
- **WHEN** the library has been loaded
- **THEN** the "In the Cloud" nav item badge shows the count of items whose status is cloud-only

#### Scenario: Active section item is visually distinguished
- **WHEN** a sidebar nav item is the active filter
- **THEN** the nav item renders in the active/selected visual state and other nav items render in the default state
