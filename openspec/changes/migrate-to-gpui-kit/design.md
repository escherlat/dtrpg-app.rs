## Context

See `docs/adr/0001-migrate-to-gpui-kit.md` for the full decision record.
Summary: an initial inventory of `gpui_component` usage (35 files / 175
usages across `crates/dtrpg-ui` and `crates/dtrpg-core`) was cross-checked
against gpui-kit 0.7's *own source* (not just Fernrohr-App's call sites,
which the first-pass research had relied on and which understated gpui-kit's
actual catalog). That direct check found a confirmed gpui-kit equivalent for
every widget in the inventory except one real API reshape (Popover). There
is no "no equivalent" bucket in this plan.

## Decisions

- **Coexistence, not a cutover.** `gpui-kit` and `gpui-component` are added
  as parallel dependencies for the duration of the migration. A single PR
  swapping 35 files/175 usages is an unreviewable, unmergeable unit; waves
  gated by subdirectory let each PR be independently tested and reverted.
- **Sequencing is dependency-direction, not file-count.** Each wave only
  touches files whose `gpui_component` usages have a confirmed gpui-kit
  equivalent, deferring everything blocked on an unresolved design question.
  Concretely, in dependency order:
  1. `util/`, `data/` — table column config (`Column`/`ColumnSort`) and the
     `Theme`/`ActiveTheme` global; no interactive widgets, lowest blast
     radius, and other waves assume these already resolve to gpui-kit types.
  2. `controllers/`, `controllers/library/` — orchestration code holding
     `TableState`/`TableEvent`/`InputState`/`SelectState` entities. Migrating
     the state types here first means the view-layer waves receive
     already-migrated entities rather than migrating state and view
     simultaneously.
  3. `ui/views/`, `ui/app/`, `ui/library/` — every remaining widget, since
     round-2 source verification (see ADR 0001) found a confirmed gpui-kit
     equivalent for the full inventory except Popover's builder reshape.
     `ui/library/` moves in this same wave rather than last, since there's
     no longer a straddle to avoid: nothing downstream is deferred to a
     later wave waiting on a design decision.
  `ui/` itself (no files directly at that level beyond the above
  subdirectories) carries no separate wave.
- **Only one widget needs a hand rewrite rather than an import swap:
  Popover.** Old: `.content(...)`. gpui-kit: `.anchor(Anchor::TopRight)
  .open(...).on_open_change(...)`. Confirmed from the inventory's own
  call-site reading of Fernrohr-App's `namespace_picker.rs`; every other
  reshape claim from the first-pass inventory (Dialog's construction
  pattern, Button's `.ghost()`/`.danger()` shortcuts) did not survive
  direct source verification — see ADR 0001's Context section for what was
  wrong and why.
- **No from-scratch implementations.** The first-pass inventory flagged
  Sidebar, Tabs, resizable split panes, Avatar, Clipboard, and a dozen
  others as having no gpui-kit equivalent, built on Fernrohr-App's call
  sites rather than gpui-kit's own source. Direct source verification
  (`crates/component/src/{avatar,sidebar,tab,...}`,
  `crates/base/src/resizable/`) found every one of them. Every task in
  `tasks.md` is a port (swap the import, adjust the one or two signatures
  that differ), not new code.

## Migration Plan

Three waves, each its own PR(s), each independently buildable and testable
(`cargo check --workspace --all-targets`, `cargo clippy ... -D warnings`,
`cargo test --workspace` must pass at the end of every wave, not just the
final one):

1. `util/`, `data/`
2. `controllers/`, `controllers/library/`
3. `ui/views/`, `ui/app/`, `ui/library/`

`gpui-component`/`gpui-component-assets` are removed from every `Cargo.toml`
only after wave 3, gated on `rg -l "gpui_component" crates/` returning empty.
