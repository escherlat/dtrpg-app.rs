## Status

Accepted

## Context

CLAUDE.md already names moving off `gpui-component` (Longbridge's git
dependency, pinned at `be4c5d30e0a51d5bfb2df93477a05050a50bf889`) to
`gpui-kit` as a short-term goal; issue #233 tracks it. `gpui-component` has
no published crates.io releases and no versioning discipline — the app
tracks a single pinned commit by hand. `gpui-kit` is Longbridge's actively
maintained successor, published to crates.io (`0.7.0` confirmed via
Fernrohr-App's `Cargo.lock`), with proper semver releases.

This is a hard-to-reverse framework swap: 35 files and 175 usages across
`crates/dtrpg-ui` and `crates/dtrpg-core` depend on `gpui_component` types,
and the UI framework underlies every view in the app.

Two rounds of research preceded this decision:

1. An inventory of every `gpui_component` usage in the app, cross-referenced
   against gpui-kit 0.7's call sites **in Fernrohr-App** (a sibling app using
   gpui-kit). That round concluded a long list of widgets — Sidebar, Tabs,
   resizable split panes, Avatar, Alert, Badge, Collapsible, DescriptionList,
   Notification, ProgressCircle, Separator, StatusBar, NumberInput, Switch,
   Clipboard, searchable Select — had **no gpui-kit equivalent**, and that
   Dialog's construction pattern was structurally different.
2. A direct clone and grep of `longbridge/gpui-kit`'s own source
   (`crates/component/src/`, `crates/base/src/`), done specifically because
   round 1's "no equivalent" conclusions rested on Fernrohr-App not calling a
   widget, which proves Fernrohr-App didn't need it — not that gpui-kit lacks
   it.

Round 2 overturned nearly all of round 1's gaps:

- `avatar/`, `sidebar/`, `tab/`, `progress/` (progress_circle.rs),
  `searchable_list/`, `setting/` all exist as dedicated modules under
  `crates/component/src/`.
- `alert.rs`, `badge.rs`, `collapsible.rs`, `description_list.rs`,
  `notification.rs`, `separator.rs`, `status_bar.rs` all exist as standalone
  files under `crates/component/src/`, with APIs matching or nearly matching
  the old `gpui_component` shape (e.g. `Notification::new().message(...)
  .with_type(...).autohide(...).on_click(...)` is unchanged).
- `crates/base/src/resizable/{mod,panel,resize_handle}.rs` exports
  `h_resizable()`, `resizable_panel()`, `ResizableState` — same names, same
  shape as the old code.
- `crates/base/src/{number_input,switch}.rs` and
  `crates/component/src/clipboard.rs` exist directly.
- `crates/component/src/select.rs` and `crates/component/src/combobox.rs`
  both exist, both generic over a `SearchableListDelegate` trait
  (`crates/component/src/searchable_list/`) — the old
  `Select`/`SearchableListDelegate` pairing has a direct home.
- `crates/base/src/component_traits.rs` defines `Selectable`, `Disableable`,
  and `Collapsible` as traits, matching the old `gpui_component` trait set.
  `crates/component/src/window_ext.rs` and `crates/base/src/element_ext.rs`
  cover `WindowExt`/`ElementExt`.
- `ButtonVariants` (`crates/component/src/button/button.rs`) still exposes
  `.primary()`, `.danger()`, `.ghost()` shortcut methods alongside
  `.with_variant(ButtonVariant::X)` — the old chaining style is not gone.
