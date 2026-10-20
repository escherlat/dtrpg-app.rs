## Status

Accepted

## Context

Themes are a closed enum today. `data/theme.rs` is 718 lines and defines
`ThemeKey` with six variants: Parchment, Slate, Sage, Ink, Moss, and
Rosewood. The first four carry the `claude-1` design prototype's hex values
exactly. `LibriTheme::new` builds the actual color tokens from a `match`
over that enum, compiled into the binary. Adding, tweaking, or offering a
theme means editing Rust and recompiling; there is no path for a theme to
exist without being compiled into the app.

Goal 7 asks users to add their own themes. A closed enum cannot do that at
any size: every additional theme is a new variant, a new `match` arm, and a
rebuild, which is a fixed ceiling no amount of enum growth removes. This is
also the goal where the gap is explicit: the target design
(`dtrpg-design/claude-2`) shows a theme gallery with duplicate, edit, import,
and hot reload, none of which a compiled-in enum can support.

Separately, `Density::Comfortable`/`Compact` already exists in the theme
model but no settings page exposes it; it was a `claude-1` tweak-panel
concept that got modeled and then never surfaced. That control is expected
to resurface as part of goal 11's per-view presentation options, not as
part of this decision, but it is worth noting here since it lives in the
same file this decision restructures.

Semantic state colors (ok, warn, error, cloud) are part of the same color-
token structure as the themeable palette today. If themes become arbitrary
user-authored files, a theme author should not be able to make a failed
download look healthy by omitting or mis-defining a semantic color; those
meanings need to survive regardless of which theme file is loaded.

Alternative considered and rejected: keep widening the `ThemeKey` enum.
This was rejected because it cannot reach the actual goal (users adding
their own themes) at any enum size, and because every widening is itself a
Rust code change and release cycle, which is the exact cost this decision
exists to remove.

## Decision

Replace the closed `ThemeKey` enum with TOML theme files read from a themes
directory at startup. Each file declares one theme's display name and
palette fields. Hot reload on change: a theme file added, edited, or
removed while the app is running updates the available theme list and, if
the active theme's file changed, its live colors, without requiring a
restart.

Fall back per-key to a built-in light or dark base when a theme file is
missing a key, so an older theme file written against an earlier version
of the palette still loads rather than failing outright. The four palettes
carried forward from `claude-1`/`claude-2` ship as the app's default theme
files.

Semantic state colors (ok, warn, error, cloud) stay outside the themeable
set entirely, in a fixed structure no theme file can override, so a theme
cannot make a failed download look healthy or make an error state
indistinguishable from a success state.

## Consequences

- Adding or editing a theme becomes a file change, not a Rust code change
  and a release, which is what makes the goal 7 gallery (duplicate, edit,
  import) possible at all.
- Hot reload means the theme system needs a file watcher or an explicit
  reload trigger, and must handle a malformed or partially written theme
  file gracefully mid-session, a failure mode the closed enum, being
  compile-time checked, never had.
- The per-key fallback to a light or dark base is what keeps an older theme
  file from breaking outright as the palette's field set grows over time;
  it needs to be exercised by a test that adds a new palette field and
  confirms an old theme file still loads with a sane fallback, not just
  reasoned about.
- Semantic colors being fixed and non-themeable is a deliberate constraint
  on theme authors. It should be documented in the themes directory's own
  format description, not only in this ADR, so a future contributor does
  not make error color themeable and reintroduce the readability risk this
  decision exists to prevent.
- `Density::Comfortable`/`Compact` is not addressed by this decision; it
  remains modeled and unexposed until goal 11's per-view presentation work
  surfaces it.
- This is independent of 0002, 0003, and 0004: it touches `dtrpg-ui`'s
  rendering layer and `data/theme.rs`, not the service, index, or platform
  layers those move into `dtrpg-core`. It can land before, after, or
  interleaved with them. It is sequenced for `claude-2` compatibility, so
  see that prototype for the intended theme-gallery surface this decision
  supports.
