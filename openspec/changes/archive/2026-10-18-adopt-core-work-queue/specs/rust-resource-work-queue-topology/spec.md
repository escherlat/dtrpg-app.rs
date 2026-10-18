## MODIFIED Requirements

### Requirement: Content downloads and catalog sync execute independently
A content download in progress SHALL NOT prevent a catalog synchronization task from running
concurrently, and the work queue's concurrency settings SHALL remain independent of the
catalog-sync dispatch path.

#### Scenario: Catalog sync proceeds while a download is in progress
- **WHEN** a content download is running on the work queue
- **THEN** a catalog synchronization task can still execute on its own serial dispatch path

## REMOVED Requirements

### Requirement: Background work runs on gpui's background executor, never the UI thread
**Reason**: Thumbnail fetches, file downloads, and availability checks no longer run on gpui's
background executor; they run on the core work queue (see the new requirement below). Catalog sync
and cache tasks still do, and keep a requirement of their own.
**Migration**: The two requirements added below replace it. The UI thread stays free in both cases.

## ADDED Requirements

### Requirement: Catalog-sync and cache tasks run on gpui's background executor, never the UI thread
Catalog-sync and cache dispatch SHALL run through gpui's `background_executor()`/`cx.spawn`, never
blocking the UI thread.

#### Scenario: Background task executes while the UI remains responsive
- **WHEN** a catalog-sync or cache task is dispatched
- **THEN** it runs on gpui's background executor and the UI thread remains responsive

### Requirement: Thumbnail fetches, downloads, and availability checks run on the core work queue
Thumbnail fetches, file downloads, and availability checks SHALL be scheduled by the work queue in
`dtrpg-core` (the `libri-work-queue` capability) and SHALL run on threads of its shared runtime,
never on the UI thread. The queue SHALL report each job's start, progress, retries, and end as
events, which the library controller applies on the UI thread.

#### Scenario: Queued work runs while the UI remains responsive
- **WHEN** a thumbnail fetch, a download, or an availability check is running
- **THEN** it runs off the UI thread and the UI thread remains responsive

#### Scenario: Results reach the UI as events
- **WHEN** a queued job finishes
- **THEN** the library controller applies its result on the UI thread in response to the queue's
  finish event, and the job's slot is free for the next one

#### Scenario: The queue does not depend on the UI
- **WHEN** jobs are submitted in a process with no window
- **THEN** they are scheduled and run to completion