- Dialog's construction pattern is **unchanged**: `window.open_dialog(cx,
  |dialog, window, cx| dialog.content(...).title(...).footer(...)
  .on_ok(...))` is defined in `crates/component/src/window_ext.rs` and
  `crates/component/src/root.rs`, called the same way as the old
  `gpui_component::dialog` API. Round 1's "structural rewrite" conclusion was
  wrong — it mistook Fernrohr-App's own `Cancel`/`Confirm` helper buttons
  (app-local, not gpui-kit exports) for a different library-level pattern.
- `DataTable`/`Table` (`crates/component/src/table/{data_table,table}.rs`),
  `Column`/`ColumnSort` (`crates/component/src/table/column.rs`), `Icon`,
  `Spinner`, `Tooltip`, `Checkbox`, `Radio`/`RadioGroup`, `Input`/
  `InputState`, `Size` (`crates/component/src/sizing.rs`), `Popover`,
  `DropdownMenu`/`PopupMenuItem` all exist with names/shapes matching or
  closely matching the inventory's expectations.
- `TextView`/markdown rendering lives under `crates/base/src/text/` (which
  has a dedicated `format/markdown.rs`), not under `component/src/text/` as
  guessed — a module-path correction, not a missing-feature one.
- `pub fn init(cx: &mut App)` exists in `crates/component/src/lib.rs`;
  `Root` in `crates/base/src/root.rs`.

What remains genuinely unconfirmed (not verified past "the type exists" —
exact signature parity needs confirming file-by-file during migration, not
before it): precise field/method names on `Avatar` sizing,
`ActiveTheme`/`Theme` global registration shape, and `Popover`'s exact
builder surface (`.anchor()`/`.open()`/`.on_open_change()` vs the old
`.content()` — this one specific reshape is real, confirmed from the earlier
inventory's own call-site reading of Fernrohr-App, and not contradicted by
anything found in round 2).

## Decision

Adopt `gpui-kit` 0.7 as the UI framework, replacing `gpui-component`
entirely. Migrate in dependency-ordered waves (documented in the
`migrate-to-gpui-kit` OpenSpec change: `util`/`data` → `controllers` →
`ui/views` → `ui/app`/`ui/library`), with `gpui-kit` and `gpui-component`
coexisting as parallel dependencies until the last file's usage is removed.

No widget in the current inventory is being deferred or stubbed out for lack
of a gpui-kit equivalent — round 2 found one for every widget except the
Popover builder reshape, which is a mechanical rewrite (new method names,
same concept), not a missing-feature gap. There is accordingly no "fallback:
defer" item on this decision; every migration task in `tasks.md` is "port,"
not "design something new."

Risk ranking, revised from round 2 (lowest risk first, given every item now
has a confirmed target):

1. **Mechanical, low-risk ports** — everything with a name-for-name or
   near-identical API match: Button, Checkbox, Radio/RadioGroup, Table/
   DataTable, Icon, Input/InputState, Spinner, Tooltip, Theme/init/Root,
   Avatar, Alert, Badge, Collapsible, DescriptionList, Notification,
   ProgressCircle, Separator, StatusBar, Sidebar, Tabs, resizable panels,
   NumberInput, Switch, Clipboard, Select/Combobox, Selectable/Disableable/
   WindowExt/ElementExt traits, Dialog (construction pattern is unchanged).
2. **Reshaped but confirmed — Popover.** The only widget where the inventory
   found a real builder-API difference (`.content()` → `.anchor()/.open()/
   .on_open_change()`); every call site needs its builder chain rewritten,
   not just its import path. Low file count (one: `status_bar_view.rs`) but
   100% of its call sites need a hand rewrite rather than a search-replace.
3. **Unverified signature details** — Avatar's pixel-exact sizing API,
   `ActiveTheme` global registration. Not blocked on a missing feature;
   blocked on reading the exact method signature when that file's wave
   comes up, same as any other dependency bump.

The original risk ranking (Dialog rewrite, Tabs, resizable layout, Sidebar
as biggest) is retracted by this record — it was built on Fernrohr-App's
usage pattern standing in for gpui-kit's actual capability, exactly the
substitution this decision process was designed to catch.

## Consequences

- The migration is materially lower-risk and lower-effort than first
  scoped: no widget requires a from-scratch reimplementation, so the
  `migrate-to-gpui-kit` OpenSpec change's wave 4 ("no confirmed equivalent")
  collapses into wave 3 ("confirmed equivalent") — update `design.md` and
  `tasks.md` to drop the from-scratch-implementation tasks and merge the
  waves accordingly.
- Dependency hygiene improves: `gpui-kit` has real crates.io releases, so
  future upgrades are a version bump, not a commit-hash pin requiring manual
  changelog archaeology.
- The one remaining hand-rewrite item (Popover) is small enough to not
  change the wave sequencing on its own.
- This record should be revisited if a future `gpui-kit` minor version
  changes any of the APIs enumerated above — re-verify against source again
  rather than assuming continuity, per the lesson in the Context section.
