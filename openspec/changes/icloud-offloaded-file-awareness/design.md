## Context

See proposal.md - Why. Two implementation facts drive this design:

- On current macOS (Sonoma+), iCloud eviction no longer creates a sibling
  `.<name>.icloud` marker file (that mechanism was removed with the switch
  to FileProvider "dataless files" in Sonoma). The evicted file keeps its
  original name and path but has no local data; reads trigger an on-demand
  download. Any marker-file-based detection would silently stop working on
  the macOS versions this app actually targets.
- The correct, current API for this is the Foundation resource key
  `NSURLUbiquitousItemDownloadingStatusKey`, queried via a file URL's
  resource values. The workspace already depends on `objc2` and
  `objc2-foundation` (`crates/dtrpg-ui/Cargo.toml`), so no new dependency is
  needed.

## Goals / Non-Goals

**Goals:**
- Correctly detect offloaded (dataless) files on current macOS via the
  supported Foundation API.
- Keep the change additive to the existing `downloaded`/`ItemStatus` model
  rather than introducing a new status value that ripples through every
  `ItemStatus` match site in `catalog_view.rs`.
- Fail safe: if the resource-value query errors or the key is unavailable
  (e.g. the file isn't in any ubiquitous container), treat the file as not
  offloaded rather than surfacing an error state.

**Non-Goals:**
- Triggering or cancelling iCloud downloads/evictions from within the app.
  This change is read-only status detection.
- Supporting the legacy pre-Sonoma `.icloud` marker-file convention. If a
  need for older-macOS compatibility surfaces later, it can be added as a
  fallback check without changing this design's data model.
- Detecting offloaded state for storage roots outside of any iCloud
  container — the resource key simply returns "not applicable," which this
  design treats identically to "not offloaded."

## Decisions

**Detection mechanism: `NSURLUbiquitousItemDownloadingStatusKey` via
`objc2-foundation`, not a marker-file heuristic.**
Rationale: it's Apple's supported, current API; it works uniformly whether
or not the path is inside a ubiquitous container (returns `nil`/not-present
for non-iCloud paths, which this design maps to "not offloaded"); and it
avoids depending on filesystem naming conventions Apple has already changed
once. Alternative considered: check the `SF_DATALESS` `st_flags` bit via
`stat(2)` (no Foundation dependency, avoids the objc bridge entirely).
Rejected as the primary mechanism because it's an undocumented/unstable flag
name to depend on directly, but it's worth keeping in mind as a lightweight
fallback if the resource-value path proves unreliable in testing — not
adopted now to avoid speculative complexity (YAGNI).

**Data model: add `offloaded: bool` to `LibraryItemFile`; no new
`ItemStatus` variant.**
Rationale: `ItemStatus::Downloaded` already means "the user has this file
downloaded" — offloaded is a refinement of *how* it's stored, not a
different ownership state, and matches the proposal's framing ("reflect as
downloaded but offloaded"). Introducing a third `ItemStatus` variant would
require updating every match site enumerated during exploration
(`catalog_view.rs` alone has ~14), touching sidebar filtering
(`matching.rs`), section counts, and context-menu logic that don't need to
change. A derived per-item helper (e.g. `LibraryItem::is_offloaded(&self) ->
bool`, true when `status == Downloaded` and any file's `offloaded` is
`true`) gives the UI what it needs without expanding the enum.

**Detection point: extend `verify_item_downloads` in
`file_presence.rs`, not a separate pass.**
Rationale: offloaded state is only meaningful for a file that exists
(`downloaded == true`); the existing verification pass already resolves
each file's path and checks existence at exactly the right trigger points
(catalog load settle, on-demand selection). Piggybacking avoids a second
filesystem/Foundation-call sweep over the catalog.

**Platform gating: `#[cfg(target_os = "macos")]` module with a no-op stub
elsewhere.**
Rationale: matches the existing pattern in this codebase for
platform-specific behavior and keeps `file_presence.rs` platform-agnostic by
calling a small function that's a real check on macOS and a constant
`false` elsewhere.

## Risks / Trade-offs

- [Risk] The Objective-C bridge call could panic or behave unexpectedly on
  an unusual path (symlink into a ubiquitous container, permission-denied
  directory) → Mitigation: wrap the resource-value query to return
  `Ok(false)` (not offloaded) on any error rather than propagating a
  failure into the verification pass; add a unit test for a
  non-ubiquitous-container path resolving to `false`.
- [Risk] `objc2-foundation`'s resource-value API surface may differ across
  its `0.3.x` releases → Mitigation: pin the exact API used behind the
  small wrapper module so a future `objc2-foundation` upgrade only requires
  changing one file.
- [Trade-off] No automated test can exercise a *real* evicted iCloud file
  (eviction requires actual iCloud sync state, not reproducible in CI) →
  Mitigation: unit-test the wrapper's non-offloaded paths (plain local file,
  nonexistent file) and rely on manual verification for the true-offloaded
  path, called out explicitly in tasks.md.

## Migration Plan

Additive change with a new struct field. `LibraryItemFile` is (de)serialized
in the on-disk catalog cache (`catalog_cache.rs`); `offloaded` needs
`#[serde(default)]` so existing cached entries without the field deserialize
as `offloaded: false`, which is corrected on the next verification pass
regardless. No other migration steps; no rollback concerns beyond reverting
the change.
