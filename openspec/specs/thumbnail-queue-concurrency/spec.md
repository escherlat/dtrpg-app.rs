# thumbnail-queue-concurrency Specification

## Purpose
TBD - created by archiving change download-queue-concurrency-control. Update Purpose after archive.
## Requirements
### Requirement: Thumbnail loading MUST run concurrently up to the configured limit
The app MUST load thumbnails concurrently, dispatching up to `max_concurrent_downloads` parallel HTTP fetches at a time, rather than serially one at a time.

#### Scenario: Multiple thumbnails visible in grid
- **WHEN** the grid view shows more thumbnails than a single fetch can resolve before the next render
- **THEN** up to `max_concurrent_downloads` thumbnail fetches run in parallel, reducing total time to fill the visible grid

#### Scenario: Slot freed after thumbnail fetch
- **WHEN** a thumbnail fetch completes (success or error)
- **THEN** the next pending thumbnail in the queue is dispatched immediately without waiting for other slots

### Requirement: Thumbnail and download concurrency MUST share the same limit
The number of concurrent thumbnail fetches plus concurrent file downloads MUST NOT exceed `max_concurrent_downloads` in aggregate, so that thumbnail loading and active downloads do not collectively saturate the connection. Availability checks do not count toward this limit.

#### Scenario: Downloading while thumbnails are loading
- **WHEN** a file download is in progress and grid thumbnails are being fetched
- **THEN** the combined number of active fetches stays at or below `max_concurrent_downloads`

#### Scenario: A check is not counted
- **WHEN** `max_concurrent_downloads` thumbnail fetches and downloads are running and an availability check is requested
- **THEN** the check runs without waiting for one of them to finish, subject to the rule that checks run one at a time

### Requirement: Priority thumbnails MUST preempt lower-priority ones
When the user asks for a thumbnail — "Load Thumbnail" on an entry, opening an entry's detail view, or refreshing a detail cover — the app MUST schedule that fetch ahead of any background thumbnail loading still waiting. It MUST NOT interrupt a fetch that is already running.

#### Scenario: Promoting a thumbnail on user request
- **WHEN** the user asks for an entry's thumbnail while background thumbnail fetches are waiting
- **THEN** that fetch is dispatched as soon as a slot is available, ahead of all the waiting background fetches

#### Scenario: Running fetches are not interrupted
- **WHEN** the user asks for a thumbnail while the limit is reached
- **THEN** the fetches already running continue, and the requested one starts when one of them finishes

#### Scenario: Requests are served in the order made
- **WHEN** the user asks for several thumbnails one after another while all slots are busy
- **THEN** they are dispatched in the order they were requested
