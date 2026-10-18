# verify-downloaded-status-against-disk Specification

## Purpose
TBD - created by archiving change verify-downloaded-status-against-disk. Update Purpose after archive.
## Requirements
### Requirement: A catalog load SHALL verify every file's downloaded status against disk
After a catalog load settles into its final displayed state — whether the
live fetch was skipped because the cache was fresh, a partial fetch merged
in updates, or a full fetch reconciled the entire catalog — the system SHALL
verify each item's files against the filesystem and set each file's
`downloaded` flag to whether a file actually exists at its expected on-disk
path, regardless of what the flag previously said.

#### Scenario: A file present on disk is marked downloaded even if the flag said otherwise
- **WHEN** a catalog load settles and an item's file exists at its expected
  on-disk path but that file's `downloaded` flag was `false`
- **THEN** after verification, the file's `downloaded` flag is `true` and
  the item's status is recomputed accordingly

#### Scenario: A file missing from disk is marked not-downloaded even if the flag said otherwise
- **WHEN** a catalog load settles and an item's file does not exist at its
  expected on-disk path but that file's `downloaded` flag was `true`
- **THEN** after verification, the file's `downloaded` flag is `false` and
  the item's status is recomputed accordingly

#### Scenario: Verification runs even when the live fetch is skipped
- **WHEN** the auto-load policy skips the live fetch because the on-disk
  catalog cache is fresh and the remote count matches
- **THEN** file-presence verification still runs against the cache's
  contents before the load is considered complete

### Requirement: Selecting an item SHALL verify that item's files against disk on demand
When the user selects a catalog item (the single-click popover path),
the system SHALL verify that specific item's files against disk in the
background, independent of and in addition to the existing per-item
availability check, so an external change to that item's files is reflected
without waiting for the next catalog load.

#### Scenario: Selecting an item with an externally deleted file updates its status
- **WHEN** the user selects an item whose file was deleted outside the app
  since the last verification
- **THEN** the item's status updates to reflect the file's absence without
  requiring a full catalog reload

### Requirement: Section counts SHALL stay synchronized after a verification pass
Completing a file-presence verification pass (catalog-wide or single-item)
that changes any item's status SHALL recompute the sidebar's smart-section
counts (`SectionCounts`) from the current catalog.

#### Scenario: On-device count reflects a verification-driven status change
- **WHEN** a verification pass changes an item's status between
  `Downloaded` and `Cloud`
- **THEN** the "On This Device" section count reflects the change
  immediately

### Requirement: Download-status UI SHALL show a distinct pending state while verification is in flight
Any button, label, or status glyph that displays an item's download status
(the catalog list/grid/thumbs status glyph, the item popover's download
button, and the detail tab's download button and status glyph) SHALL show
a distinct "pending verification" state, visually and via tooltip, for the
duration of a file-presence verification pass affecting that item — whether
the catalog-wide load-time pass or the on-demand single-item check. This
state is distinct from the existing network-bound availability-check
indicator, and from the Downloaded/Cloud states, so a user is never shown a
value that verification may be about to overturn.

#### Scenario: Catalog status glyph shows pending state during catalog-wide verification
- **WHEN** a catalog load's file-presence verification pass is in flight for
  an item
- **THEN** that item's status glyph in the list, grid, and thumbs views
  shows a pending indicator (not Downloaded or Cloud) with a tooltip
  distinct from the availability-check indicator

#### Scenario: Download button shows pending state during on-demand verification
- **WHEN** the user selects an item and its on-demand file-presence
  verification is in flight
- **THEN** the item popover's and detail tab's download buttons are
  disabled and show a loading/pending state with a "checking download
  status" tooltip, until verification completes

#### Scenario: Pending state clears once verification completes
- **WHEN** a file-presence verification pass for an item completes (whether
  or not it changed the item's `downloaded` flags)
- **THEN** the item's download-status UI reverts to showing its current
  Downloaded/Cloud state

### Requirement: File-presence verification SHALL also record whether a present file is offloaded

Whenever a file-presence verification pass (catalog-wide on load, or
on-demand on item selection) determines that a file exists at its resolved
on-disk path, it SHALL also determine and record whether that file is an
iCloud offloaded (dataless) placeholder rather than fully materialized
local data. A file that does not exist at its resolved path is not
offloaded (it is simply not downloaded).

#### Scenario: A present, fully materialized file is recorded as not offloaded
- **WHEN** a verification pass finds a file present at its resolved path and
  fully materialized locally
- **THEN** the file's offloaded flag is `false`

#### Scenario: A present but evicted file is recorded as offloaded
- **WHEN** a verification pass finds a file present at its resolved path but
  its data has been evicted to iCloud
- **THEN** the file's `downloaded` flag remains `true` and its offloaded
  flag is `true`

#### Scenario: A missing file is recorded as not offloaded
- **WHEN** a verification pass finds no file at its resolved path
- **THEN** the file's `downloaded` flag is `false` and its offloaded flag is
  `false`

### Requirement: A change in offloaded state SHALL be treated as a change requiring catalog update

The verification pass's "did anything change" result SHALL account for a
file's offloaded flag changing value, in addition to the existing
`downloaded` flag and item status, so that catalog updates and section-count
recomputation are triggered when a file transitions between offloaded and
fully materialized.

#### Scenario: Only the offloaded flag changing still triggers an update
- **WHEN** a verification pass finds a file's `downloaded` flag unchanged
  but its offloaded flag differs from the previously recorded value
- **THEN** the verification pass reports that a change occurred
