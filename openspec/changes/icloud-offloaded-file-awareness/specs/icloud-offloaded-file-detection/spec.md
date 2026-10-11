## Purpose

Lets the app tell, on macOS, whether a file that exists on disk is fully
present locally or is an iCloud "dataless" placeholder whose bytes have been
evicted to save disk space and would trigger a network download on open.

## ADDED Requirements

### Requirement: The system SHALL classify an existing file's iCloud materialization state

For a file path that exists on disk, on macOS, the system SHALL determine
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

#### Scenario: A file mid-download is not classified as offloaded
- **WHEN** the system checks materialization state for a file that is
  currently being fetched from iCloud (downloading but not yet complete)
- **THEN** the system reports the file as not offloaded

### Requirement: The system SHALL treat iCloud materialization state as unavailable on non-macOS platforms

On platforms other than macOS, the system SHALL report every existing file
as not offloaded, without error, since offloading is a macOS/iCloud-specific
filesystem behavior.

#### Scenario: Non-macOS platform always reports not offloaded
- **WHEN** the system checks materialization state for an existing file on a
  non-macOS platform
- **THEN** the system reports the file as not offloaded
