# Design

## Current state

- `dtrpg_core::LibraryApi` exists but opens the index per call and does not
  hold queue state. It is the right shape for stateless headless calls, not
  for the desktop UI's live session.
- `dtrpg_ui::controllers::library::LibraryController` owns:
  - `LibraryIndex index` — catalog storage,
  - `WorkQueue work` + `ThumbnailJobs`/`DownloadJobs`/`CheckJobs`,
  - filtered/grouped caches and sidebar counts,
  - plus a large amount of pure UI state (selection, popovers, inspector, etc.).

The goal is to separate the library *session* (core) from the library *view*
(UI).

## Target architecture

```text
+----------------------------+        +----------------------------+
| dtrpg-desktop / dtrpg-ui   |        | dtrpg-mcp / headless tests   |
|                            |        |                              |
| LibraryController (view)   |        | Library (core)               |
|   - selection              |<------>|   - LibraryIndex             |
|   - filter/sort prefs      |  query |   - WorkQueue                |
|   - sidebar open/closed    |        |   - caches                   |
|   - popover anchors        |        |                              |
+----------------------------+        +----------------------------+
```

`dtrpg_core::library::Library` is a long-lived, headless session object.
`LibraryController` holds an `Entity<Library>` or an `Arc<Library>` and
forwards view-state changes to it.

## Slices

### Slice 1: Extract core `Library` skeleton

Create `dtrpg_core::library::Library` with:
- `LibraryIndex`,
- `WorkQueue`,
- `cover_fetcher`,
- `network_monitor`.

Move construction from `LibraryController::build` into `Library::new`.
`LibraryController` keeps a `library: Arc<Library>` field but continues to
reach through it for everything else. No behavior change.

### Slice 2: Move catalog load/refresh into `Library`

Move `start_load`, `apply_load_result`, `apply_partial_fetch`,
`append_catalog_page`, and the catalog-sync state machine into `Library`.
`LibraryController` receives a `LibraryChanged`-like event and updates its
own view state. This slice is the biggest behavioral boundary; it is kept
separate so the async-load tests can be verified in core without GPUI.

### Slice 3: Move filtered/grouped queries and caches into `Library`

`Library` exposes:
- `visible_items(&self, view_state) -> Arc<Vec<LibraryItem>>`,
- `visible_items_grouped(&self, view_state) -> Arc<Vec<ItemGroup>>`,
- `section_counts()`, `publishers()`, `tags()`.

The UI controller stops holding `items_cache`/`grouped_items_cache` and
instead reads from `Library` on render. `invalidate_cache` becomes a core
method called when view-state changes.

### Slice 4: Move job orchestration into `Library`

Move `ThumbnailJobs`, `DownloadJobs`, `CheckJobs`, and the methods that
submit/cancel/finish jobs into `Library`. The UI controller only knows which
items are currently checking/downloading via query methods on `Library`.

### Slice 5: Move catalog mutations into `Library`

Move per-item status changes, tag edits, collection membership edits, and
disk-presence reconciliation into `Library`. `LibraryController` becomes a
pure view-model.

As built, `Library` owns every write to a catalog entry outside a sync run
(`apply_presence_verifications`, `apply_availability_result`,
`record_download_result`, `clear_item_downloads`, `clear_catalog`), the
scope of a tag edit (`items_affected_by_tag_edit`), and the pure
optimistic-membership rules (`add_member`, `remove_member` and their
rollbacks). The controller no longer writes to the index. The collections
list itself, its service, the tag-load loop, and the `verifying_downloads`
set stay on the controller and are tracked as a follow-up.

## Render-path invariant

No slice may add SQLite I/O or whole-catalog copies to the render path. The
core `Library` keeps the same caches the UI controller keeps today; render
paths read cached data only.

## Testing strategy

For each slice:
- `dtrpg-core` gains tests that drive `Library` directly (no GPUI).
- `dtrpg-ui` tests keep passing with the thinned controller.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo test
  --workspace`, `make fmt-check`, `make size-check` clean.

## Open questions

- Should `Library` be a `gpui::Global` or passed as `Arc<Library>` into the
  controller? `Arc` keeps it testable and headless-friendly.
- Should the UI controller receive events via a GPUI subscription, or poll
  a `Library` event channel each frame? The current `WorkQueue` already emits
  `JobEvent`s on a channel; the controller can keep its existing event pump.
