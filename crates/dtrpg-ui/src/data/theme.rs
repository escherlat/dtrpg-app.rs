//! Libri theme system: six color themes, two density variants, four
//! user-selectable font-family roles, and one shared UI text size.

use dtrpg_core::data::constants::{
    DEFAULT_BODY_FONT, DEFAULT_LABEL_FONT, DEFAULT_MONO_FONT, DEFAULT_UI_TEXT_SIZE,
    DEFAULT_VALUE_FONT,
};
use gpui::{Hsla, Pixels, SharedString, px}; /* Pixels kept for GPUI layout
                                             * fields */

use crate::data::theme_catalog::ThemeId;

// ── Color tokens
// ──────────────────────────────────────────────────────────────

/// Semantic color tokens for one Libri theme.
#[derive(Debug, Clone)]
pub struct ColorTokens {
    /// App desktop background color.
    pub desktop_bg:     Hsla,
    /// Main window / panel background.
    pub surface:        Hsla,
    /// Sidebar / secondary surface background.
    pub surface_alt:    Hsla,
    /// Hover state background.
    pub hover:          Hsla,
    /// Primary text.
    pub text_primary:   Hsla,
    /// Secondary / dimmed text.
    pub text_secondary: Hsla,
    /// Tertiary / placeholder text.
    pub text_tertiary:  Hsla,
    /// Default border / divider.
    pub border:         Hsla,
    /// Stronger border for inputs.
    pub border_strong:  Hsla,
    /// Accent (active nav, focus rings).
    pub accent:         Hsla,
    /// Accent at low opacity for backgrounds.
    pub accent_soft:    Hsla,
    /// Text color drawn on top of an accent background.
    pub accent_on:      Hsla,
    /// Drop shadow color.
    pub shadow:         Hsla,
    /// Overlay scrim.
    pub scrim:          Hsla,
    /// Error / destructive state (red). Fixed by mode, never read from a
    /// theme file (see [`semantic_colors`]).
    pub error:          Hsla,
    /// Warning banner background (amber, low opacity). Fixed by mode.
    pub warning_bg:     Hsla,
    /// Warning banner text / icon color (amber, full opacity). Fixed by mode.
    pub warning_text:   Hsla,
}

// ── Semantic state colors
// ────────────────────────────────────────────────────────

/// Light or dark variant a theme declares. Used to select the fixed semantic
/// state colors and the fallback base for a partially-specified theme file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    /// The canonical lowercase spelling used in theme files.
    pub fn as_str(&self) -> &'static str {
        match self {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
}

impl std::str::FromStr for ThemeMode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "light" => Ok(ThemeMode::Light),
            "dark" => Ok(ThemeMode::Dark),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Semantic state colors (error / warning) that no theme file may override,
/// so a theme can never make a failed download look healthy or an error
/// indistinguishable from a success state (ADR-0005).
#[derive(Debug, Clone, Copy)]
pub struct SemanticColors {
    /// Error / destructive state (red).
    pub error:        Hsla,
    /// Warning banner background (amber, low opacity).
    pub warning_bg:   Hsla,
    /// Warning banner text / icon color (amber, full opacity).
    pub warning_text: Hsla,
}

/// Returns the fixed semantic colors for a theme's [`ThemeMode`].
///
/// These are compile-time constants chosen by light/dark mode, deliberately
/// outside the themeable palette: a theme file cannot set them.
pub fn semantic_colors(mode: ThemeMode) -> SemanticColors {
    match mode {
        ThemeMode::Light => SemanticColors { error:        hex(0xB0, 0x30, 0x28),
                                             warning_bg:   gpui::hsla(0.11, 0.9, 0.5, 0.12),
                                             warning_text: gpui::hsla(0.08, 0.85, 0.35, 1.0), },
        ThemeMode::Dark => SemanticColors { error:        hex(0xE0, 0x58, 0x58),
                                            warning_bg:   gpui::hsla(0.11, 0.9, 0.5, 0.10),
                                            warning_text: gpui::hsla(0.10, 0.90, 0.65, 1.0), },
    }
}

// ── Density
// ───────────────────────────────────────────────────────────────────

/// Spacing density variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

/// Layout constants that vary with density.
#[derive(Debug, Clone)]
pub struct DensityConstants {
    /// Height of a text-list row.
    pub row_text_height:    Pixels,
    /// Height of a thumbs-list row.
    pub thumb_row_height:   Pixels,
    /// Width of a thumbnail in a thumbs-list row (plain f32 for arithmetic).
    pub thumb_width:        f32,
    /// Minimum width of a grid card (plain f32 for arithmetic).
    pub card_min_width:     f32,
    /// Horizontal gap between grid cards.
    pub card_gap_x:         Pixels,
    /// Vertical gap between grid cards.
    pub card_gap_y:         Pixels,
    /// Catalog area padding (top/side/bottom).
    pub catalog_pad_top:    Pixels,
    pub catalog_pad_side:   Pixels,
    pub catalog_pad_bottom: Pixels,
}

