//! Loads theme definitions from TOML files.
//!
//! This is the first step toward ADR-0005 (themes as data files). The
//! current implementation reads from `assets/themes/*.toml` at build time
//! via `include_str!` for built-in themes and provides a simple loader for
//! user themes. Behavior is backwards-compatible with compiled defaults.

use std::path::PathBuf;

use gpui::Hsla;
use serde::Deserialize;

use crate::data::theme::{ColorTokens, ThemeMode, hex, semantic_colors};
use crate::data::theme_catalog::ThemeId;

#[derive(Debug, Deserialize)]
struct ThemeToml {
    name:    String,
    /// Light or dark declaration; drives fixed semantic colors and the
    /// fallback base for missing palette keys. Defaults to light.
    #[serde(default)]
    mode:    Option<String>,
    /// Palette fields live flat alongside `name`/`mode` in the file; flatten
    /// collects them so a theme file needs no nested table.
    #[serde(flatten)]
    palette: PaletteToml,
}

#[derive(Debug, Default, Deserialize)]
struct PaletteToml {
    #[serde(default)]
    desktop_bg:     Option<String>,
    #[serde(default)]
    surface:        Option<String>,
    #[serde(default)]
    surface_alt:    Option<String>,
    #[serde(default)]
    hover:          Option<String>,
    #[serde(default)]
    text_primary:   Option<String>,
    #[serde(default)]
    text_secondary: Option<String>,
    #[serde(default)]
    text_tertiary:  Option<String>,
    #[serde(default)]
    border:         Option<String>,
    #[serde(default)]
    border_strong:  Option<String>,
    #[serde(default)]
    accent:         Option<String>,
    #[serde(default)]
    accent_soft:    Option<String>,
    #[serde(default)]
    accent_on:      Option<String>,
    #[serde(default)]
    shadow:         Option<String>,
    #[serde(default)]
    scrim:          Option<String>,
}

/// Loads a theme's colors by id: a built-in id reads its embedded TOML, any
/// other id reads `<themes_dir>/<id>.toml` (a user-authored theme, ADR-0005).
///
/// An unknown, unreadable, or malformed id falls back to the compiled default
/// (parchment), so a stale persisted selection never fails to produce a usable
/// palette.
pub fn load(id: &ThemeId) -> ColorTokens {
    let toml = match id.as_str() {
        "parchment" => include_str!("../../../../assets/themes/parchment.toml"),
        "slate" => include_str!("../../../../assets/themes/slate.toml"),
        "sage" => include_str!("../../../../assets/themes/sage.toml"),
        "ink" => include_str!("../../../../assets/themes/ink.toml"),
        "moss" => include_str!("../../../../assets/themes/moss.toml"),
        "rosewood" => include_str!("../../../../assets/themes/rosewood.toml"),
        other => return load_user_theme(other).unwrap_or_else(|| compiled_default(id)),
    };
    parse_theme_toml(toml).unwrap_or_else(|_| compiled_default(id))
}

/// Reads a user theme file by id from the themes directory.
///
/// Returns `None` for an id containing path separators or `..` (a persisted
/// id is never trusted as a filesystem path) or for an unreadable/invalid
/// file.
fn load_user_theme(id: &str) -> Option<ColorTokens> {
    if id.contains(['/', '\\']) || id.is_empty() || id.contains("..") {
        return None;
    }
    let path = dtrpg_core::data::paths::themes_dir().join(format!("{id}.toml"));
    let content = std::fs::read_to_string(path).ok()?;
    parse_theme_toml(&content).ok()
}

/// Loads a theme from an arbitrary TOML file.
/// Returns None if parsing fails.
pub fn load_from_path(path: &PathBuf) -> Option<ColorTokens> {
    std::fs::read_to_string(path).ok()
                                 .and_then(|s| parse_theme_toml(&s).ok())
}

