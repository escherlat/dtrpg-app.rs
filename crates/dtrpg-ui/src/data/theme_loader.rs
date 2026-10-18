//! Loads theme definitions from TOML files.
//!
//! This is the first step toward ADR-0005 (themes as data files). The
//! current implementation reads from `assets/themes/*.toml` at build time
//! via `include_str!` for built-in themes and provides a simple loader for
//! user themes. Behavior is backwards-compatible with compiled defaults.

use std::path::PathBuf;

use gpui::Hsla;
use serde::Deserialize;

use crate::data::theme::{ColorTokens, ThemeKey, hex};

#[derive(Debug, Deserialize)]
struct ThemeToml {
    name: String,
    #[expect(dead_code, reason = "reserved for future light/dark UI hints")]
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    palette: PaletteToml,
    #[serde(default)]
    colors: PaletteToml, // allow either "palette" or "colors"
}

#[derive(Debug, Default, Deserialize)]
struct PaletteToml {
    #[serde(default)]
    desktop_bg: Option<String>,
    #[serde(default)]
    surface: Option<String>,
    #[serde(default)]
    surface_alt: Option<String>,
    #[serde(default)]
    hover: Option<String>,
    #[serde(default)]
    text_primary: Option<String>,
    #[serde(default)]
    text_secondary: Option<String>,
    #[serde(default)]
    text_tertiary: Option<String>,
    #[serde(default)]
    border: Option<String>,
    #[serde(default)]
    border_strong: Option<String>,
    #[serde(default)]
    accent: Option<String>,
    #[serde(default)]
    accent_soft: Option<String>,
    #[serde(default)]
    accent_on: Option<String>,
    #[serde(default)]
    shadow: Option<String>,
    #[serde(default)]
    scrim: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    warning_bg: Option<String>,
    #[serde(default)]
    warning_text: Option<String>,
}

/// Loads a built-in theme by key from embedded TOML files.
/// Falls back to compiled defaults if not found.
pub fn load_builtin(key: ThemeKey) -> ColorTokens {
    let id = key.as_str();
    let toml = match id {
        "parchment" => include_str!("../../../../assets/themes/parchment.toml"),
        "slate" => include_str!("../../../../assets/themes/slate.toml"),
        "sage" => include_str!("../../../../assets/themes/sage.toml"),
        "ink" => include_str!("../../../../assets/themes/ink.toml"),
        "moss" => include_str!("../../../../assets/themes/moss.toml"),
        "rosewood" => include_str!("../../../../assets/themes/rosewood.toml"),
        _ => return compiled_default(key),
    };
    parse_theme_toml(toml).unwrap_or_else(|_| compiled_default(key))
}

/// Loads a theme from an arbitrary TOML file.
/// Returns None if parsing fails.
pub fn load_from_path(path: &PathBuf) -> Option<ColorTokens> {
    std::fs::read_to_string(path).ok().and_then(|s| parse_theme_toml(&s).ok())
}

fn parse_theme_toml(s: &str) -> Result<ColorTokens, toml::de::Error> {
    let theme: ThemeToml = toml::from_str(s)?;
    let p = theme.palette;
    let c = theme.colors;
    // prefer palette if set, else colors
    let palette = if has_any(&p) { p } else { c };
    let defaults = compiled_default_for_name(&theme.name);
    let tokens = ColorTokens {
        desktop_bg: parse_hex_opt(palette.desktop_bg.as_deref()).unwrap_or(defaults.desktop_bg),
        surface: parse_hex_opt(palette.surface.as_deref()).unwrap_or(defaults.surface),
        surface_alt: parse_hex_opt(palette.surface_alt.as_deref()).unwrap_or(defaults.surface_alt),
        hover: parse_hex_opt(palette.hover.as_deref()).unwrap_or(defaults.hover),
        text_primary: parse_hex_opt(palette.text_primary.as_deref()).unwrap_or(defaults.text_primary),
        text_secondary: parse_hex_opt(palette.text_secondary.as_deref()).unwrap_or(defaults.text_secondary),
        text_tertiary: parse_hex_opt(palette.text_tertiary.as_deref()).unwrap_or(defaults.text_tertiary),
        border: parse_hex_opt(palette.border.as_deref()).unwrap_or(defaults.border),
        border_strong: parse_hex_opt(palette.border_strong.as_deref()).unwrap_or(defaults.border_strong),
        accent: parse_hex_opt(palette.accent.as_deref()).unwrap_or(defaults.accent),
        accent_soft: parse_hex_alpha_opt(palette.accent_soft.as_deref()).unwrap_or(defaults.accent_soft),
        accent_on: parse_hex_opt(palette.accent_on.as_deref()).unwrap_or(defaults.accent_on),
        shadow: parse_hex_alpha_opt(palette.shadow.as_deref()).unwrap_or(defaults.shadow),
        scrim: parse_hex_alpha_opt(palette.scrim.as_deref()).unwrap_or(defaults.scrim),
        error: parse_hex_opt(palette.error.as_deref()).unwrap_or(defaults.error),
        warning_bg: parse_hsla_like(palette.warning_bg.as_deref()).unwrap_or(defaults.warning_bg),
        warning_text: parse_hsla_like(palette.warning_text.as_deref()).unwrap_or(defaults.warning_text),
    };
    Ok(tokens)
}