impl DensityConstants {
    fn comfortable() -> Self {
        Self { row_text_height:    px(44.0),
               thumb_row_height:   px(90.0),
               thumb_width:        60.0,
               card_min_width:     158.0,
               card_gap_x:         px(22.0),
               card_gap_y:         px(26.0),
               catalog_pad_top:    px(18.0),
               catalog_pad_side:   px(22.0),
               catalog_pad_bottom: px(48.0), }
    }

    fn compact() -> Self {
        Self { row_text_height:    px(33.0),
               thumb_row_height:   px(76.0),
               thumb_width:        50.0,
               card_min_width:     132.0,
               card_gap_x:         px(16.0),
               card_gap_y:         px(18.0),
               catalog_pad_top:    px(12.0),
               catalog_pad_side:   px(20.0),
               catalog_pad_bottom: px(40.0), }
    }
}

// ── LibriTheme
// ────────────────────────────────────────────────────────────────

/// Font-family and size selections for a [`LibriTheme`], grouped into one
/// struct rather than threaded as separate constructor arguments (see
/// `docs/rust.md`'s guidance against many-argument functions).
///
/// Family choices are free-form: any font name the user's system reports via
/// `cx.text_system().all_font_names()` is valid, not a curated list — see
/// `settings_appearance_view`. `ui_text_size` drives the window's rem size,
/// so every role scales together as the Appearance page's "Text Scale"
/// control changes it.
#[derive(Debug, Clone)]
pub struct FontSelections {
    /// Active body-font family; also applied to `gpui_kit::component::Theme`'s
    /// `font_family` by whichever call site changes it (see
    /// `LibraryController::set_body_font`).
    pub body_font:    SharedString,
    /// Active value-font family (e.g. Advanced settings' "Cache details"
    /// rows).
    pub value_font:   SharedString,
    /// Active label-font family (e.g. the detail tab's metadata labels).
    pub label_font:   SharedString,
    /// Active monospace-font family (e.g. the masked API key hint).
    pub mono_font:    SharedString,
    /// Shared UI text size, applied via `Window::set_rem_size` so every
    /// `rems(...)`-based size utility (`.text_sm()`, `.text_xs()`, etc.)
    /// scales together, like zooming a page.
    pub ui_text_size: Pixels,
}

impl Default for FontSelections {
    fn default() -> Self {
        Self { body_font:    SharedString::from(DEFAULT_BODY_FONT),
               value_font:   SharedString::from(DEFAULT_VALUE_FONT),
               label_font:   SharedString::from(DEFAULT_LABEL_FONT),
               mono_font:    SharedString::from(DEFAULT_MONO_FONT),
               ui_text_size: px(DEFAULT_UI_TEXT_SIZE), }
    }
}

/// GPUI app-level global containing the active Libri theme, density, and
/// font selections.
#[derive(Debug, Clone)]
pub struct LibriTheme {
    /// Stable id of the active theme (see [`ThemeId`]).
    pub id:                ThemeId,
    pub colors:            ColorTokens,
    pub density:           Density,
    pub density_constants: DensityConstants,
    pub fonts:             FontSelections,
}

impl gpui::Global for LibriTheme {}

impl LibriTheme {
    /// Constructs the theme for `id`, `density`, and `fonts`.
    pub fn new(id: ThemeId, density: Density, fonts: FontSelections) -> Self {
        let colors = crate::data::theme_loader::load(&id);
        let density_constants = match density {
            Density::Comfortable => DensityConstants::comfortable(),
            Density::Compact => DensityConstants::compact(),
        };
        Self { id,
               colors,
               density,
               density_constants,
               fonts }
    }

    /// Returns the default theme (parchment, comfortable, default fonts).
    pub fn default_theme() -> Self {
        Self::new(ThemeId::default(),
                  Density::Comfortable,
                  FontSelections::default())
    }
}

#[path = "palette.rs"]
mod palette;
#[path = "theme_sync.rs"]
mod theme_sync;

pub(crate) use palette::{
    hex, ink_colors, moss_colors, parchment_colors, rosewood_colors, sage_colors, slate_colors,
};
pub use theme_sync::{apply_theme_colors, install_theme};
