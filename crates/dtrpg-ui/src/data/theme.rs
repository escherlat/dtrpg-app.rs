//! Libri theme system: six color themes, two density variants, four
//! user-selectable font-family roles, and one shared UI text size.

use gpui::{Hsla, Pixels, SharedString, px}; // Pixels kept for GPUI layout fields

use crate::data::constants::{
    DEFAULT_BODY_FONT, DEFAULT_LABEL_FONT, DEFAULT_MONO_FONT, DEFAULT_UI_TEXT_SIZE,
    DEFAULT_VALUE_FONT,
};

// ── Color tokens
// ──────────────────────────────────────────────────────────────

/// Semantic color tokens for one Libri theme.
#[derive(Debug, Clone)]
pub struct ColorTokens {
    /// App desktop background color.
    pub desktop_bg: Hsla,
    /// Main window / panel background.
    pub surface: Hsla,
    /// Sidebar / secondary surface background.
    pub surface_alt: Hsla,
    /// Hover state background.
    pub hover: Hsla,
    /// Primary text.
    pub text_primary: Hsla,
    /// Secondary / dimmed text.
    pub text_secondary: Hsla,
    /// Tertiary / placeholder text.
    pub text_tertiary: Hsla,
    /// Default border / divider.
    pub border: Hsla,
    /// Stronger border for inputs.
    pub border_strong: Hsla,
    /// Accent (active nav, focus rings).
    pub accent: Hsla,
    /// Accent at low opacity for backgrounds.
    pub accent_soft: Hsla,
    /// Text color drawn on top of an accent background.
    pub accent_on: Hsla,
    /// Drop shadow color.
    pub shadow: Hsla,
    /// Overlay scrim.
    pub scrim: Hsla,
    /// Error / destructive state (red).
    pub error: Hsla,
    /// Warning banner background (amber, low opacity).
    pub warning_bg: Hsla,
    /// Warning banner text / icon color (amber, full opacity).
    pub warning_text: Hsla,
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
    pub row_text_height: Pixels,
    /// Height of a thumbs-list row.
    pub thumb_row_height: Pixels,
    /// Width of a thumbnail in a thumbs-list row (plain f32 for arithmetic).
    pub thumb_width: f32,
    /// Minimum width of a grid card (plain f32 for arithmetic).
    pub card_min_width: f32,
    /// Horizontal gap between grid cards.
    pub card_gap_x: Pixels,
    /// Vertical gap between grid cards.
    pub card_gap_y: Pixels,
    /// Catalog area padding (top/side/bottom).
    pub catalog_pad_top: Pixels,
    pub catalog_pad_side: Pixels,
    pub catalog_pad_bottom: Pixels,
}

impl DensityConstants {
    fn comfortable() -> Self {
        Self {
            row_text_height: px(44.0),
            thumb_row_height: px(90.0),
            thumb_width: 60.0,
            card_min_width: 158.0,
            card_gap_x: px(22.0),
            card_gap_y: px(26.0),
            catalog_pad_top: px(18.0),
            catalog_pad_side: px(22.0),
            catalog_pad_bottom: px(48.0),
        }
    }

    fn compact() -> Self {
        Self {
            row_text_height: px(33.0),
            thumb_row_height: px(76.0),
            thumb_width: 50.0,
            card_min_width: 132.0,
            card_gap_x: px(16.0),
            card_gap_y: px(18.0),
            catalog_pad_top: px(12.0),
            catalog_pad_side: px(20.0),
            catalog_pad_bottom: px(40.0),
        }
    }
}

// ── Theme key
// ─────────────────────────────────────────────────────────────────

/// Identifies one of the six named themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeKey {
    #[default]
    Parchment,
    Slate,
    Sage,
    Ink,
    Moss,
    Rosewood,
}

