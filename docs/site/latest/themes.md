---
title: Themes and appearance
version: latest
slug: themes
---

# Themes and appearance

Libri ships six built-in color themes, two density variants, and four
independently selectable font roles, all configurable from Settings >
Appearance.

## Color themes

Four light, two dark:

| Theme | Mode | Character |
| --- | --- | --- |
| Parchment | Light | Warm tan/cream, the default |
| Slate | Light | Cool blue-gray |
| Sage | Light | Cool forest green |
| Ink | Dark | Warm brown-black |
| Moss | Dark | Cool forest green-black |
| Rosewood | Light | Warm burgundy/wine |

## Density

- **Comfortable** (default) - taller rows and wider padding.
- **Compact** - tighter spacing throughout the catalog, detail, and settings
  views.

## Fonts

Four roles, each independently selectable from any font installed on your
system:

- **Body** - the catalog and detail view's primary text.
- **Value** - data values (e.g. the Advanced settings page's cache details),
  rendered slightly smaller than body text by default.
- **Label** - field and row labels, sans-serif by default.
- **Monospace** - fixed-width content such as the masked API key hint.

A separate "Text Scale" control adjusts the shared UI text size, scaling
every size-aware element in the window together, like zooming a page.

## Theme data files

Each built-in theme ships as a TOML file under `assets/themes/` in the
repository. The app loads these at startup and falls back to compiled
defaults if a file is missing or malformed. The format is a flat palette of
hex colors, with optional alpha appended after a space:

```toml
name = "Parchment"
mode = "light"

desktop_bg = "#C5B99D"
surface = "#FCF9F3"
surface_alt = "#F2ECDF"
hover = "#EDE6D6"
text_primary = "#26211A"
text_secondary = "#5B5346"
text_tertiary = "#8C8270"
border = "#E7DFCD"
border_strong = "#DBD1BB"
accent = "#8C4A22"
accent_soft = "#8C4A22 0.13"
accent_on = "#FCF9F3"
shadow = "#3A2E1A 0.18"
scrim = "#1E160A 0.26"
```

`mode` is `light` or `dark`; it selects the fixed semantic state colors and
the fallback base for any palette field the file omits. Fields left out fall
back to a built-in base of the same mode, so a partially specified theme file
still loads.

### User themes

Drop your own `*.toml` theme files into the app's themes directory (created
on first launch):

- macOS: `~/Library/Application Support/com.pilgrimagesoftware.dtrpg/themes/`
- Linux: `~/.local/share/dtrpg/themes/`

Each file's stem becomes the theme's id (used for persistence), and its
`name` field is what Settings > Appearance and the status-bar theme picker
show. The same flat palette format applies; built-in ids are reserved, so a
file named `parchment.toml` is ignored in favor of the built-in. The
directory is read at startup and the pickers list whatever is there at that
point - restart the app after adding, renaming, or removing files.

A malformed file (no `name`, unparseable TOML) is skipped; a theme that loads
but omits palette fields falls back per-key as described above.

### Semantic state colors are not themeable

The error and warning colors are deliberately **not** part of the themeable
palette. They are fixed by `mode` at compile time so a theme file cannot make
a failed download look healthy or an error state indistinguishable from a
success state. Any `error`, `warning_bg`, or `warning_text` keys in a theme
file are ignored.
