## Why

Thumbnail fetches, file downloads, and availability checks used to be scheduled by three hand-rolled
queues inside `LibraryController`, a GPUI entity. That kept scheduling out of reach of anything
headless (the MCP server), left user-initiated downloads waiting behind thumbnail backlogs, and gave
each queue its own slot bookkeeping. They now run on one work queue in `dtrpg-core`, designed in
`docs/adr/0007-core-work-queue.md` and specified for the whole project by the umbrella capability
`libri-work-queue` (`dtrpg/openspec/changes/libri-v2-architecture`).

This change restates the app's own specs against that queue, so they describe the behavior the code
has instead of the mechanism it replaced. The implementation landed in pull request
pilgrimagesoftware/dtrpg-app.rs#318 (stage 3 of `libri-v2-architecture`).

## What Changes

- Scheduling of the three kinds of background work moves to the core work queue. Specs that named
  `download_queue`, "the front of the queue", or gpui's background executor for this work are
  restated.
- **BREAKING (behavior):** a download the user starts runs ahead of thumbnails still waiting. Before,
  it queued behind all of them.
- Thumbnail activity progress counts every fetch not yet finished, and the activity entry completes
  when the last one finishes, not when the queue first empties.
- A running download shows a progress bar (new). Before, only a label and retry text.
- Availability checks get their own lane: one at a time, outside the shared concurrency limit, with
  user-requested checks ahead of automatic ones. No spec covered this before.
- The stale "Sequential single-item processing" requirement for thumbnails is removed: thumbnails have
  loaded concurrently since `download-queue-concurrency-control`.

## Capabilities

### New Capabilities

- `availability-check-scheduling`: How availability checks are scheduled, prioritized, and shown.

### Modified Capabilities

- `rust-resource-work-queue-topology`: Queued work runs on the core work queue and the shared
  runtime, not gpui's executor; catalog sync keeps its own serial path.
- `thumbnail-queue-concurrency`: Availability checks are excluded from the shared limit; user-requested
  thumbnails run ahead of background loading rather than "preempting".
- `thumbnail-loading-queue`: Concurrent fetching is already specified elsewhere; activity progress and
  the manual trigger are restated.
- `download-queue`: User-initiated downloads run ahead of background work; running downloads show
  progress.

## Impact

- `dtrpg-core`: `work` (the queue), `runtime`, `services::covers`, `services::avatar`; the
  `LibraryService::download_item` signature takes a `DownloadControl`.
- `dtrpg-ui`: `controllers/library` submits jobs and applies the queue's events; the three old queues
  and their slot counters are gone.
- `docs/site/latest/downloads.md` documents the progress bar and the priority behavior.