impl ThemeKey {
    /// Stable, lowercase identifier used for persistence — independent of the
    /// enum variant name so a future rename doesn't silently invalidate saved
    /// preferences.
    pub fn as_str(self) -> &'static str {
        match self {
            ThemeKey::Parchment => "parchment",
            ThemeKey::Slate => "slate",
            ThemeKey::Sage => "sage",
            ThemeKey::Ink => "ink",
            ThemeKey::Moss => "moss",
            ThemeKey::Rosewood => "rosewood",
        }
    }

    /// Resolves a persisted key back to a `ThemeKey`, or `None` if it doesn't
    /// match any known theme (e.g. a prefs file from a version with fewer
    /// themes, or written after this one removes one).
    pub fn from_persisted_key(key: &str) -> Option<Self> {
        match key {
            "parchment" => Some(ThemeKey::Parchment),
            "slate" => Some(ThemeKey::Slate),
            "sage" => Some(ThemeKey::Sage),
            "ink" => Some(ThemeKey::Ink),
            "moss" => Some(ThemeKey::Moss),
            "rosewood" => Some(ThemeKey::Rosewood),
            _ => None,
        }
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
    /// Active body-font family; also applied to `gpui_component::Theme`'s
    /// `font_family` by whichever call site changes it (see
    /// `LibraryController::set_body_font`).
    pub body_font: SharedString,
    /// Active value-font family (e.g. Advanced settings' "Cache details"
    /// rows).
    pub value_font: SharedString,
    /// Active label-font family (e.g. the detail tab's metadata labels).
    pub label_font: SharedString,
    /// Active monospace-font family (e.g. the masked API key hint).
    pub mono_font: SharedString,
    /// Shared UI text size, applied via `Window::set_rem_size` so every
    /// `rems(...)`-based size utility (`.text_sm()`, `.text_xs()`, etc.)
    /// scales together, like zooming a page.
    pub ui_text_size: Pixels,
}

impl Default for FontSelections {
    fn default() -> Self {
        Self {
            body_font: SharedString::from(DEFAULT_BODY_FONT),
            value_font: SharedString::from(DEFAULT_VALUE_FONT),
            label_font: SharedString::from(DEFAULT_LABEL_FONT),
            mono_font: SharedString::from(DEFAULT_MONO_FONT),
            ui_text_size: px(DEFAULT_UI_TEXT_SIZE),
        }
    }
}

/// GPUI app-level global containing the active Libri theme, density, and
/// font selections.
#[derive(Debug, Clone)]
pub struct LibriTheme {
    pub key: ThemeKey,
    pub colors: ColorTokens,
    pub density: Density,
    pub density_constants: DensityConstants,
    pub fonts: FontSelections,
}

impl gpui::Global for LibriTheme {}

impl LibriTheme {
    /// Constructs the theme for `key`, `density`, and `fonts`.
    pub fn new(key: ThemeKey, density: Density, fonts: FontSelections) -> Self {
        let colors = match key {
            ThemeKey::Parchment => parchment_colors(),
            ThemeKey::Slate => slate_colors(),
            ThemeKey::Sage => sage_colors(),
            ThemeKey::Ink => ink_colors(),
            ThemeKey::Moss => moss_colors(),
            ThemeKey::Rosewood => rosewood_colors(),
        };
        let density_constants = match density {
            Density::Comfortable => DensityConstants::comfortable(),
            Density::Compact => DensityConstants::compact(),
        };
        Self {
            key,
            colors,
            density,
            density_constants,
            fonts,
        }
    }

    /// Returns the default theme (parchment, comfortable, default fonts).
    pub fn default_theme() -> Self {
        Self::new(
            ThemeKey::Parchment,
            Density::Comfortable,
            FontSelections::default(),
        )
    }
}

// ── gpui-component theme color sync
// ─────────────────────────────────────────────

