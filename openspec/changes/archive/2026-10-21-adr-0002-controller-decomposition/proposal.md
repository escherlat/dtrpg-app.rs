# Proposal

## Why

Issue pilgrimagesoftware/dtrpg-app.rs#364. ADR 0002 inverted the crate graph
and moved domain models/persistence into `dtrpg-core`, but it explicitly left
three follow-ups for later commits:

1. Merge the three hand-rolled work queues into one bounded priority queue.
2. Share one `tokio::runtime::Runtime` across services.
3. Decompose `LibraryController` so catalog/queue state lives in core.

(1) and (2) are already done: `dtrpg_core::work::WorkQueue` is the single
queue, and `dtrpg_core::runtime::handle()` is the shared runtime. This change
addresses (3).

## What Changes

Introduce a headless `dtrpg_core::library::Library` type that owns:
- the `LibraryIndex` connection,
- the `WorkQueue` and the job trackers for downloads, checks, and thumbnails,
- the filtered/grouped result caches derived from the current view state.

`LibraryController` shrinks to GPUI-facing view state: selection, sidebar
filter/search/sort preferences, open/closed sections, popover anchors, detail
panel state, and so on. It queries `Library` for the visible result set,
counts, publishers, and tags, and it forwards user actions (download,
cancel, set tags, etc.) to `Library`.

Because the refactor touches most of `controllers/library/`, it is split into
small, independently reviewable slices. Each slice keeps the existing test
suite passing and changes no observable behavior.

## Outcomes

The MCP server and any future headless consumer can construct the same
`Library` the desktop UI uses, without GPUI. The UI controller becomes a
view-model, not the owner of library state.