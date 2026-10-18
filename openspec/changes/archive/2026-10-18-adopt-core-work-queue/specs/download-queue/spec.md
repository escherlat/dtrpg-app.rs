## ADDED Requirements

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