/// Overrides `gpui_component::Theme`'s semantic colors to match `colors`.
///
/// `gpui-component` widgets (`Button`, `Input`, `Popover`/`PopupMenu`,
/// tooltips, scrollbars, `Sidebar`, `DataTable`/`Table`, `TabBar`/`Tab`
/// (including the catalog view-mode selector and tab strip), `StatusBar`,
/// `TitleBar`, etc.) read their colors from `cx.theme()`
/// (`gpui_component::Theme`), which is a separate
/// global from [`LibriTheme`] and is never otherwise synced with the active
/// Libri palette — so those widgets rendered with `gpui-component`'s default
/// light/dark colors (whatever `Theme::apply_config` computed for the
/// ambient system mode) regardless of which Libri theme was active.
///
/// Call this whenever [`LibriTheme`] changes, updating both `colors` (read
/// directly by most components) and `tokens` (read by `DataTable` and other
/// newer widgets) so the two stay in sync. `ColorTokens` has a smaller set of
/// semantic roles than `gpui_component::Theme` — where there's no dedicated
/// Libri token for a field (e.g. a distinct "info" or "success" hue), the
/// semantically closest token is reused rather than leaving the field on its
/// light/dark default.
pub fn apply_theme_colors(theme: &mut gpui_component::Theme, colors: &ColorTokens) {
    // Base semantic fields.
    theme.colors.background = colors.surface;
    theme.colors.foreground = colors.text_primary;
    theme.colors.border = colors.border;
    theme.colors.muted = colors.surface_alt;
    theme.colors.muted_foreground = colors.text_tertiary;
    theme.colors.ring = colors.accent;
    theme.colors.selection = colors.accent_soft;
    theme.colors.caret = colors.text_primary;
    theme.colors.drag_border = colors.accent;
    theme.colors.drop_target = colors.accent_soft;
    theme.colors.description_list_label = colors.surface_alt;
    theme.colors.description_list_label_foreground = colors.text_secondary;

    theme.tokens.background = colors.surface.into();
    theme.tokens.foreground = colors.text_primary.into();
    theme.tokens.border = colors.border.into();
    theme.tokens.muted = colors.surface_alt.into();
    theme.tokens.muted_foreground = colors.text_tertiary.into();
    theme.tokens.ring = colors.accent.into();
    theme.tokens.selection = colors.accent_soft.into();
    theme.tokens.caret = colors.text_primary.into();
    theme.tokens.drag_border = colors.accent.into();
    theme.tokens.drop_target = colors.accent_soft.into();
    theme.tokens.description_list_label = colors.surface_alt.into();
    theme.tokens.description_list_label_foreground = colors.text_secondary.into();

    // Base semantic-role fields backing the button/sidebar variants.
    theme.colors.primary = colors.accent;
    theme.colors.primary_active = colors.accent_soft;
    theme.colors.primary_foreground = colors.accent_on;
    theme.colors.primary_hover = colors.accent;
    theme.colors.secondary = colors.surface_alt;
    theme.colors.secondary_active = colors.accent_soft;
    theme.colors.secondary_foreground = colors.text_secondary;
    theme.colors.secondary_hover = colors.hover;
    theme.colors.danger = colors.error;
    theme.colors.danger_active = colors.error;
    theme.colors.danger_foreground = colors.accent_on;
    theme.colors.danger_hover = colors.error;
    theme.colors.warning = colors.warning_bg;
    theme.colors.warning_active = colors.warning_bg;
    theme.colors.warning_foreground = colors.warning_text;
    theme.colors.warning_hover = colors.warning_bg;
    theme.colors.info = colors.accent;
    theme.colors.info_active = colors.accent_soft;
    theme.colors.info_foreground = colors.accent_on;
    theme.colors.info_hover = colors.accent;
    theme.colors.success = colors.accent;
    theme.colors.success_active = colors.accent_soft;
    theme.colors.success_foreground = colors.accent_on;
    theme.colors.success_hover = colors.accent;
    theme.colors.list_active = colors.accent_soft;

    theme.tokens.primary = colors.accent.into();
    theme.tokens.primary_active = colors.accent_soft.into();
    theme.tokens.primary_foreground = colors.accent_on.into();
    theme.tokens.primary_hover = colors.accent.into();
    theme.tokens.secondary = colors.surface_alt.into();
    theme.tokens.secondary_active = colors.accent_soft.into();
    theme.tokens.secondary_foreground = colors.text_secondary.into();
    theme.tokens.secondary_hover = colors.hover.into();
    theme.tokens.danger = colors.error.into();
    theme.tokens.danger_active = colors.error.into();
    theme.tokens.danger_foreground = colors.accent_on.into();
    theme.tokens.danger_hover = colors.error.into();
    theme.tokens.warning = colors.warning_bg.into();
    theme.tokens.warning_active = colors.warning_bg.into();
    theme.tokens.warning_foreground = colors.warning_text.into();
    theme.tokens.warning_hover = colors.warning_bg.into();
    theme.tokens.info = colors.accent.into();
    theme.tokens.info_active = colors.accent_soft.into();
    theme.tokens.info_foreground = colors.accent_on.into();
    theme.tokens.info_hover = colors.accent.into();
    theme.tokens.success = colors.accent.into();
    theme.tokens.success_active = colors.accent_soft.into();
    theme.tokens.success_foreground = colors.accent_on.into();
    theme.tokens.success_hover = colors.accent.into();
    theme.tokens.list_active = colors.accent_soft.into();

    // Buttons.
    theme.colors.button = colors.surface_alt;
    theme.colors.button_hover = colors.hover;
    theme.colors.button_active = colors.accent_soft;
    theme.colors.button_foreground = colors.text_primary;
    theme.colors.button_danger = colors.error;
    theme.colors.button_danger_active = colors.error;
    theme.colors.button_danger_foreground = colors.accent_on;
    theme.colors.button_danger_hover = colors.error;
    theme.colors.button_info = colors.accent;
    theme.colors.button_info_active = colors.accent_soft;
    theme.colors.button_info_foreground = colors.accent_on;
    theme.colors.button_info_hover = colors.accent;
    theme.colors.button_primary = colors.accent;
    theme.colors.button_primary_active = colors.accent_soft;
    theme.colors.button_primary_foreground = colors.accent_on;
    theme.colors.button_primary_hover = colors.accent;
    theme.colors.button_secondary = colors.surface_alt;
    theme.colors.button_secondary_active = colors.accent_soft;
    theme.colors.button_secondary_foreground = colors.text_secondary;
    theme.colors.button_secondary_hover = colors.hover;
    theme.colors.button_success = colors.accent;
    theme.colors.button_success_active = colors.accent_soft;
    theme.colors.button_success_foreground = colors.accent_on;
    theme.colors.button_success_hover = colors.accent;
    theme.colors.button_warning = colors.warning_bg;
    theme.colors.button_warning_active = colors.warning_bg;
    theme.colors.button_warning_foreground = colors.warning_text;
    theme.colors.button_warning_hover = colors.warning_bg;

    theme.tokens.button = colors.surface_alt.into();
    theme.tokens.button_hover = colors.hover.into();
    theme.tokens.button_active = colors.accent_soft.into();
    theme.tokens.button_foreground = colors.text_primary.into();
    theme.tokens.button_danger = colors.error.into();
    theme.tokens.button_danger_active = colors.error.into();
    theme.tokens.button_danger_foreground = colors.accent_on.into();
    theme.tokens.button_danger_hover = colors.error.into();
    theme.tokens.button_info = colors.accent.into();
    theme.tokens.button_info_active = colors.accent_soft.into();
    theme.tokens.button_info_foreground = colors.accent_on.into();
    theme.tokens.button_info_hover = colors.accent.into();
    theme.tokens.button_primary = colors.accent.into();
    theme.tokens.button_primary_active = colors.accent_soft.into();
    theme.tokens.button_primary_foreground = colors.accent_on.into();
    theme.tokens.button_primary_hover = colors.accent.into();
    theme.tokens.button_secondary = colors.surface_alt.into();
    theme.tokens.button_secondary_active = colors.accent_soft.into();
    theme.tokens.button_secondary_foreground = colors.text_secondary.into();
    theme.tokens.button_secondary_hover = colors.hover.into();
    theme.tokens.button_success = colors.accent.into();
    theme.tokens.button_success_active = colors.accent_soft.into();
    theme.tokens.button_success_foreground = colors.accent_on.into();
    theme.tokens.button_success_hover = colors.accent.into();
    theme.tokens.button_warning = colors.warning_bg.into();
    theme.tokens.button_warning_active = colors.warning_bg.into();
    theme.tokens.button_warning_foreground = colors.warning_text.into();
    theme.tokens.button_warning_hover = colors.warning_bg.into();

    // Input.
    theme.colors.input = colors.border;
    theme.tokens.input = colors.border.into();

    // Popover.
    theme.colors.popover = colors.surface;
    theme.colors.popover_foreground = colors.text_primary;
    theme.tokens.popover = colors.surface.into();
    theme.tokens.popover_foreground = colors.text_primary.into();

    // Scrollbar.
    theme.colors.scrollbar = colors.surface_alt;
    theme.colors.scrollbar_thumb = colors.border_strong;
    theme.colors.scrollbar_thumb_hover = colors.text_tertiary;
    theme.tokens.scrollbar = colors.surface_alt.into();
    theme.tokens.scrollbar_thumb = colors.border_strong.into();
    theme.tokens.scrollbar_thumb_hover = colors.text_tertiary.into();

    // Sidebar.
    theme.colors.sidebar = colors.surface_alt;
    theme.colors.sidebar_accent = colors.accent_soft;
    theme.colors.sidebar_accent_foreground = colors.text_secondary;
    theme.colors.sidebar_border = colors.border;
    theme.colors.sidebar_foreground = colors.text_primary;
    theme.colors.sidebar_primary = colors.accent;
    theme.colors.sidebar_primary_foreground = colors.accent_on;
    theme.tokens.sidebar = colors.surface_alt.into();
    theme.tokens.sidebar_accent = colors.accent_soft.into();
    theme.tokens.sidebar_accent_foreground = colors.text_secondary.into();
    theme.tokens.sidebar_border = colors.border.into();
    theme.tokens.sidebar_foreground = colors.text_primary.into();
    theme.tokens.sidebar_primary = colors.accent.into();
    theme.tokens.sidebar_primary_foreground = colors.accent_on.into();

    // Table (pre-existing, unchanged mapping).
    theme.colors.table = colors.surface;
    theme.colors.table_even = colors.surface_alt;
    theme.colors.table_head = colors.surface_alt;
    theme.colors.table_head_foreground = colors.text_secondary;
    theme.colors.table_foot = colors.surface_alt;
    theme.colors.table_foot_foreground = colors.text_secondary;
    theme.colors.table_hover = colors.hover;
    theme.colors.table_row_border = colors.border;
    theme.colors.table_active = colors.accent_soft;
    theme.colors.table_active_border = colors.accent;

    theme.tokens.table = colors.surface.into();
    theme.tokens.table_even = colors.surface_alt.into();
    theme.tokens.table_head = colors.surface_alt.into();
    theme.tokens.table_head_foreground = colors.text_secondary.into();
    theme.tokens.table_foot = colors.surface_alt.into();
    theme.tokens.table_foot_foreground = colors.text_secondary.into();
    theme.tokens.table_hover = colors.hover.into();
    theme.tokens.table_row_border = colors.border.into();
    theme.tokens.table_active = colors.accent_soft.into();
    theme.tokens.table_active_border = colors.accent.into();

    // Tab bar (the catalog view-mode selector and any other TabBar/Tab use).
    theme.colors.tab_bar = colors.surface_alt;
    theme.colors.tab_bar_segmented = colors.surface;
    theme.colors.tab = colors.surface_alt;
    theme.colors.tab_active = colors.accent_soft;
    theme.colors.tab_foreground = colors.text_secondary;
    theme.colors.tab_active_foreground = colors.text_primary;

    theme.tokens.tab_bar = colors.surface_alt.into();
    theme.tokens.tab_bar_segmented = colors.surface.into();
    theme.tokens.tab = colors.surface_alt.into();
    theme.tokens.tab_active = colors.accent_soft.into();
    theme.tokens.tab_foreground = colors.text_secondary.into();
    theme.tokens.tab_active_foreground = colors.text_primary.into();

    // Title bar / status bar.
    theme.colors.title_bar = colors.surface;
    theme.colors.title_bar_border = colors.border;
    theme.colors.status_bar = colors.surface_alt;
    theme.colors.status_bar_border = colors.border;

    theme.tokens.title_bar = colors.surface.into();
    theme.tokens.title_bar_border = colors.border.into();
    theme.tokens.status_bar = colors.surface_alt.into();
    theme.tokens.status_bar_border = colors.border.into();
}