fn has_any(p: &PaletteToml) -> bool {
    p.desktop_bg.is_some() || p.surface.is_some() || p.surface_alt.is_some() || p.hover.is_some() ||
    p.text_primary.is_some() || p.text_secondary.is_some() || p.text_tertiary.is_some() ||
    p.border.is_some() || p.border_strong.is_some() || p.accent.is_some() ||
    p.accent_soft.is_some() || p.accent_on.is_some() || p.shadow.is_some() ||
    p.scrim.is_some() || p.error.is_some() || p.warning_bg.is_some() || p.warning_text.is_some()
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
    } else if s.len() == 3 {
        let r = u8::from_str_radix(&s[0..1], 16).ok()?;
        let g = u8::from_str_radix(&s[1..2], 16).ok()?;
        let b = u8::from_str_radix(&s[2..3], 16).ok()?;
        Some(hex(r*17, g*17, b*17))
    } else {
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

fn parse_hsla_like(s: Option<&str>) -> Option<Hsla> {
    let s = s?.trim();
    let s = s.strip_prefix("hsla(")?.strip_suffix(")")?;
    let parts: Vec<f32> = s.split_whitespace().filter_map(|p| p.parse().ok()).collect();
    if parts.len() == 4 {
        Some(gpui::hsla(parts[0], parts[1], parts[2], parts[3]))
    } else {
        None
    }
}

fn compiled_default_for_name(name: &str) -> ColorTokens {
    match name.to_lowercase().as_str() {
        "parchment" => crate::data::theme::parchment_colors(),
        "slate" => crate::data::theme::slate_colors(),
        "sage" => crate::data::theme::sage_colors(),
        "ink" => crate::data::theme::ink_colors(),
        "moss" => crate::data::theme::moss_colors(),
        "rosewood" => crate::data::theme::rosewood_colors(),
        _ => crate::data::theme::parchment_colors(),
    }
}

fn compiled_default(key: ThemeKey) -> ColorTokens {
    match key {
        ThemeKey::Parchment => crate::data::theme::parchment_colors(),
        ThemeKey::Slate => crate::data::theme::slate_colors(),
        ThemeKey::Sage => crate::data::theme::sage_colors(),
        ThemeKey::Ink => crate::data::theme::ink_colors(),
        ThemeKey::Moss => crate::data::theme::moss_colors(),
        ThemeKey::Rosewood => crate::data::theme::rosewood_colors(),
    }
}

#[cfg(test)]
mod tests {
    use crate::data::theme::ThemeKey;

    #[test]
    fn builtin_toml_matches_compiled_defaults() {
        for key in [
            ThemeKey::Parchment,
            ThemeKey::Slate,
            ThemeKey::Sage,
            ThemeKey::Ink,
            ThemeKey::Moss,
            ThemeKey::Rosewood,
        ] {
            let loaded = super::load_builtin(key);
            let compiled = super::compiled_default(key);
            let fields = [
                ("desktop_bg", loaded.desktop_bg, compiled.desktop_bg),
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
                ("warning_text", loaded.warning_text, compiled.warning_text),
            ];
            for (name, l, c) in fields {
                assert_eq!(l, c, "mismatch in {:?} field {}", key, name);
            }
        }
    }
}
