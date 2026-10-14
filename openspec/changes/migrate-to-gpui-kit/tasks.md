## 0. Pre-flight

- [x] 0.1 Clone `longbridge/gpui-kit` directly and verify every item from
      the research inventory against its actual source — done; see
      `docs/adr/0001-migrate-to-gpui-kit.md`. Only Popover needs a builder
      rewrite; everything else has a confirmed, near-identical equivalent.
- [ ] 0.2 Add `gpui-kit = "0.7"` to `crates/dtrpg-ui/Cargo.toml` and
      `crates/dtrpg-core/Cargo.toml` alongside the existing `gpui-component`
      dependency (coexistence, no removal yet)

## 1. `util/`, `data/`

- [ ] 1.1 Migrate `Theme`/`ActiveTheme` global usage to gpui-kit's
      equivalent (`crates/component/src/theme/`)
- [ ] 1.2 Migrate `Column`/`ColumnSort` table config types (direct
      equivalent: `crates/component/src/table/column.rs`)
- [ ] 1.3 `cargo check -p dtrpg-ui -p dtrpg-core --all-targets`, `cargo
      clippy -p dtrpg-ui -p dtrpg-core --all-targets -- -D warnings`

## 2. `controllers/`, `controllers/library/`

- [ ] 2.1 Migrate `TableState`/`TableEvent`/`TableDelegate` entity usage in
      `controllers/tabs.rs` to gpui-kit's `component::table` equivalents
      (same names: `DataTable`, `Table`, `TableDelegate`, `TableState`)
- [ ] 2.2 Migrate any `InputState`/`SelectState` entities held by
      controllers (not yet the views that render them)
- [ ] 2.3 `cargo check -p dtrpg-ui --all-targets`, `cargo clippy -p
      dtrpg-ui --all-targets -- -D warnings`

## 3. `ui/views/`, `ui/app/`, `ui/library/`

All of the below are import swaps plus adjusting whatever method-level
signature differs (verify per-file as you touch it; do not assume byte-for-
byte parity beyond what ADR 0001 confirmed at the type level). Group by file
to keep diffs reviewable; order within the wave doesn't matter since nothing
here blocks on anything else in the wave.

- [ ] 3.1 Button/ButtonVariants/ButtonCustomVariant — direct swap,
      `.primary()/.danger()/.ghost()` shortcuts confirmed present (all 19+
      files using Button)
- [ ] 3.2 Checkbox, Radio/RadioGroup — direct swap (`catalog_view.rs`,
      `manage_collections_dialog.rs`, `report_feature_bug_dialog.rs`)
- [ ] 3.3 Table/DataTable/TableDelegate/TableEvent/TableState — direct swap
      in `catalog_view.rs`, `detail_panel_view.rs` (entities already
      migrated in task 2.1)
- [ ] 3.4 DropdownMenu/PopupMenu/PopupMenuItem/ContextMenuExt — direct swap
      across `catalog_view.rs`, `toolbar_view.rs`, `bulk_action_bar_view.rs`,
      `settings_appearance_view.rs`, `title_bar_view.rs`, `status_bar_view.rs`
- [ ] 3.5 Icon/IconName, Spinner, Tooltip — direct swap, all files
- [ ] 3.6 Input/InputEvent/InputState, NumberInput — direct swap in
      remaining view files (entities already migrated in task 2.2)
- [ ] 3.7 **Popover — the one real rewrite.** `status_bar_view.rs`: replace
      `.content(...)` with `.anchor(Anchor::TopRight).open(...)
      .on_open_change(...)`; confirm the exact anchor/open-state wiring
      against `crates/component/src/popover.rs` while doing this file
- [ ] 3.8 Sizable/Size — direct swap; confirm `Avatar`'s pixel-exact sizing
      method against `crates/component/src/avatar/avatar.rs` while touching
      `settings_account_view.rs`
- [ ] 3.9 Clipboard — direct swap (`crates/component/src/clipboard.rs`) in
      `detail_panel_view.rs`, `alert_history_view.rs`
- [ ] 3.10 Select/SelectEvent/SelectState, Combobox — direct swap
      (`crates/component/src/select.rs`, `combobox.rs`) in `root_view.rs`,
      `settings_appearance_view.rs`
- [ ] 3.11 Dialog/DialogAction/DialogClose/DialogFooter/DialogHeader/
      DialogTitle/DialogContent — construction pattern is unchanged
      (`window.open_dialog(cx, |dialog, ...| dialog.content(...))` still
      exists); swap imports only, in `root_view.rs`,
      `report_feature_bug_dialog.rs`, `manage_collections_dialog.rs`,
      `sidebar_view.rs`
- [ ] 3.12 Sidebar/SidebarMenu/SidebarMenuItem — direct swap
      (`crates/component/src/sidebar/`) in `sidebar_view.rs`,
      `settings_view.rs`
- [ ] 3.13 Tab/TabBar — direct swap (`crates/component/src/tab/`) in
      `tab_strip_view.rs`, `toolbar_view.rs`
- [ ] 3.14 ResizableState/h_resizable/resizable_panel — direct swap
      (`crates/base/src/resizable/`) in `root_view.rs`
- [ ] 3.15 Avatar — direct swap (`crates/component/src/avatar/`) in
      `settings_account_view.rs`
- [ ] 3.16 Separator, StatusBar, Badge, Alert, Collapsible,
      DescriptionItem/DescriptionList, Notification/NotificationType,
      ProgressCircle, Switch — direct swap at their respective call sites
      (`status_bar_view.rs`, `catalog_view.rs`, `detail_panel_view.rs`,
      `settings_advanced_view.rs`, `settings_storage_view.rs`,
      `root_view.rs`)
- [ ] 3.17 `Root`/`init` app bootstrap in `ui/app/mod.rs` (direct swap:
      `crates/component/src/lib.rs::init`, `crates/base/src/root.rs::Root`)
- [ ] 3.18 Disableable/ElementExt/ScrollableElement/WindowExt/Selectable
      traits — direct swap (`crates/base/src/component_traits.rs`,
      `element_ext.rs`, `crates/component/src/window_ext.rs`,
      `scroll/scrollable.rs`) across all files importing them
- [ ] 3.19 TextView/markdown rendering — direct swap
      (`crates/base/src/text/`, `text_view.rs`, `format/markdown.rs`) in
      `ui/widgets.rs`, `detail_panel_view.rs`
- [ ] 3.20 Shared widgets in `ui/library/` (e.g. `drag.rs`'s `ActiveTheme`
      usage) — direct swap
- [ ] 3.21 `cargo check --workspace --all-targets`, `cargo clippy
      --workspace --all-targets -- -D warnings`, `cargo +nightly fmt
      --check`, `cargo test --workspace`

## 4. Cutover

- [ ] 4.1 `rg -l "gpui_component" crates/` returns empty
- [ ] 4.2 Remove `gpui-component`/`gpui-component-assets` git dependencies
      from every `Cargo.toml`; replace `gpui_component_assets::Assets`
      fallback in `crates/dtrpg-core/src/app/assets.rs` with a gpui-kit
      equivalent (`crates/assets/`) or in-house loader
- [ ] 4.3 Update `CLAUDE.md`'s "Frameworks" and "gpui-component Usage"
      sections to describe gpui-kit instead
- [ ] 4.4 Full workspace build/lint/test pass; manual smoke test left to
      the user (no automated UI verification)
