//! Appearance/locale preferences and small presentation accessors.

use super::*;
use crate::data::theme_catalog::ThemeId;

impl LibraryController {
    /// Returns distinct publisher names currently present in the catalog for
    /// placement-rule auto-completion.
    pub fn publisher_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.all_items()
                                         .into_iter()
                                         .map(|item| item.publisher.to_string())
                                         .collect();
        names.sort_unstable_by_key(|name| name.to_ascii_lowercase());
        names.dedup();
        names
    }

    // ── Theme / density mutations (dispatched via callbacks)
    // ──────────────────

    /// Applies a new theme key (updates the GPUI global) and persists it via
    /// [`dtrpg_core::data::ui_preferences::UiPreferences`] so it survives a
    /// restart instead of always resetting to Parchment.
    ///
    /// Also re-syncs `gpui_kit::component::Theme`'s colors (see
    /// [`crate::data::theme::apply_theme_colors`]) so buttons, inputs,
    /// popovers, scrollbars, the sidebar, and the catalog `DataTable` all
    /// track the newly selected Libri palette instead of staying on whichever
    /// palette was active at startup.
    pub fn set_theme(&self, id: ThemeId, cx: &mut Context<Self>) {
        let current = cx.global::<LibriTheme>();
        let new_theme = LibriTheme::new(id.clone(), current.density, current.fonts.clone());
        install_theme(new_theme, cx);
        dtrpg_core::data::ui_preferences::UiPreferences::load().save_theme_key(id.as_str());
        cx.notify();
    }

    /// Applies a new density (updates the GPUI global).
    pub fn set_density(&self, density: Density, cx: &mut Context<Self>) {
        let current = cx.global::<LibriTheme>();
        let new_theme = LibriTheme::new(current.id.clone(), density, current.fonts.clone());
        cx.set_global(new_theme);
        cx.notify();
    }

    /// Switches the active locale (updates `rust_i18n`'s global state) and
    /// persists it via [`dtrpg_core::data::ui_preferences::UiPreferences`] so
    /// it takes precedence over OS-locale detection on the next launch.
    pub fn set_locale(&self, locale: crate::i18n::Locale, cx: &mut Context<Self>) {
        rust_i18n::set_locale(locale.code());
        dtrpg_core::data::ui_preferences::UiPreferences::load().save_locale(locale.code());
        cx.notify();
    }

    /// Applies a new body font family (any font installed on the user's
    /// system, not a curated list — see `settings_appearance_view`), updating
    /// the GPUI global, `gpui_kit`'s `Theme.font_family`, and
    /// persisting the selection via
    /// [`dtrpg_core::data::ui_preferences::UiPreferences`].
    pub fn set_body_font(&self, font: impl Into<SharedString>, cx: &mut Context<Self>) {
        let font = font.into();
        let current = cx.global::<LibriTheme>();
        let mut fonts = current.fonts.clone();
        fonts.body_font = font.clone();
        let new_theme = LibriTheme::new(current.id.clone(), current.density, fonts);
        cx.set_global(new_theme);
        cx.update_global::<gpui_kit::component::Theme, _>(|theme, _cx| {
              theme.font_family = font.clone();
          });
        dtrpg_core::data::ui_preferences::UiPreferences::load().save_body_font_name(&font);
        cx.notify();
    }

    /// Applies a new value font family and persists the selection via
    /// [`dtrpg_core::data::ui_preferences::UiPreferences`].
    pub fn set_value_font(&self, font: impl Into<SharedString>, cx: &mut Context<Self>) {
        let font = font.into();
        let current = cx.global::<LibriTheme>();
        let mut fonts = current.fonts.clone();
        fonts.value_font = font.clone();
        let new_theme = LibriTheme::new(current.id.clone(), current.density, fonts);
        cx.set_global(new_theme);
        dtrpg_core::data::ui_preferences::UiPreferences::load().save_value_font_name(&font);
        cx.notify();
    }

    /// Applies a new label font family and persists the selection via
    /// [`dtrpg_core::data::ui_preferences::UiPreferences`].
    pub fn set_label_font(&self, font: impl Into<SharedString>, cx: &mut Context<Self>) {
        let font = font.into();
        let current = cx.global::<LibriTheme>();
        let mut fonts = current.fonts.clone();
        fonts.label_font = font.clone();
        let new_theme = LibriTheme::new(current.id.clone(), current.density, fonts);
        cx.set_global(new_theme);
        dtrpg_core::data::ui_preferences::UiPreferences::load().save_label_font_name(&font);
        cx.notify();
    }

    /// Applies a new monospace font family and persists the selection via
    /// [`dtrpg_core::data::ui_preferences::UiPreferences`].
    pub fn set_mono_font(&self, font: impl Into<SharedString>, cx: &mut Context<Self>) {
        let font = font.into();
        let current = cx.global::<LibriTheme>();
        let mut fonts = current.fonts.clone();
        fonts.mono_font = font.clone();
        let new_theme = LibriTheme::new(current.id.clone(), current.density, fonts);
        cx.set_global(new_theme);
        dtrpg_core::data::ui_preferences::UiPreferences::load().save_mono_font_name(&font);
        cx.notify();
    }

    /// Applies a new shared UI text size (see [`FontSelections::ui_text_size`])
    /// and persists it via [`dtrpg_core::data::ui_preferences::UiPreferences`].
    /// Applied to each window via `Window::set_rem_size` in
    /// `LibraryRootView::render` and `SettingsWindowView::render`, so every
    /// `rems(...)`-based size utility scales together, like zooming a page.
    pub fn set_ui_text_size(&self, size: Pixels, cx: &mut Context<Self>) {
        let current = cx.global::<LibriTheme>();
        let mut fonts = current.fonts.clone();
        fonts.ui_text_size = size;
        let new_theme = LibriTheme::new(current.id.clone(), current.density, fonts);
        cx.set_global(new_theme);
        // Persisted as the scale multiplier shown in Settings > Appearance
        // (e.g. 1.1), not the derived absolute pixel size, so the file's
        // stored value matches what the UI displays.
        let scale = size.as_f32() / dtrpg_core::data::constants::DEFAULT_UI_TEXT_SIZE;
        dtrpg_core::data::ui_preferences::UiPreferences::load().save_text_scale(scale);
        cx.notify();
    }

    // ── Helper accessors
    // ──────────────────────────────────────────────────────

    /// Returns the selected `LibraryItem`, if any.
    #[must_use]
    pub fn selected_item(&self) -> Option<LibraryItem> {
        match &self.selection {
            Selection::Item(id) => self.item_by_id(id),
            Selection::None => None,
        }
    }

    /// Total file size of all items in the catalog, in MB.
    #[must_use]
    pub fn total_size_mb(&self) -> f64 {
        self.all_items().iter().map(|i| i.size_mb).sum()
    }
}