// ── Color constructors
// ────────────────────────────────────────────────────────

fn hex(r: u8, g: u8, b: u8) -> Hsla {
    let n = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
    gpui::rgb(n).into()
}

fn hex_a(r: u8, g: u8, b: u8, a: f32) -> Hsla {
    let n = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
    let base: Hsla = gpui::rgb(n).into();
    Hsla { a, ..base }
}

// ── Parchment
// ─────────────────────────────────────────────────────────────────
// --bg:#FAF7F0  --surface:#FCF9F3  --surface-2:#F2ECDF  --hover:#EDE6D6
// --text:#26211A  --text-2:#5B5346  --text-3:#8C8270
// --line:#E7DFCD  --line-2:#DBD1BB  --accent-on:#FCF9F3
// --shadow:rgba(58,46,26,0.18)  --scrim:rgba(30,22,10,0.26)
// accent: oklch(0.47 0.105 25) ≈ #8C4A22 (warm brown-orange)

fn parchment_colors() -> ColorTokens {
    ColorTokens {
        desktop_bg: hex(0xC5, 0xB9, 0x9D),
        surface: hex(0xFC, 0xF9, 0xF3),
        surface_alt: hex(0xF2, 0xEC, 0xDF),
        hover: hex(0xED, 0xE6, 0xD6),
        text_primary: hex(0x26, 0x21, 0x1A),
        text_secondary: hex(0x5B, 0x53, 0x46),
        text_tertiary: hex(0x8C, 0x82, 0x70),
        border: hex(0xE7, 0xDF, 0xCD),
        border_strong: hex(0xDB, 0xD1, 0xBB),
        accent: hex(0x8C, 0x4A, 0x22),
        accent_soft: hex_a(0x8C, 0x4A, 0x22, 0.13),
        accent_on: hex(0xFC, 0xF9, 0xF3),
        shadow: hex_a(0x3A, 0x2E, 0x1A, 0.18),
        scrim: hex_a(0x1E, 0x16, 0x0A, 0.26),
        error: hex(0xB0, 0x30, 0x28),
        warning_bg: gpui::hsla(0.11, 0.9, 0.5, 0.12),
        warning_text: gpui::hsla(0.08, 0.85, 0.35, 1.0),
    }
}