fn parse_theme_toml(s: &str) -> Result<ColorTokens, toml::de::Error> {
    let theme: ThemeToml = toml::from_str(s)?;
    let palette = theme.palette;
    let mode = theme.mode
                    .as_deref()
                    .and_then(|m| m.parse().ok())
                    .unwrap_or(ThemeMode::Light);
    let semantic = semantic_colors(mode);
    let defaults = fallback_base(&theme.name, mode);
    let tokens = ColorTokens {
        desktop_bg: parse_hex_opt(palette.desktop_bg.as_deref()).unwrap_or(defaults.desktop_bg),
        surface: parse_hex_opt(palette.surface.as_deref()).unwrap_or(defaults.surface),
        surface_alt: parse_hex_opt(palette.surface_alt.as_deref()).unwrap_or(defaults.surface_alt),
        hover: parse_hex_opt(palette.hover.as_deref()).unwrap_or(defaults.hover),
        text_primary: parse_hex_opt(palette.text_primary.as_deref())
            .unwrap_or(defaults.text_primary),
        text_secondary: parse_hex_opt(palette.text_secondary.as_deref())
            .unwrap_or(defaults.text_secondary),
        text_tertiary: parse_hex_opt(palette.text_tertiary.as_deref())
            .unwrap_or(defaults.text_tertiary),
        border: parse_hex_opt(palette.border.as_deref()).unwrap_or(defaults.border),
        border_strong: parse_hex_opt(palette.border_strong.as_deref())
            .unwrap_or(defaults.border_strong),
        accent: parse_hex_opt(palette.accent.as_deref()).unwrap_or(defaults.accent),
        accent_soft: parse_hex_alpha_opt(palette.accent_soft.as_deref())
            .unwrap_or(defaults.accent_soft),
        accent_on: parse_hex_opt(palette.accent_on.as_deref()).unwrap_or(defaults.accent_on),
        shadow: parse_hex_alpha_opt(palette.shadow.as_deref()).unwrap_or(defaults.shadow),
        scrim: parse_hex_alpha_opt(palette.scrim.as_deref()).unwrap_or(defaults.scrim),
        // Semantic state colors are fixed by mode and never read from the
        // theme file, so a theme cannot make an error look healthy (ADR-0005).
        error: semantic.error,
        warning_bg: semantic.warning_bg,
        warning_text: semantic.warning_text,
    };
    Ok(tokens)
}

fn parse_hex_opt(s: Option<&str>) -> Option<Hsla> {
    s.and_then(parse_hex)
}

fn parse_hex_alpha_opt(s: Option<&str>) -> Option<Hsla> {
    s.and_then(parse_hex_alpha)
}

fn parse_hex(s: &str) -> Option<Hsla> {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(hex(r, g, b))
    }
    else if s.len() == 3 {
        let r = u8::from_str_radix(&s[0..1], 16).ok()?;
        let g = u8::from_str_radix(&s[1..2], 16).ok()?;
        let b = u8::from_str_radix(&s[2..3], 16).ok()?;
        Some(hex(r * 17, g * 17, b * 17))
    }
    else {
        None
    }
}

fn parse_hex_alpha(s: &str) -> Option<Hsla> {
    let s = s.trim();
    // "#RRGGBB a.f32" format: hex then space then alpha
    let parts: Vec<&str> = s.split_whitespace().collect();
    let hex_part = parts.first().copied().unwrap_or("");
    let alpha = parts.get(1).and_then(|a| a.parse::<f32>().ok());
    let base = parse_hex(hex_part)?;
    match alpha {
        Some(a) => Some(Hsla { a, ..base }),
        None => Some(base),
    }
}

/// Per-key fallback base for a theme file: a known built-in name resolves to
/// that built-in's compiled palette; an unrecognized name falls back to a
/// light or dark built-in base matching the declared mode, so a partially
/// specified custom theme still loads with sane values (ADR-0005).
fn fallback_base(name: &str, mode: ThemeMode) -> ColorTokens {
    match name.to_lowercase().as_str() {
        "parchment" => crate::data::theme::parchment_colors(),
        "slate" => crate::data::theme::slate_colors(),
        "sage" => crate::data::theme::sage_colors(),
        "ink" => crate::data::theme::ink_colors(),
        "moss" => crate::data::theme::moss_colors(),
        "rosewood" => crate::data::theme::rosewood_colors(),
        _ => match mode {
            ThemeMode::Light => crate::data::theme::parchment_colors(),
            ThemeMode::Dark => crate::data::theme::ink_colors(),
        },
    }
}

