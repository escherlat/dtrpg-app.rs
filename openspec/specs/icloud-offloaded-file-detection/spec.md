# icloud-offloaded-file-detection Specification

## Purpose
TBD - created by archiving change icloud-offloaded-file-awareness. Update Purpose after archive.
## Requirements
### Requirement: The system SHALL classify an existing file's iCloud materialization state

For a file path that exists on disk, on macOS or Windows, the system SHALL determine
whether the file is fully materialized locally or is an iCloud offloaded
(dataless) placeholder, without altering the file's contents or triggering
its download.

#### Scenario: A fully local file is classified as not offloaded
- **WHEN** the system checks materialization state for a file that is not
  part of an iCloud-synced location, or is a fully downloaded iCloud file
- **THEN** the system reports the file as not offloaded

#### Scenario: An evicted iCloud file is classified as offloaded
- **WHEN** the system checks materialization state for a file whose data has
  been evicted by macOS to free disk space, leaving a dataless placeholder
  at the same path
- **THEN** the system reports the file as offloaded

### Requirement: The system SHALL treat materialization state as unavailable on platforms that do not report it

The platform integration layer reports cloud placeholder state on macOS and Windows. On any other
platform, the system SHALL report every existing file as not offloaded, without error.

#### Scenario: Windows placeholder is classified as offloaded
- **WHEN** the system checks materialization state for a Files On-Demand placeholder on Windows
- **THEN** the system reports the file as offloaded

#### Scenario: A platform without cloud placeholder state reports not offloaded
- **WHEN** the system checks materialization state for an existing file on Linux
- **THEN** the system reports the file as not offloaded
