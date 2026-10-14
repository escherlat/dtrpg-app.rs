## Why

CLAUDE.md already names moving from `gpui` + `gpui-component` to `gpui-kit`
(https://github.com/longbridge/gpui-kit) as a short-term goal. Issue #233
tracks it. See `docs/adr/0001-migrate-to-gpui-kit.md` for the full decision
record, including its round-3 amendment. Summary: `gpui-kit` forks
zed-industries/zed's `gpui` under its own package family (`gpui-pre`,
reached transitively through `gpui-component` 0.7.3), with its own
`Context`/`Window`/`App`/`Global`/`Entity` types. Those cannot be bridged to
our existing `gpui` types from local code — the orphan rule blocks any
`impl` connecting two foreign types/traits — so there is no partial,
file-by-file, or wave-by-wave migration state that compiles. This is a
single all-or-nothing rewrite: the app's bootstrap and every `Context`/
`Window`/`App` parameter in `dtrpg-ui` and `dtrpg-core` switch together, in
one unit of work. The widget-level mapping (35 files, 175 usages, confirmed
equivalent for nearly every type except Popover's reshaped builder) still
holds as the reference for what to port each type to.

## What Changes

- Replace `gpui-component` (and effectively `gpui`/`gpui_platform`, since
  the app's entry point and every render-tree signature move to gpui-kit's
  fork) with `gpui-kit = "0.7"` in the same change — no coexistence period;
  nothing compiles until the whole crate pair agrees on one set of
  `Context`/`Window`/`App`/`Global` types.
- Switch `dtrpg-core`'s app bootstrap from `gpui_platform::application()` to
  `gpui_kit::application()`, and every `Context<Self>`/`&mut Window`/`App`
  parameter across every view and controller in `dtrpg-ui` to gpui-kit's
  types, together.
- Add small local conversion helpers for value types that are
  field-identical but distinct across the fork boundary (`Hsla`,
  `SharedString`) — needed wherever a value crosses from app code into a
  gpui-kit widget.
- Remove the `gpui-component`/`gpui-component-assets` git dependencies once
  `rg -l "gpui_component" crates/` returns empty.
- No user-visible behavior change is intended; this is a dependency/API
  migration. Any visual regression surfaced during migration (e.g. a
  gpui-kit widget rendering a size/spacing differently) is a bug to fix in
  the same change, not a deferred follow-up.

## Capabilities

### New Capabilities

- `gpui-kit-migration`: Tracks the dependency swap itself. Scoped to
  `dtrpg-app/rust`; no behavior-level spec changes (views keep their
  existing specs, which describe behavior, not the widget library that
  implements it).

## Impact

- `Cargo.toml` / `crates/*/Cargo.toml`: add `gpui-kit = "0.7"`; remove
  `gpui-component` + `gpui-component-assets` git dependencies in the same
  change.
- `crates/dtrpg-core/src/app/mod.rs`: app-bootstrap entry point
  (`gpui_platform::application()` → `gpui_kit::application()`).
- `crates/dtrpg-core/src/app/assets.rs`: `gpui_component_assets::Assets`
  asset loader fallback — needs a gpui-kit equivalent or an in-house loader.
- Every `.rs` file in `crates/dtrpg-ui/src/{controllers,data,ui,util}/**`
  and `crates/dtrpg-core/src/**` that holds a `Context`/`Window`/`App`
  parameter or a `gpui_component` type — all 35 files from the original
  inventory, touched together rather than in sequenced waves. See
  `tasks.md` for the per-type/per-file breakdown (still useful as a
  checklist; no longer a sequencing plan).
- No SDK, service, or credential-store code is touched — this is UI-layer
  only.
