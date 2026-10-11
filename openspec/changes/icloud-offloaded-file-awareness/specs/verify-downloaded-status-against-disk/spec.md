## ADDED Requirements

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
