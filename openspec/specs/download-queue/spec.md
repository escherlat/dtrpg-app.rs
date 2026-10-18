# download-queue Specification

## Purpose
TBD - created by archiving change download-queue-concurrency-control. Update Purpose after archive.
## Requirements
### Requirement: Downloads MUST be dispatched through a bounded queue

The app MUST maintain a download queue that dispatches at most `max_concurrent_downloads`
concurrent file downloads at any time. Requests that exceed the limit MUST wait in the queue
until a slot opens. The queue's unit of work is a single file within a catalog entry, identified
by `(entry_id, file_index)` — a multi-item entry MAY have more than one of its files queued or
active at once, each tracked and cancellable independently.

#### Scenario: Enqueueing a download within capacity

- **WHEN** the user requests a file download and fewer than `max_concurrent_downloads` downloads
  are active
- **THEN** the download starts immediately and an activity panel entry appears identifying the
  entry title and the specific file

#### Scenario: Enqueueing a download at capacity

- **WHEN** the user requests a file download and `max_concurrent_downloads` slots are all
  occupied
- **THEN** the request is added to the queue and begins once a slot frees

#### Scenario: Enqueueing multiple items from the same entry

- **WHEN** the user requests downloads for two different files belonging to the same multi-item
  entry
- **THEN** both are tracked as independent queue entries, each with its own activity panel entry,
  and either MAY start, complete, or be cancelled without affecting the other

#### Scenario: Download completes successfully

- **WHEN** a file download finishes without error
- **THEN** that file's downloaded status updates, the entry's overall status updates to
  Downloaded only once every one of its files has completed, and the file's activity panel entry
  moves to the recent/complete state

#### Scenario: Download fails

- **WHEN** a file download encounters a network or storage error
- **THEN** that file's downloaded status remains unset, the entry's overall status remains (or
  reverts to) Cloud, the file's activity panel entry shows an error message, and the slot is
  freed for the next queued item

### Requirement: Each download MUST have a named activity panel entry
The app MUST create one activity panel entry per enqueued download, identified by the item's title, and MUST update that entry when the download completes or fails.

#### Scenario: Activity entry lifecycle
- **WHEN** a download is enqueued
- **THEN** an activity entry is started with the item title as its label; it transitions to complete or error when the download finishes

### Requirement: Users MUST be able to cancel a queued or in-progress download

The app MUST allow a user to cancel a specific file's download that has not yet completed,
removing it from the queue or aborting the in-flight fetch and freeing its concurrency slot,
without affecting any other file download queued or active for the same entry.

#### Scenario: Cancelling a queued download

- **WHEN** the user cancels a file download that is waiting in the queue
- **THEN** it is removed from the queue and no activity entry is created for that file

#### Scenario: Cancelling an in-progress download

- **WHEN** the user cancels a file download that is actively fetching
- **THEN** the fetch is aborted, any partial file is deleted, the slot is freed, and that file's
  activity entry is removed

#### Scenario: Cancelling one item leaves sibling items unaffected

- **WHEN** the user cancels one file's download while a different file from the same entry is
  still queued or downloading
- **THEN** the cancelled file's queue entry and activity entry are removed, and the other file's
  queue position, in-flight fetch, and activity entry are unaffected

### Requirement: The entry-level download action MUST enqueue every not-yet-downloaded item

Triggering the entry-level download action on a multi-item entry MUST enqueue a separate download
for every file in that entry whose downloaded status is not already set, and MUST NOT re-enqueue
files that are already downloaded or already queued/active.

#### Scenario: Downloading a multi-item entry with nothing downloaded yet

- **WHEN** the user triggers the entry-level download action on a multi-item entry with none of
  its files downloaded
- **THEN** every file in the entry is enqueued as a separate download

#### Scenario: Downloading a multi-item entry with some items already present

- **WHEN** the user triggers the entry-level download action on a multi-item entry where one file
  is already downloaded and the rest are not
- **THEN** only the not-yet-downloaded files are enqueued; the already-downloaded file is left
  untouched

#### Scenario: Triggering the entry-level action while some items are already queued

- **WHEN** the user triggers the entry-level download action on an entry that already has one of
  its files queued or actively downloading
- **THEN** that file is not enqueued a second time; only files not yet downloaded and not already
  queued/active are enqueued

### Requirement: User-initiated downloads MUST run ahead of background work
A download the user asks for MUST be dispatched before any background work (such as automatic thumbnail loading) still waiting in the same queue. It MUST NOT interrupt work that is already running.

#### Scenario: A download is requested while thumbnails are waiting
- **WHEN** the user requests a download while background thumbnail fetches are waiting and all concurrency slots are occupied
- **THEN** the download starts as soon as a slot frees, ahead of all the waiting thumbnail fetches

#### Scenario: Running work is not interrupted
- **WHEN** the user requests a download while all concurrency slots are occupied
- **THEN** the fetches already running continue until they finish

### Requirement: A running download MUST show its progress
A download's activity panel entry MUST show how much of the file has arrived, as a progress bar, for as long as the download is running. When the server does not say how large the file is, the entry MUST keep its indeterminate bar rather than guess.

#### Scenario: The size is known
- **WHEN** a download is running and the server reported the file's size
- **THEN** the activity entry's progress bar shows the fraction of the file received so far, never more than full

#### Scenario: The size is unknown
- **WHEN** a download is running and the server did not report the file's size
- **THEN** the activity entry keeps its indeterminate bar

#### Scenario: A retry starts again
- **WHEN** a download fails in a retryable way and is retried
- **THEN** the activity entry's label says it is retrying, and the progress bar follows the new attempt from the start
