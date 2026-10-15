This was a single unit of work (see ADR 0001's round-3 amendment): the tree
did not compile until every section below landed together. Done — see
commit on `feature/233-migrate-to-gpui-kit`.

## 0. Pre-flight

- [x] 0.1 Clone `longbridge/gpui-kit` directly and verify every item from
      the research inventory against its actual source — done; see
      `docs/adr/0001-migrate-to-gpui-kit.md`.
- [x] 0.2 Add `gpui-kit = "0.7"` to `crates/dtrpg-ui/Cargo.toml` and
      `crates/dtrpg-core/Cargo.toml` alongside the existing `gpui-component`
      dependency — superseded by section 4/5, which removed the coexistence
      scaffolding once the rewrite landed.

## 1. App bootstrap

- [x] 1.1 Workspace `Cargo.toml`: renamed `gpui`/`gpui_platform` from the
      zed-industries/zed git dependency to the `gpui-pre`/`gpui-pre-platform`
      crates.io packages gpui-kit itself depends on (pinned `=0.3.9`,
      matching gpui-kit 0.7.3's own pin) — this is the key move: it makes
      our `Context`/`Window`/`App`/`Global`/`Hsla`/`SharedString` the exact
      same types gpui-kit's widgets use, eliminating the orphan-rule
      conversion problem entirely rather than needing per-value-type
      helpers (section 2 below turned out to be unnecessary — see note
      there). `crates/dtrpg-core/src/app/mod.rs`'s `open_library_window`/
      `open_settings_window` switched from `cx.open_window` + manual
      `Root::new(view, window, cx).bordered(false)` to
      `gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| View::new(...)))`,
      which wraps the view in `Root` internally. `WindowHandle<Root>` is
      recovered via `AnyWindowHandle::downcast::<Root>()` where callers need
      it (window activation, `AddCollection`'s dialog path).
- [x] 1.2 `ui/app/mod.rs`: `init`/`Root` resolve via
      `gpui_kit::component::{init, Root}` (component re-exports
      `gpui_base::Root`) — same names as guessed in ADR 0001, confirmed
      working.
- [x] 1.2b `ui/views/root_view.rs` and `ui/views/settings_window_view.rs`:
      removed the manual `Root::render_sheet_layer`/`render_dialog_layer`/
      `render_notification_layer` calls (and their `.children(...)`
      composition) — gpui-kit's `Root` renders these layers itself via its
      own `Render` impl now; the wrapped view doesn't call anything. This
      wasn't anticipated by ADR 0001; found by reading gpui-kit's own
      `StoryRoot` example (`crates/story/src/lib.rs`) after hitting
      `no associated function render_sheet_layer`.

## 2. Value-type conversion helpers

- [x] 2.1-2.3 **Turned out to be unnecessary.** Once `gpui`/`gpui_platform`
      were repointed to the same `gpui-pre` fork gpui-kit uses (task 1.1),
      `gpui::Hsla`/`gpui::SharedString`/etc. in our own code already *are*
      gpui-kit's types — no conversion needed anywhere. The Hsla-conversion
      helper written during an earlier (reverted) coexistence attempt was
      deleted along with that attempt.

## 3. Global/state types

- [x] 3.1-3.4 No cross-file sequencing needed in practice: once task 1.1
      landed, `Theme`, `TableState`/`TableEvent`/`TableDelegate`,
      `InputState`/`SelectState` all type-checked as ordinary import swaps
      — the "must move together" constraint ADR 0001 anticipated was a
      symptom of the two-crate problem task 1.1 eliminated, not an
      independent sequencing issue.

## 4. Widget-level import swaps

- [x] 4.1-4.18 Bulk `gpui_component::` → `gpui_kit::component::` rename
      across all 31 affected files, then fixed real exceptions found by
      `cargo check`:
      - **Popover**: did not need the anticipated `.anchor()/.open()/
        .on_open_change()` rewrite — `status_bar_view.rs`'s usage compiled
        as a straight import swap. (ADR 0001's Popover-reshape finding was
        itself based on Fernrohr-App's call site, not gpui-kit's own API
        shape at this version — turned out not to need the predicted
        rewrite after all.)
      - **Multi-line input**: `InputState::multi_line(true)` no longer
        exists — multi-line is now a distinct type, `TextareaState`
        (`InputBaseState<TextareaMode>`), rendered with `Textarea::new`
        instead of `Input::new`. Fixed in
        `report_feature_bug_dialog.rs` (the one call site using it).
      - Every other widget (Button, Checkbox, Radio, Table/DataTable,
        DropdownMenu, Icon, Spinner, Tooltip, Sizable/Size, Clipboard,
        Select/Combobox, Dialog, Sidebar, Tab, resizable panels, Avatar,
        Separator/StatusBar/Badge/Alert/Collapsible/DescriptionList/
        Notification/ProgressCircle/Switch, Disableable/ElementExt/
        ScrollableElement/WindowExt/Selectable traits, TextView) — direct
        rename, no signature changes needed.

## 5. Cutover and verification

- [x] 5.1 `rg -l "gpui_component" crates/` returns empty (two harmless
      historical-comment mentions of `gpui_component_assets`/`gpui-component`
      remain in `app/assets.rs` and `util/text.rs`'s doc comments, describing
      prior behavior — not code).
- [x] 5.2 Removed `gpui-component`/`gpui-component-assets` from every
      `Cargo.toml`. `crates/dtrpg-core/src/app/assets.rs`'s fallback now
      uses `gpui_kit::assets::Assets` (same `AssetSource` shape as the old
      `gpui_component_assets::Assets`, confirmed via
      `gpui-kit-assets`'s `native_assets.rs`) — one behavior note: gpui-kit's
      `load()` returns `Err` for an unknown path rather than the old
      `Ok(None)`, downgraded explicitly to preserve the old contract.
- [x] 5.3 Updated `AGENTS.md`/`CLAUDE.md`'s Frameworks and gpui-component
      Usage sections.
- [x] 5.4 `cargo check --workspace --all-targets`: clean. `cargo clippy
      --workspace --all-targets -- -D warnings`: clean. `cargo +nightly fmt
      --check` (via `rustup run nightly cargo fmt`): clean after one
      `cargo fmt` pass (two import lines exceeded width after the rename).
      `cargo test --workspace`: 54 (dtrpg-core) + 338 (dtrpg-ui) + 12
      (doc-tests) passed, 0 failed.

## 6. Manual smoke test findings (fixed post-merge-readiness)

- [x] 6.1 Avatar photo rendered squished (thin vertical sliver) in both the
      title bar and settings account view. Root causes, found in order:
      `Avatar::src()` itself works fine (confirmed against gpui-kit.com's own
      demo); the title-bar instance was being squashed by `Button::render()`
      applying its own ~10px padding whenever `.child(..)` is non-empty (its
      "normal button" sizing branch, since passing the avatar via `.child()`
      disqualifies the no-padding "icon button" branch) — `.p(px(0.0))` on
      the `Button` cancels it.
- [x] 6.2 Avatar photo wasn't circularly clipped — `overflow_hidden()` on a
      wrapping `div` only clips children to a rectangular bounds mask in
      this gpui-kit version (`Style::overflow_mask` never reads
      `corner_radii`); not applicable once 6.1's fix let `Avatar`'s own
      internal clipping (which already applies `corner_radii` directly to
      the image at its own paint call) render correctly.
- [x] 6.3 Most non-button interactive controls (list rows, menu items,
      switches, sliders) showed a glaring light/white hover background under
      dark themes like Ink. `apply_theme_colors` only covered the
      button/table/sidebar/tab/status-bar token families; `accent`,
      `accent_foreground`, and the list/group-box/switch/slider/link family
      were left at gpui-kit's light-theme defaults. Extended the mapping to
      cover them from the existing `ColorTokens` palette.