// ── Slate ─────────────────────────────────────────────────────────────────────
// --bg:#FAFBFC  --surface:#FCFDFE  --surface-2:#EEF1F4  --hover:#E7ECF0
// --text:#1B2530  --text-2:#4C5965  --text-3:#7E8B98
// --line:#E4E9ED  --line-2:#D5DCE2  --accent-on:#FCFDFE
// --shadow:rgba(28,42,58,0.18)  --scrim:rgba(18,28,40,0.26)
// accent: oklch(0.47 0.095 25) ≈ #7A4220 (cooler warm)

fn slate_colors() -> ColorTokens {
    ColorTokens {
        desktop_bg: hex(0xAD, 0xB7, 0xBF),
        surface: hex(0xFC, 0xFD, 0xFE),
        surface_alt: hex(0xEE, 0xF1, 0xF4),
        hover: hex(0xE7, 0xEC, 0xF0),
        text_primary: hex(0x1B, 0x25, 0x30),
        text_secondary: hex(0x4C, 0x59, 0x65),
        text_tertiary: hex(0x7E, 0x8B, 0x98),
        border: hex(0xE4, 0xE9, 0xED),
        border_strong: hex(0xD5, 0xDC, 0xE2),
        accent: hex(0x7A, 0x42, 0x20),
        accent_soft: hex_a(0x7A, 0x42, 0x20, 0.13),
        accent_on: hex(0xFC, 0xFD, 0xFE),
        shadow: hex_a(0x1C, 0x2A, 0x3A, 0.18),
        scrim: hex_a(0x12, 0x1C, 0x28, 0.26),
        error: hex(0xB0, 0x30, 0x28),
        warning_bg: gpui::hsla(0.11, 0.9, 0.5, 0.12),
        warning_text: gpui::hsla(0.08, 0.85, 0.35, 1.0),
    }
}

