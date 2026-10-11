## Why

The `dtrpg-app` meta-repo's `define-shared-report-feature-bug` change defines a
language-agnostic baseline for in-app bug/feature reporting (Help menu + account
dropdown, a kind/subject/description dialog with diagnostics, browser-handoff
submission) and requires a Rust (gpui) child change to realize it — this is that
child change.

## What Changes

- Add a `ReportFeatureBug` action, registered in the Help menu (below About)
  and dispatched from the title bar account dropdown, so both entry points
  open the same dialog.
- Add `report_feature_bug_dialog.rs`: a `gpui_component::dialog` modal
  (following `manage_collections_dialog.rs`'s pattern) with a Bug/Feature
  Request radio selector, subject and description inputs, and a read-only
  diagnostics block (app version, commit + build date, target triple — reusing
  `build_info.rs`, the same source the About/Advanced-Settings diagnostics
  already use).
- Submit composes a percent-encoded GitHub "new issue" URL
  (`pilgrimagesoftware/dtrpg-app.rs/issues/new`) and opens it via
  `cx.open_url()` — no `gh` CLI, no stored token, no background network call,
  matching Fernrohr-App's submission mechanism per the parent change's design
  decision.
- Guard against stacking: opening the dialog while one is already open is a
  no-op (`window.has_active_dialog(cx)`).
- Add new i18n strings (`menu.help_report`, `title_bar.report`, `report.*`) to
  all three locales (`en`, `de`, `fr`).

## Capabilities

### New Capabilities

- `shared-report-feature-bug`: Implements the parent capability's Rust
  surface — entry points, dialog fields/validation, and browser-handoff
  submission, scoped to `dtrpg-app/rust`'s gpui UI.

## Impact

- `crates/dtrpg-ui/src/ui/actions.rs`: new `ReportFeatureBug` action.
- `crates/dtrpg-ui/src/ui/app/mod.rs`: Help menu item, app-level fallback
  handler (mirrors `About`'s).
- `crates/dtrpg-ui/src/ui/views/root_view.rs`: real `ReportFeatureBug` handler
  (opens the dialog).
- `crates/dtrpg-ui/src/ui/views/title_bar_view.rs`: account dropdown entry,
  dispatches the same action.
- `crates/dtrpg-ui/src/ui/views/report_feature_bug_dialog.rs`: new file — the
  dialog itself and URL-composition logic.
- `crates/dtrpg-ui/i18n/{en,de,fr}.yaml`: new strings.
- No new dependencies: URL percent-encoding is a small local helper rather
  than the `percent-encoding` crate, since the app has no other use for it.