fn compiled_default(id: &ThemeId) -> ColorTokens {
    match id.as_str() {
        "parchment" => crate::data::theme::parchment_colors(),
        "slate" => crate::data::theme::slate_colors(),
        "sage" => crate::data::theme::sage_colors(),
        "ink" => crate::data::theme::ink_colors(),
        "moss" => crate::data::theme::moss_colors(),
        "rosewood" => crate::data::theme::rosewood_colors(),
        _ => crate::data::theme::parchment_colors(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures use unwrap/expect for concise assertions.

    use crate::data::theme_catalog::ThemeId;

    #[test]
    fn custom_theme_reads_flat_palette_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom.toml");
        std::fs::write(
            &path,
            "name = \"Custom\"\nmode = \"dark\"\nsurface = \"#101010\"\naccent = \"#FF00FF\"\n",
        )
        .unwrap();
        let colors = super::load_from_path(&path).expect("custom theme should load");
        // The distinct surface/accent must be parsed from the root keys, not
        // fall back to the compiled base.
        assert_eq!(colors.surface, crate::data::theme::hex(0x10, 0x10, 0x10));
        assert_eq!(colors.accent, crate::data::theme::hex(0xFF, 0x00, 0xFF));
        // Missing keys fall back to the dark base (ink).
        let ink = crate::data::theme::ink_colors();
        assert_eq!(colors.text_primary, ink.text_primary);
    }

    #[test]
    fn semantic_colors_cannot_be_overridden_by_theme_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evil.toml");
        // A theme file that tries to make an error look healthy.
        std::fs::write(
            &path,
            "name = \"Parchment\"\nmode = \"light\"\nerror = \"#00FF00\"\nwarning_text = \"#00FF00\"\n",
        )
        .unwrap();
        let colors = super::load_from_path(&path).expect("theme should load");
        let light = crate::data::theme::semantic_colors(crate::data::theme::ThemeMode::Light);
        assert_eq!(colors.error, light.error, "error color must stay semantic");
        assert_eq!(colors.warning_text, light.warning_text);
        assert_ne!(colors.error, crate::data::theme::hex(0x00, 0xFF, 0x00));
    }

    #[test]
    fn partial_unknown_theme_falls_back_to_mode_base() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sparse.toml");
        std::fs::write(&path, "name = \"Sparse\"\nmode = \"dark\"\n").unwrap();
        let colors = super::load_from_path(&path).expect("sparse theme should load");
        let ink = crate::data::theme::ink_colors();
        // Every missing key falls back to the dark base, and semantic colors
        // are the fixed dark set.
        assert_eq!(colors.surface, ink.surface);
        let dark = crate::data::theme::semantic_colors(crate::data::theme::ThemeMode::Dark);
        assert_eq!(colors.error, dark.error);
    }

    #[test]
    fn unknown_user_id_falls_back_to_parchment() {
        let loaded = super::load(&ThemeId::from("no-such-theme"));
        assert_eq!(loaded.surface,
                   crate::data::theme::parchment_colors().surface);
    }

    #[test]
    fn traversal_ids_never_reach_the_filesystem() {
        // A crafted persisted id must not escape the themes directory; both
        // resolve to the parchment fallback rather than reading arbitrary
        // files.
        for id in ["../secrets", "..", "a/b", "a\\b", ""] {
            assert!(super::load_user_theme(id).is_none(),
                    "id {id:?} must be rejected");
        }
    }

    #[test]
    fn builtin_toml_matches_compiled_defaults() {
        for id in ["parchment", "slate", "sage", "ink", "moss", "rosewood"] {
            let key = ThemeId::from(id);
            let loaded = super::load(&key);
            let compiled = super::compiled_default(&key);
            let fields = [("desktop_bg", loaded.desktop_bg, compiled.desktop_bg),
                          ("surface", loaded.surface, compiled.surface),
                          ("surface_alt", loaded.surface_alt, compiled.surface_alt),
                          ("hover", loaded.hover, compiled.hover),
                          ("text_primary", loaded.text_primary, compiled.text_primary),
                          ("text_secondary", loaded.text_secondary, compiled.text_secondary),
                          ("text_tertiary", loaded.text_tertiary, compiled.text_tertiary),
                          ("border", loaded.border, compiled.border),
                          ("border_strong", loaded.border_strong, compiled.border_strong),
                          ("accent", loaded.accent, compiled.accent),
                          ("accent_soft", loaded.accent_soft, compiled.accent_soft),
                          ("accent_on", loaded.accent_on, compiled.accent_on),
                          ("shadow", loaded.shadow, compiled.shadow),
                          ("scrim", loaded.scrim, compiled.scrim),
                          ("error", loaded.error, compiled.error),
                          ("warning_bg", loaded.warning_bg, compiled.warning_bg),
                          ("warning_text", loaded.warning_text, compiled.warning_text)];
            for (name, l, c) in fields {
                assert_eq!(l, c, "mismatch in {:?} field {}", key, name);
            }
        }
    }
}