// ── Sage ──────────────────────────────────────────────────────────────────────
// --bg:#F8F9F4  --surface:#FAFBF6  --surface-2:#EDF0E7  --hover:#E6EBDE
// --text:#23271F  --text-2:#515A4A  --text-3:#848D78
// --line:#E2E7DA  --line-2:#D4DBC8  --accent-on:#FAFBF6
// --shadow:rgba(38,48,28,0.18)  --scrim:rgba(24,32,18,0.26)
// accent: oklch(0.47 0.095 25) ≈ #7A4220

fn sage_colors() -> ColorTokens {
    ColorTokens {
        desktop_bg: hex(0xB2, 0xBC, 0xA0),
        surface: hex(0xFA, 0xFB, 0xF6),
        surface_alt: hex(0xED, 0xF0, 0xE7),
        hover: hex(0xE6, 0xEB, 0xDE),
        text_primary: hex(0x23, 0x27, 0x1F),
        text_secondary: hex(0x51, 0x5A, 0x4A),
        text_tertiary: hex(0x84, 0x8D, 0x78),
        border: hex(0xE2, 0xE7, 0xDA),
        border_strong: hex(0xD4, 0xDB, 0xC8),
        accent: hex(0x7A, 0x42, 0x20),
        accent_soft: hex_a(0x7A, 0x42, 0x20, 0.13),
        accent_on: hex(0xFA, 0xFB, 0xF6),
        shadow: hex_a(0x26, 0x30, 0x1C, 0.18),
        scrim: hex_a(0x18, 0x20, 0x12, 0.26),
        error: hex(0xB0, 0x30, 0x28),
        warning_bg: gpui::hsla(0.11, 0.9, 0.5, 0.12),
        warning_text: gpui::hsla(0.08, 0.85, 0.35, 1.0),
    }
}

