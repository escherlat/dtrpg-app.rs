## Context

See `docs/adr/0001-migrate-to-gpui-kit.md` for the full decision record,
including its round-3 amendment, which this file now follows. Summary: an
initial inventory of `gpui_component` usage (35 files / 175 usages across
`crates/dtrpg-ui` and `crates/dtrpg-core`) was cross-checked against
gpui-kit 0.7's own source and found a confirmed equivalent for nearly every
widget (one real reshape: Popover). That verification was still incomplete
— it confirmed *what exists* in gpui-kit but not *what crate those types
actually resolve to*. `gpui-component` 0.7.3 (pulled in via `gpui-kit`)
renames its own `gpui` dependency to `gpui-pre`, a Longbridge fork of
zed-industries/zed's `gpui` — a different crate from the one our app and
every existing view/controller use. Every gpui-kit `Entity`/`Global`/
`Render` implementation is expressed in terms of `gpui_pre::Context`/
`Window`/`App`, which cannot be bridged to our existing `gpui::Context`/
`Window`/`App` from local code (orphan rule: both the trait and the type
are foreign). There is no partial, file-local, or wave-local migration
state that compiles — see the ADR amendment for the compiler-proven detail.

## Decisions

- **Single all-or-nothing rewrite, not incremental coexistence.** `gpui-kit`
  replaces `gpui-component` (and `gpui`/`gpui_platform`, since the app's
  bootstrap and every render-tree signature move to gpui-kit's fork) in one
  unit of work. `dtrpg-core`'s `gpui_platform::application()` call and every
  `Context<Self>`/`&mut Window`/`App` parameter across every view and
  controller in `dtrpg-ui` and `dtrpg-core` switch together — nothing
  compiles at any intermediate point otherwise, so there is no "wave" to
  sequence.
- **Commit structure may still split by concern, not by compile-checkpoint.**
  A commit splitting mechanical import renames from the app-bootstrap swap
  is fine; the constraint is that the *tree* only needs to compile at the
  final commit of the unit, not after each one — unlike every other change
  in this repo, where every commit is independently buildable.
- **The widget-level mapping from round 2 is still the reference for what to
  port each type to.** Nearly every `gpui_component` type has a same-named
  or near-identical counterpart in `gpui_kit::component`/`gpui_kit::base`;
  Popover is the one confirmed builder reshape (`.content(...)` →
  `.anchor(...).open(...).on_open_change(...)`). What changed is not *what*
  to port to, but that the porting has to happen everywhere at once.
- **Value-type conversions are mechanical but real.** `gpui::Hsla` and
  `gpui_pre::Hsla` (reached via `gpui_kit::Hsla`) are field-identical
  (`h`/`s`/`l`/`a: f32`) but distinct types — no blanket `.into()` exists
  across the orphan-rule boundary, so a small local conversion helper is
  needed wherever a color value crosses from app code into a gpui-kit
  widget. Same pattern for `SharedString` (`gpui_pre::SharedString::from
  (our_str.as_ref())`, since `From<&str>` exists on the gpui-pre side).
  These are per-value-type helpers written once and reused, not something
  to solve per call site.
- **`gpui-component`/`gpui-component-assets` are removed in the same change**
  that introduces `gpui-kit`'s replacements — there's no longer a
  "coexistence window" during which both stay in `Cargo.toml`.

## Migration Plan

One unit of work, landing as one commit or a tightly-scoped handful of
commits that only make sense together (e.g. mechanical renames split from
the bootstrap swap) — `cargo check --workspace --all-targets`, `cargo
clippy --workspace --all-targets -- -D warnings`, `cargo +nightly fmt
--check`, and `cargo test --workspace` only need to pass at the end of the
unit, not after each commit within it. See `tasks.md` for the task
breakdown (collapsed from four sequenced waves into sections of one body of
work, per the ADR amendment).
