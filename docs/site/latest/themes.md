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

Three light, three dark:

| Theme | Mode | Character |
| --- | --- | --- |
| Parchment | Light | Warm tan/cream, the default |
| Slate | Light | Cool blue-gray |
| Sage | Light | Cool forest green |
| Ink | Dark | Warm brown-black |
| Moss | Dark | Cool forest green-black |
| Rosewood | Dark | Warm burgundy/wine |

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
error = "#B03028"
warning_bg = "hsla(0.11 0.9 0.5 0.12)"
warning_text = "hsla(0.08 0.85 0.35 1.0)"
```

Fields left out of a theme file fall back to the matching built-in theme's
value. User-defined theme files loaded from disk use the same format via
`theme_loader::load_from_path`.
