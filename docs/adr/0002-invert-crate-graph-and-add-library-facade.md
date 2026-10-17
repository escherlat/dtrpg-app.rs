## Status

Accepted

## Context

The crate split is inverted relative to what each crate holds. `dtrpg-ui`
is 29,212 lines and contains the domain models (`data/library.rs`),
persistence (`data/catalog_cache.rs`, `data/storage.rs`), the service
traits (`services/mod.rs`), and the credential store, none of which need a
UI framework. `dtrpg-core` is 3,489 lines: an SDK adapter plus the GPUI
application bootstrap. `dtrpg-core` depends on `dtrpg-ui`, not the other
way round, so what is named "core" is the thin crate and what is named "ui"
is the one carrying the domain. The `dtrpg_sdk` imports themselves stay
confined to `dtrpg-core/src/services/sdk`, so the service-trait seam is
sound; it is only the crate-level dependency direction that is backward.

Library state is held inside `LibraryController`, a `gpui::Entity`
implemented across 17 files under `controllers/library/`. It owns the
catalog, three independent work queues with duplicated concurrency-slot
bookkeeping, sort and filter and selection state, several ephemeral UI
`HashMap`s, and pending-download confirmation state. Splitting it across
files has kept per-file size down without narrowing what the type is
responsible for. Because it is a `gpui::Entity`, none of that state can be
read or mutated without a running GPUI application.

This is one of the twelve product goals' forcing functions. Goal 10 is an
optional MCP server over the user's library: a headless agent surface that
must not require a display. Today nothing headless can reach the library,
because every path to it runs through `dtrpg-ui` and therefore through
GPUI. Building the MCP server on the current structure means one of two
things: duplicate the catalog, persistence, and service logic inside the
MCP binary and maintain two copies that will drift, or link GPUI into a
server process that has no display and no reason to boot one. Neither is
acceptable, and this is the one goal among the twelve that cannot be built
incrementally on the current structure without one of those two outcomes.

## Decision

Invert the crate graph. Move domain models, persistence, service traits,
and the credential store out of `dtrpg-ui` into a `dtrpg-core` that has no
GPUI dependency. `dtrpg-ui` depends on `dtrpg-core`, not the reverse.

Add `dtrpg_core::api::LibraryApi` as the only `pub` way into the library
for callers outside the core. The desktop UI and the MCP server both go
through it, which is what keeps the two from drifting apart. Everything
else in `dtrpg-core` becomes `pub(crate)`, so the boundary is a compile
error to bypass, not a convention a future contributor can quietly route
around.

Two thin binary crates replace the current single binary: `dtrpg-desktop`
(depends on `dtrpg-core` and `dtrpg-ui`, wires dependency injection and
globals) and `dtrpg-mcp` (depends on `dtrpg-core` only, and must never link
`dtrpg-ui` or GPUI). The workspace grows from two crates to four.
`platform`, `api`, `archive`, and `insight` live as modules inside
`dtrpg-core`, not as further crates: Cargo's `[target.'cfg(...)'.dependencies]`
already keeps platform-specific dependencies off the wrong target, so a
separate platform crate would buy isolation Cargo already gives for free.

`LibraryController` is decomposed as part of this move: catalog data and
queue state move into `dtrpg-core` (see also the work-queue consolidation
named in the parent proposal), and the controller that remains in
`dtrpg-ui` keeps only per-pane view state, becoming a view-model over
`LibraryApi` rather than an owner of library state.

Alternative considered and rejected: duplicate the catalog and service
logic inside the MCP binary instead of relocating it. This was rejected
because it creates two implementations of the same domain logic with no
mechanism forcing them to agree, which is exactly the drift this decision
exists to prevent, and because it does nothing to fix the underlying
inversion that every other goal needing headless access would hit next.

## Consequences

- The MCP server (goal 10) becomes buildable at all, without linking GPUI
  or duplicating logic.
- `dtrpg-core` becomes testable and buildable without a display server,
  which benefits CI and lets the service and persistence layer be unit
  tested without a GPUI harness.
- The relocation diff is large and mostly mechanical: moving `data/`,
  `services/`, `credentials/`, and `models/` out of a 29k-line crate
  touches many files at once, and the dependency direction has to flip in
  one commit to keep the workspace compiling throughout. The move should be
  done with no behavior change and no reformatting, so the diff reviews as
  a rename; the queue, runtime, and controller decomposition land as
  separate follow-up commits on the same branch rather than folded into the
  same diff.
- 130 existing OpenSpec specs describe current behavior and are the
  regression bar for this move; the full test suite needs to run against
  each commit in the sequence, not just at the end, to catch behavior drift
  during the relocation.
- `LibraryApi` becomes a second public contract to keep stable, alongside
  `dtrpg-sdk`'s own API: a signature change there now has two downstream
  consumers (the desktop app, the MCP server) instead of one.
 - This ADR is the prerequisite for 0003 (SQLite index), 0004 (platform
   integration layer), and 0006 (container format abstraction): each of
   those assumes a headless `dtrpg-core` already exists. They should not
   land ahead of this one.

## Implementation checkpoint

Accepted on 2026-10-17 after the crate inversion, `LibraryApi` facade, desktop
and MCP entry-point split, SQLite index integration, and index-backed API reads
landed and passed workspace verification. The interactive GPUI catalog
intentionally retains its in-memory render model; controller queue
consolidation and the shared runtime are separate follow-up changes and do not
change this crate-boundary decision.
