## Context

See the parent change's `design.md` in `dtrpg-app/openspec/changes/define-shared-report-feature-bug`
for the cross-app decision record (browser-handoff over `gh`-CLI, kind
selector included, no log attachment, no Swift implementation here).

This change only covers how that baseline is realized in `dtrpg-app/rust`.

## Decisions

- **Diagnostics source: `build_info.rs`, not a fresh `env!(CARGO_PKG_VERSION)` +
  `std::env::consts::OS` pair.** The parent's design doc assumed no
  build-identifier mechanism existed; `crates/dtrpg-ui/src/build_info.rs`
  already captures `GIT_HASH`, `BUILD_DATE`, and `TARGET` at build time and is
  already used by the About dialog / Advanced Settings diagnostics section.
  Reusing it keeps one source of truth and gives a materially more useful
  "build identifier" (commit + date) than version alone.
- **Dialog pattern: `manage_collections_dialog.rs`'s `Rc<RefCell<_>>` +
  `window.refresh()`,** not a new `Entity`-backed view. The kind selector is
  the only state that isn't already a GPUI entity (`InputState` handles
  subject/description reactively); this matches the established pattern for
  exactly that situation in this codebase.
- **No new crate dependency for percent-encoding.** A dozen-line local
  `encode()` helper (keep ASCII alphanumerics, `%XX`-escape everything else)
  covers the one call site; Fernrohr-App's `percent-encoding` crate dependency
  isn't otherwise needed here.
- **`has_active_dialog` guard for no-stacking**, per the parent spec's
  "Repeated invocation does not stack dialogs" scenario — `open_dialog`
  itself has no such guard (it pushes onto a stack), so this change adds it
  explicitly rather than relying on framework behavior.

## Migration Plan

Net-new feature; no existing behavior changes.
