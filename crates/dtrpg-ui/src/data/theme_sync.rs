//! Synchronizes Libri colors with gpui-kit globals.

use gpui::BorrowAppContext;

use super::{ColorTokens, LibriTheme};

// ── gpui-kit theme color sync
// ─────────────────────────────────────────────

/// Overrides `gpui_kit::component::Theme`'s semantic colors to match `colors`.
///
/// `gpui-kit` widgets (`Button`, `Input`, `Popover`/`PopupMenu`,
/// tooltips, scrollbars, `Sidebar`, `DataTable`/`Table`, `TabBar`/`Tab`
/// (including the catalog view-mode selector and tab strip), `StatusBar`,
/// `TitleBar`, etc.) read their colors from `cx.theme()`
/// (`gpui_kit::component::Theme`), which is a separate
/// global from [`LibriTheme`] and is never otherwise synced with the active
/// Libri palette — so those widgets rendered with `gpui-kit`'s default
/// light/dark colors (whatever `Theme::apply_config` computed for the
/// ambient system mode) regardless of which Libri theme was active.
///
/// Call this whenever [`LibriTheme`] changes, updating both `colors` (read
/// Installs `theme` as the active global and syncs its colors into the
/// `gpui_kit::component::Theme` global. Shared by the settings/status theme
/// pickers and the themes-directory hot reload so neither duplicates the
/// two-global update.
pub fn install_theme(theme: LibriTheme, cx: &mut gpui::App) {
    let colors = theme.colors.clone();
    cx.set_global(theme);
    cx.update_global::<gpui_kit::component::Theme, _>(|theme, _cx| {
          apply_theme_colors(theme, &colors);
      });
}

/// directly by most components) and `tokens` (read by `DataTable` and other
/// newer widgets) so the two stay in sync. `ColorTokens` has a smaller set of
/// semantic roles than `gpui_kit::component::Theme` — where there's no
/// dedicated Libri token for a field (e.g. a distinct "info" or "success" hue),
/// the semantically closest token is reused rather than leaving the field on
/// its light/dark default.
pub fn apply_theme_colors(theme: &mut gpui_kit::component::Theme, colors: &ColorTokens) {
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

    // `accent`/`accent_foreground` and the list/group-box/switch/slider/link
    // family below were missing from this mapping even though gpui-kit's
    // `ThemeColor` carries them: anything that falls back to them (most
    // non-button interactive hover states - list rows, menu items, switches,
    // sliders) was keeping gpui-kit's light-theme default instead of this
    // theme's colors, which is why hovering most controls under a dark theme
    // like Ink showed a glaring light/white background.
    theme.colors.accent = colors.accent;
    theme.colors.accent_foreground = colors.accent_on;
    theme.colors.list = colors.surface;
    theme.colors.list_even = colors.surface_alt;
    theme.colors.list_head = colors.surface_alt;
    theme.colors.list_hover = colors.hover;
    theme.colors.list_active_border = colors.accent;
    theme.colors.group_box = colors.surface_alt;
    theme.colors.group_box_foreground = colors.text_secondary;
    theme.colors.overlay = colors.scrim;
    theme.colors.window_border = colors.border_strong;
    theme.colors.skeleton = colors.surface_alt;
    theme.colors.switch = colors.surface_alt;
    theme.colors.switch_thumb = colors.accent_on;
    theme.colors.slider_bar = colors.accent;
    theme.colors.slider_thumb = colors.accent_on;
    theme.colors.link = colors.accent;
    theme.colors.link_active = colors.accent_soft;
    theme.colors.link_hover = colors.accent;
    theme.colors.accordion = colors.surface_alt;
    theme.colors.progress_bar = colors.accent;

    theme.tokens.accent = colors.accent.into();
    theme.tokens.accent_foreground = colors.accent_on.into();
    theme.tokens.list = colors.surface.into();
    theme.tokens.list_even = colors.surface_alt.into();
    theme.tokens.list_head = colors.surface_alt.into();
    theme.tokens.list_hover = colors.hover.into();
    theme.tokens.list_active_border = colors.accent.into();
    theme.tokens.group_box = colors.surface_alt.into();
    theme.tokens.group_box_foreground = colors.text_secondary.into();
    theme.tokens.overlay = colors.scrim.into();
    theme.tokens.window_border = colors.border_strong.into();
    theme.tokens.skeleton = colors.surface_alt.into();
    theme.tokens.switch = colors.surface_alt.into();
    theme.tokens.switch_thumb = colors.accent_on.into();
    theme.tokens.slider_bar = colors.accent.into();
    theme.tokens.slider_thumb = colors.accent_on.into();
    theme.tokens.link = colors.accent.into();
    theme.tokens.link_active = colors.accent_soft.into();
    theme.tokens.link_hover = colors.accent.into();
    theme.tokens.accordion = colors.surface_alt.into();
    theme.tokens.progress_bar = colors.accent.into();

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
