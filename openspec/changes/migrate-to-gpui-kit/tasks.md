This is now a single unit of work (see ADR 0001's round-3 amendment): the
tree does not compile until every section below lands together. Checklist
order reflects a sensible editing sequence, not independent checkpoints —
only the final verification (section 5) gates "done."

## 0. Pre-flight

- [x] 0.1 Clone `longbridge/gpui-kit` directly and verify every item from
      the research inventory against its actual source — done; see
      `docs/adr/0001-migrate-to-gpui-kit.md`. Only Popover needs a builder
      rewrite; everything else has a confirmed, near-identical equivalent.
- [x] 0.2 Add `gpui-kit = "0.7"` to `crates/dtrpg-ui/Cargo.toml` and
      `crates/dtrpg-core/Cargo.toml` alongside the existing `gpui-component`
      dependency — done, resolved to 0.7.3, commit 236e0d8 on
      feature/233-migrate-to-gpui-kit. (Superseded by section 4: this
      coexistence is temporary scaffolding for the rewrite, not the final
      state — `gpui-component` is removed once the rewrite lands.)

## 1. App bootstrap

- [ ] 1.1 `crates/dtrpg-core/src/app/mod.rs` (or wherever `main`/`setup`
      calls `gpui_platform::application()`): switch to
      `gpui_kit::application()`. Everything downstream of this call now runs
      on gpui-kit's `Context`/`Window`/`App`, so this has to land in the same
      commit as every view/controller signature change below — there is no
      intermediate state where the bootstrap is swapped but views aren't, or
      vice versa.
- [ ] 1.2 `crates/dtrpg-ui/src/ui/app/mod.rs`: `init(cx)`/`Root` swap to
      `gpui_kit::component::init`/`gpui_kit::base::Root` (confirm exact
      paths against the source, not just the names used in ADR 0001).

## 2. Value-type conversion helpers

- [ ] 2.1 Write the `Hsla` conversion helper once (field-wise `h`/`s`/`l`/`a`
      copy between `gpui::Hsla` and `gpui_kit::Hsla` — the orphan rule blocks
      a blanket `From`/`Into`, so this is a local free function or extension
      trait, not a derive). Used wherever a Libri color value crosses into a
      gpui-kit widget — most prominently `data/theme.rs`'s
      `apply_theme_colors`.
- [ ] 2.2 Write the `SharedString` conversion helper (`gpui_kit::SharedString
      ::from(our_str.as_ref())`, since gpui-kit's `SharedString` has a
      `From<&str>` impl).
- [ ] 2.3 Check whether `Pixels`/`Point`/`Bounds`/other geometry types need
      the same treatment — not yet confirmed either way; verify against
      gpui-kit source before assuming parity or needing a helper.

## 3. Global/state types

- [ ] 3.1 `Theme`/`ActiveTheme` global: `data/theme.rs`'s
      `apply_theme_colors` signature, plus every `cx.update_global::<Theme,
      _>`/`cx.global::<Theme>()` call site (`controllers/library/
      preferences.rs`, `ui/app/mod.rs`) — all must move to
      `gpui_kit::component::Theme` together, using the conversion helpers
      from section 2.
- [ ] 3.2 `Column`/`ColumnSort` table config types (`ui/views/catalog_view
      .rs`) — direct equivalent, `crates/component/src/table/column.rs`.
- [ ] 3.3 `TableState`/`TableEvent`/`TableDelegate` entity usage
      (`controllers/tabs.rs`'s `Entity<TableState<ItemListDelegate>>`,
      `ui/views/detail_panel_view.rs`'s `ItemListDelegate` implementing
      `TableDelegate`, `ui/views/catalog_view.rs`'s table usage) — these
      three files' table types must move together; the delegate's trait impl
      and every holder of the entity have to agree on which `TableState`
      they mean.
- [ ] 3.4 `InputState`/`SelectState` entities held by controllers and
      rendered by views — same cross-file constraint as 3.3.

## 4. Widget-level import swaps

All of the below are import swaps plus adjusting whatever method-level
signature differs (verify per-file as you touch it; do not assume byte-for-
byte parity beyond what ADR 0001 confirmed at the type level).

- [ ] 4.1 Button/ButtonVariants/ButtonCustomVariant — direct swap,
      `.primary()/.danger()/.ghost()` shortcuts confirmed present (all 19+
      files using Button)
- [ ] 4.2 Checkbox, Radio/RadioGroup — direct swap (`catalog_view.rs`,
      `manage_collections_dialog.rs`, `report_feature_bug_dialog.rs`)
- [ ] 4.3 DropdownMenu/PopupMenu/PopupMenuItem/ContextMenuExt — direct swap
      across `catalog_view.rs`, `toolbar_view.rs`, `bulk_action_bar_view.rs`,
      `settings_appearance_view.rs`, `title_bar_view.rs`, `status_bar_view.rs`
- [ ] 4.4 Icon/IconName, Spinner, Tooltip — direct swap, all files
- [ ] 4.5 Input/InputEvent, NumberInput — direct swap in remaining view files
      (entity types already migrated in task 3.4)
- [ ] 4.6 **Popover — the one real rewrite.** `status_bar_view.rs`: replace
      `.content(...)` with `.anchor(Anchor::TopRight).open(...)
      .on_open_change(...)`; confirm the exact anchor/open-state wiring
      against `crates/component/src/popover.rs` while doing this file
- [ ] 4.7 Sizable/Size — direct swap; confirm `Avatar`'s pixel-exact sizing
      method against `crates/component/src/avatar/avatar.rs` while touching
      `settings_account_view.rs`
- [ ] 4.8 Clipboard — direct swap (`crates/component/src/clipboard.rs`) in
      `detail_panel_view.rs`, `alert_history_view.rs`
- [ ] 4.9 Select/SelectEvent, Combobox — direct swap
      (`crates/component/src/select.rs`, `combobox.rs`) in `root_view.rs`,
      `settings_appearance_view.rs` (entity types already migrated in 3.4)
- [ ] 4.10 Dialog/DialogAction/DialogClose/DialogFooter/DialogHeader/
      DialogTitle/DialogContent — construction pattern is unchanged
      (`window.open_dialog(cx, |dialog, ...| dialog.content(...))` still
      exists); swap imports only, in `root_view.rs`,
      `report_feature_bug_dialog.rs`, `manage_collections_dialog.rs`,
      `sidebar_view.rs`
- [ ] 4.11 Sidebar/SidebarMenu/SidebarMenuItem — direct swap
      (`crates/component/src/sidebar/`) in `sidebar_view.rs`,
      `settings_view.rs`
- [ ] 4.12 Tab/TabBar — direct swap (`crates/component/src/tab/`) in
      `tab_strip_view.rs`, `toolbar_view.rs`
- [ ] 4.13 ResizableState/h_resizable/resizable_panel — direct swap
      (`crates/base/src/resizable/`) in `root_view.rs`
- [ ] 4.14 Avatar — direct swap (`crates/component/src/avatar/`) in
      `settings_account_view.rs`
- [ ] 4.15 Separator, StatusBar, Badge, Alert, Collapsible,
      DescriptionItem/DescriptionList, Notification/NotificationType,
      ProgressCircle, Switch — direct swap at their respective call sites
      (`status_bar_view.rs`, `catalog_view.rs`, `detail_panel_view.rs`,
      `settings_advanced_view.rs`, `settings_storage_view.rs`,
      `root_view.rs`)
- [ ] 4.16 Disableable/ElementExt/ScrollableElement/WindowExt/Selectable
      traits — direct swap (`crates/base/src/component_traits.rs`,
      `element_ext.rs`, `crates/component/src/window_ext.rs`,
      `scroll/scrollable.rs`) across all files importing them
- [ ] 4.17 TextView/markdown rendering — direct swap
      (`crates/base/src/text/`, `text_view.rs`, `format/markdown.rs`) in
      `ui/widgets.rs`, `detail_panel_view.rs`
- [ ] 4.18 Shared widgets in `ui/library/` (e.g. `drag.rs`'s `ActiveTheme`
      usage) — direct swap

## 5. Cutover and verification

- [ ] 5.1 `rg -l "gpui_component" crates/` returns empty
- [ ] 5.2 Remove `gpui-component`/`gpui-component-assets` git dependencies
      from every `Cargo.toml`; replace `gpui_component_assets::Assets`
      fallback in `crates/dtrpg-core/src/app/assets.rs` with a gpui-kit
      equivalent (`crates/assets/`) or in-house loader
- [ ] 5.3 Update `CLAUDE.md`'s "Frameworks" and "gpui-component Usage"
      sections to describe gpui-kit instead
- [ ] 5.4 `cargo check --workspace --all-targets`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo +nightly fmt --check`, `cargo
      test --workspace` — this is the first point in the unit where all of
      these are expected to pass; manual smoke test left to the user (no
      automated UI verification)
