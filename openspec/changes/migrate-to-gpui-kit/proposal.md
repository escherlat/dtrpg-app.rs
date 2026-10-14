## Why

CLAUDE.md already names moving from `gpui` + `gpui-component` to `gpui-kit`
(https://github.com/longbridge/gpui-kit) as a short-term goal. Issue #233
tracks it. See `docs/adr/0001-migrate-to-gpui-kit.md` for the full decision
record. Summary: a `gpui_component` usage inventory (35 files, 175 usages,
37 distinct types/traits across `crates/dtrpg-ui` and `crates/dtrpg-core`)
was cross-checked directly against gpui-kit 0.7's own source (an earlier
pass had relied on Fernrohr-App's call sites and significantly overstated
the gaps — Fernrohr-App not using a widget isn't evidence gpui-kit lacks
it). Direct source verification found a confirmed equivalent for every
widget in the inventory except one real builder-API reshape (Popover).
There is no custom-implementation or deferred-design-decision bucket in
this plan.

## What Changes

- Add `gpui-kit = "0.7"` alongside the existing `gpui-component` git
  dependency; the two coexist for the duration of the migration (no "big
  bang" cutover — risk of a single multi-thousand-line PR is unacceptable
  given 35 files are touched).
- Migrate `crates/dtrpg-ui` and `crates/dtrpg-core` off `gpui_component` in
  dependency order, not file-alphabetical order: leaf UI primitives first
  (`util`, `data`), then state-holding orchestration code
  (`controllers`, `controllers/library`), then every remaining view
  (`ui/views`, `ui/app`, `ui/library`) in one wave, since all of it is a
  confirmed port rather than new code.
- Remove the `gpui-component`/`gpui-component-assets` git dependencies and
  the coexistence shim only once `rg -l "gpui_component" crates/` is empty.
- No user-visible behavior change is intended; this is a dependency/API
  migration. Any visual regression surfaced during migration (e.g. a
  gpui-kit widget rendering a size/spacing differently) is a bug to fix in
  the same PR that introduced it, not a deferred follow-up.

## Capabilities

### New Capabilities

- `gpui-kit-migration`: Tracks the dependency swap itself — coexistence
  period, per-subdirectory sequencing, and the removal gate. Scoped to
  `dtrpg-app/rust`; no behavior-level spec changes (views keep their
  existing specs, which describe behavior, not the widget library that
  implements it).

## Impact

- `Cargo.toml` / `crates/*/Cargo.toml`: add `gpui-kit = "0.7"`; remove
  `gpui-component` + `gpui-component-assets` git dependencies at the end.
- `crates/dtrpg-core/src/app/assets.rs`: `gpui_component_assets::Assets`
  asset loader fallback — needs a gpui-kit equivalent or an in-house
  loader before `gpui-component-assets` can be dropped.
- `crates/dtrpg-ui/src/controllers/**`, `crates/dtrpg-ui/src/data/**`,
  `crates/dtrpg-ui/src/util/**`: low-risk, no UI-rendering `gpui_component`
  types beyond `Theme`/`ActiveTheme` globals and `Column`/`ColumnSort` table
  config — first wave.
- `crates/dtrpg-ui/src/ui/views/*.rs` (catalog_view.rs, detail_panel_view.rs,
  toolbar_view.rs, status_bar_view.rs, settings_*.rs, item_popover_view.rs,
  zip_preview_popover.rs, alert_history_view.rs, bulk_action_bar_view.rs,
  notification_banner_view.rs, sidebar_view.rs, tab_strip_view.rs,
  root_view.rs, settings_view.rs, report_feature_bug_dialog.rs,
  manage_collections_dialog.rs): third wave, widget-by-widget. All but one
  (Popover, in `status_bar_view.rs`) are confirmed import swaps; Popover's
  builder chain needs a hand rewrite (`.content(...)` →
  `.anchor(...).open(...).on_open_change(...)`).
- `crates/dtrpg-ui/src/ui/app/mod.rs`, `crates/dtrpg-ui/src/ui/library/**`:
  same (third) wave — app bootstrap (`Root`/`init`) and shared library
  widgets. No longer sequenced last: with no deferred-design-decision
  files remaining, there's no straddle to avoid.
- No SDK, service, or credential-store code is touched — this is UI-layer
  only.
