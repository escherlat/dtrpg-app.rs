# Theme files

Each `*.toml` file here is a built-in color theme compiled into the app via
`include_str!` (`crates/dtrpg-ui/src/data/theme_loader.rs`). The same format
applies to user-authored themes placed in the runtime themes directory (see
"User themes" in `docs/site/latest/themes.md`).

## Format

Fields sit flat at the file root - there is no `[palette]` table:

```toml
name = "Slate"   # display name; also selects a built-in fallback base if recognized
mode = "light"   # "light" or "dark"; defaults to "light"

desktop_bg = "#1A1A1A"
surface = "#242424"
surface_alt = "#2E2E2E"
hover = "#333333"
text_primary = "#F2F2F2"
text_secondary = "#A8A8A8"
text_tertiary = "#777777"
border = "#3A3A3A"
border_strong = "#4E4E4E"
accent = "#5A5A5A"
accent_soft = "#8A8A8A"
accent_on = "#FFFFFF"
shadow = "#000000 0.5"   # "#RRGGBB <alpha>"
scrim = "#000000 0.5"

# Semantic state colors are intentionally absent (see below).
```

Every palette key is optional. A missing key falls back per-key to a
matching built-in palette: a recognized `name` uses that built-in's colors;
otherwise `mode` selects the light (Parchment) or dark (Ink) base.

## Semantic colors are not themeable

`error`, `warning_bg`, and `warning_text` are fixed by `mode` at compile time
and are never read from a theme file. This is deliberate (ADR-0005): a theme
cannot make a failed download look healthy or hide a warning, so all themes
keep the same semantic state meaning.

`mode` is what themes control here - it picks which fixed set (light or dark)
applies.
