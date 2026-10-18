## Status

Accepted

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

## Implementation checkpoint

Accepted on 2026-10-17. The platform boundary now owns capability reporting,
native reveal, cloud-file materialization hooks, and file availability state.
The desktop UI routes reveal actions, platform labels, file-presence checks,
settings capability text, and title-bar platform behavior through
`dtrpg_core::platform`; those views no longer contain platform-selection
branches. Filesystem tag projection remains the dependent tags feature's
responsibility, using the source-of-truth rule defined above.

## Audit of the remaining `cfg(target_os)` blocks

Completed 2026-10-18 (stage 4 of `libri-v2-architecture`, tasks 4.4 and 4.5).
Every remaining block was checked for whether it is platform behavior, which
belongs behind the trait, or a path or build constant, which does not.

| Site | What it does | Decision |
|---|---|---|
| `dtrpg-core` `data/constants.rs`, `data/paths.rs` | Per-OS default directories | Stays: a constant, not behavior |
| `dtrpg-core` `logging.rs` | Per-OS log directory | Stays: a path |
| `dtrpg-core` `data/file_openers.rs` | Config file location (`%APPDATA%` or `~/.config`) | Stays: a path |
| `dtrpg-ui` `util/symlink.rs` | `symlink` versus `symlink_file`, in one function | Stays: one standard-library call with a single caller path, isolated in a util and not in a view |
| `dtrpg-ui` `util/datetime.rs` | Locale-aware date formatting through `objc2` on macOS | Stays: presentation formatting, and moving it would add an Objective-C dependency to `dtrpg-core` |

None of these is a view, and none decides what the user is offered, so none
needs a capability. The requirement that views contain no platform branches
holds.

## Per-OS verification

Checked against the table above, the implementations differed from it in
three places, now fixed:

- **Windows file state** reported every existing file as local although
  `Capabilities::file_state` is `true`. It now reads the recall and offline
  attributes (`FileState::from_windows_attributes`).
- **Windows reveal** treated `explorer`'s exit status 1 as a failure. Explorer
  exits with 1 even when it opens the window, so only a failure to start it
  is an error now.
- **macOS file state** asked `mdls` for an `NSURL` key that `mdls` does not
  expose, so it always answered local. It now reads the `SF_DATALESS` bit of
  `st_flags` (`FileState::from_macos_flags`), and treats a missing file with a
  hidden `.name.icloud` sibling as a placeholder, which is how iCloud Drive
  offloaded files before macOS 14. This reads the file system directly, so no
  `objc2` call is needed for file state; `NSURLUbiquitousItemDownloadingStatusKey`
  in the table is the documented equivalent, not the mechanism.

The decoding is pure and tested on every platform. Reading the real flags
from a real offloaded file is not covered by an automated test, because CI
cannot produce one.
