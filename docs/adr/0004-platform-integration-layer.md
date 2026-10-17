## Status

Proposed

## Context

Platform-specific behavior is scattered rather than centralized. `cfg
(target_os)` blocks sit in ten files, most of them views (`catalog_view.rs`,
`detail_panel_view.rs`, `settings_storage_view.rs`) rather than a dedicated
platform module. Each call site that needs to know what the current
platform supports either re-implements the branch or assumes a capability
exists and fails at the point of use.

Goal 6, OS-native integration, is the goal that exposes this most directly,
and it is mostly absent today: the in-flight `icloud-offloaded-file-
awareness` proposal covers reading iCloud download status via `objc2`,
which is the easier half. Tags are the harder half, because the three
target platforms do not offer equivalent primitives:

| Operation | macOS | Windows | Linux |
|---|---|---|---|
| File state | `NSURLUbiquitousItemDownloadingStatusKey` | `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` | present only |
| Tags | `_kMDItemUserTags` xattr | index only | `user.xdg.tags` xattr |
| Materialize | `startDownloadingUbiquitousItem` | open to trigger recall | not applicable |
| Reveal | `NSWorkspace` | `explorer /select,` | `xdg-open` |

Windows has no tag-storage primitive at all. A naive implementation that
treats the filesystem as authoritative for tags either loses tag data on
Windows (nothing to write it to) or diverges in behavior per platform in a
way users would notice and that no spec currently resolves. This is a
design problem, not an implementation gap: the question of which source
wins when the index and the filesystem disagree has to be answered before
any tag code is written, not discovered afterward.

This decision also depends on 0002: a platform trait needs a home that does
not require GPUI, since the MCP server (goal 10) and the desktop UI both
need to query platform capabilities, and the MCP server must not link
`dtrpg-ui`.

Alternative considered and rejected: treat the filesystem as the source of
truth for tags, with the index as a cache or projection of it. This was
rejected because it loses tags outright on Windows, which has no file-level
tag storage to read back from, and because it means the index's tag data
is only as reliable as whatever the filesystem last reported, rather than
the reverse.

## Decision

Define one platform-integration trait in `dtrpg_core::platform`, with
per-OS implementations gated behind Cargo's `[target.'cfg(...)'.dependencies]`
so platform-specific dependencies (`objc2`, `windows-rs`) never compile into
the wrong target. This replaces the ten scattered `cfg(target_os)` blocks
currently living across the UI crate's view files with one trait and one
implementation per platform.

The trait returns a `Capabilities` struct that the UI queries before
offering any OS-native affordance, so Linux does not show a materialize
button it cannot act on and Windows does not show a Finder-tag UI it has no
primitive for. This is a runtime capability check, not a documentation note
the user has to self-diagnose against.

Tags follow one rule: the SQLite index (0003) is the source of truth, and
the filesystem (xattrs on macOS and Linux) is a projection of it,
reconciled on launch. The index wins when the two disagree. Doing it the
other way round loses tags on Windows, which has no tag-storage primitive
to project onto at all, and makes the index's data only as trustworthy as
whatever the filesystem last reported on every platform, not just Windows.

## Consequences

- Adding a platform, or narrowing behavior for a build target within an
  existing OS, means implementing one trait, not auditing ten scattered
  `cfg(target_os)` call sites across view files.
- The MCP server (0002) can query the same `Capabilities` struct the
  desktop UI does, independent of whatever OS the GUI client happens to be
  running on, rather than assuming parity with `cfg` branches it cannot see.
- Tags survive on Windows, at the cost of never being able to treat a
  user's direct edit to a file's xattr-based tag (on macOS or Linux) as
  authoritative; it is reconciled against the index on the next launch and
  can be overwritten by it. This needs to be a documented behavior, not a
  silent one, since a user who edits a tag directly on disk and expects it
  to stick will be surprised otherwise.
- `platform` lives as a module inside `dtrpg-core`, not as a separate
  crate: Cargo's `[target.'cfg(...)'.dependencies]` mechanism already keeps
  `objc2` and `windows-rs` off the wrong platform, so a dedicated platform
  crate would buy isolation Cargo already provides for free, at the cost of
  one more crate to version and publish.
- `Platform` and `Capabilities` become part of `dtrpg-core`'s surface that
  `LibraryApi` (0002) depends on, so this ADR should land after or
  alongside 0002, not as a later refactor bolted onto a facade that already
  shipped without it.