// ── Ink ───────────────────────────────────────────────────────────────────────
// --bg:#16130D  --surface:#1B1812  --surface-2:#211D15  --hover:#2A241B
// --text:#ECE4D3  --text-2:#B4AA94  --text-3:#877D68
// --line:#2C271E  --line-2:#392F23  --accent-on:#1B1812
// --shadow:rgba(0,0,0,0.5)  --scrim:rgba(0,0,0,0.45)
// accent: oklch(0.76 0.115 25) ≈ #E0845A (light warm)

fn ink_colors() -> ColorTokens {
    ColorTokens {
        desktop_bg: hex(0x14, 0x11, 0x0A),
        surface: hex(0x1B, 0x18, 0x12),
        surface_alt: hex(0x21, 0x1D, 0x15),
        hover: hex(0x2A, 0x24, 0x1B),
        text_primary: hex(0xEC, 0xE4, 0xD3),
        text_secondary: hex(0xB4, 0xAA, 0x94),
        text_tertiary: hex(0x87, 0x7D, 0x68),
        border: hex(0x2C, 0x27, 0x1E),
        border_strong: hex(0x39, 0x2F, 0x23),
        accent: hex(0xE0, 0x84, 0x5A),
        accent_soft: hex_a(0xE0, 0x84, 0x5A, 0.13),
        accent_on: hex(0x1B, 0x18, 0x12),
        shadow: hex_a(0x00, 0x00, 0x00, 0.50),
        scrim: hex_a(0x00, 0x00, 0x00, 0.45),
        error: hex(0xE0, 0x58, 0x58),
        warning_bg: gpui::hsla(0.11, 0.9, 0.5, 0.10),
        warning_text: gpui::hsla(0.10, 0.90, 0.65, 1.0),
    }
}

