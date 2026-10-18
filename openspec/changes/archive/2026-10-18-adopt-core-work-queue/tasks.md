## 1. Implementation (landed in pilgrimagesoftware/dtrpg-app.rs#318)

- [x] 1.1 Shared Tokio runtime in `dtrpg-core` (`runtime`)
- [x] 1.2 Core work queue with limit, priority, cancellation, progress and retry events, and a lane
      for availability checks (`work`)
- [x] 1.3 Thumbnail fetches on the queue; cover fetching behind `CoverTransport`
- [x] 1.4 File downloads on the queue, sharing one limit with thumbnails
- [x] 1.5 Availability checks on the queue
- [x] 1.6 Avatar fetching behind `AvatarTransport`
- [x] 1.7 Download progress: `DownloadControl` on `LibraryService::download_item`, byte progress from
      the transfer, progress bar in the activity panel

## 2. Specs and documentation

- [x] 2.1 Restate `rust-resource-work-queue-topology`, `thumbnail-queue-concurrency`,
      `thumbnail-loading-queue`, and `download-queue` against the work queue
- [x] 2.2 Add `availability-check-scheduling`
- [x] 2.3 Record the design in `docs/adr/0007-core-work-queue.md`
- [x] 2.4 Document the progress bar and the priority behavior in `docs/site/latest/downloads.md`