// ── Moss ──────────────────────────────────────────────────────────────────────
// A second dark theme alongside Ink, distinguished by a cool forest-green
// cast rather than Ink's warm brown-black.
// --bg:#0F1712  --surface:#16201A  --surface-2:#1C2720  --hover:#243028
// --text:#E4ECDD  --text-2:#A9B8A0  --text-3:#7A8A74
// --line:#28332B  --line-2:#344036  --accent-on:#16201A
// --shadow:rgba(0,0,0,0.5)  --scrim:rgba(0,0,0,0.45)
// accent: muted gold-green ≈ #9CB06A

fn moss_colors() -> ColorTokens {
    ColorTokens {
        desktop_bg: hex(0x0F, 0x17, 0x12),
        surface: hex(0x16, 0x20, 0x1A),
        surface_alt: hex(0x1C, 0x27, 0x20),
        hover: hex(0x24, 0x30, 0x28),
        text_primary: hex(0xE4, 0xEC, 0xDD),
        text_secondary: hex(0xA9, 0xB8, 0xA0),
        text_tertiary: hex(0x7A, 0x8A, 0x74),
        border: hex(0x28, 0x33, 0x2B),
        border_strong: hex(0x34, 0x40, 0x36),
        accent: hex(0x9C, 0xB0, 0x6A),
        accent_soft: hex_a(0x9C, 0xB0, 0x6A, 0.13),
        accent_on: hex(0x16, 0x20, 0x1A),
        shadow: hex_a(0x00, 0x00, 0x00, 0.50),
        scrim: hex_a(0x00, 0x00, 0x00, 0.45),
        error: hex(0xE0, 0x58, 0x58),
        warning_bg: gpui::hsla(0.11, 0.9, 0.5, 0.10),
        warning_text: gpui::hsla(0.10, 0.90, 0.65, 1.0),
    }
}

// ── Rosewood
// ──────────────────────────────────────────────────────────────────
// A second light theme alongside Parchment, distinguished by a warm
// burgundy/wine cast rather than Parchment's tan/cream.
// --bg:#C9A8A0  --surface:#FBF3F1  --surface-2:#F0E0DC  --hover:#EAD5D0
// --text:#2A1816  --text-2:#634540  --text-3:#937872
// --line:#E8D5D0  --line-2:#DBC2BC  --accent-on:#FBF3F1
// --shadow:rgba(58,26,24,0.18)  --scrim:rgba(32,14,12,0.26)
// accent: deep wine red ≈ #7A2C2C

fn rosewood_colors() -> ColorTokens {
    ColorTokens {
        desktop_bg: hex(0xC9, 0xA8, 0xA0),
        surface: hex(0xFB, 0xF3, 0xF1),
        surface_alt: hex(0xF0, 0xE0, 0xDC),
        hover: hex(0xEA, 0xD5, 0xD0),
        text_primary: hex(0x2A, 0x18, 0x16),
        text_secondary: hex(0x63, 0x45, 0x40),
        text_tertiary: hex(0x93, 0x78, 0x72),
        border: hex(0xE8, 0xD5, 0xD0),
        border_strong: hex(0xDB, 0xC2, 0xBC),
        accent: hex(0x7A, 0x2C, 0x2C),
        accent_soft: hex_a(0x7A, 0x2C, 0x2C, 0.13),
        accent_on: hex(0xFB, 0xF3, 0xF1),
        shadow: hex_a(0x3A, 0x1A, 0x18, 0.18),
        scrim: hex_a(0x20, 0x0E, 0x0C, 0.26),
        error: hex(0xB0, 0x30, 0x28),
        warning_bg: gpui::hsla(0.11, 0.9, 0.5, 0.12),
        warning_text: gpui::hsla(0.08, 0.85, 0.35, 1.0),
    }
}
