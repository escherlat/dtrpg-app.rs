use gpui::{App, SharedString, px};

use crate::data::catalog_cache::ensure_cache_metadata_exists;
use crate::data::constants::{
    DEFAULT_BODY_FONT, DEFAULT_LABEL_FONT, DEFAULT_MONO_FONT, DEFAULT_UI_TEXT_SIZE,
    DEFAULT_VALUE_FONT,
};
use crate::data::paths::cache_dir;
use crate::data::theme::{Density, FontSelections, LibriTheme, ThemeKey};
use crate::data::ui_preferences::UiPreferences;
use crate::i18n;
use crate::ui::library::cover::CoverCache;

// ── GPUI global initializer
// ───────────────────────────────────────────────────

/// Detects the system locale and activates it, then registers GPUI app-level
/// globals.
///
/// Restores the persisted theme and font selections from [`UiPreferences`]
/// (falling back to defaults for anything missing, unrecognized, or no longer
/// installed — e.g. first launch, a prefs file predating this preference, or
/// a font that was uninstalled since it was chosen), rather than always
/// starting from [`LibriTheme::default_theme`].
///
/// Must be called before any view renders.
pub fn init_globals(cx: &mut App) {
    i18n::init();
    cx.set_global(initial_theme(cx));
    cx.set_global(CoverCache::new());
    // `StorageConfig::load`/`UiPreferences::load`/`UiState::load` already
    // write their own defaults to disk on first call; the catalog cache
    // metadata sidecar has no such self-initializing wrapper type (it's a
    // free function keyed by an explicit `root: &Path`), so it needs an
    // explicit boot-time call instead.
    ensure_cache_metadata_exists(&cache_dir());
}

/// Resolves the persisted theme key and font selections into a [`LibriTheme`],
/// used only at startup — subsequent changes go through
/// `LibraryController::set_theme`/`set_body_font`/`set_value_font`/
/// `set_label_font`/`set_mono_font`/`set_ui_text_size`, which preserve
/// whichever selections aren't being changed.
fn initial_theme(cx: &App) -> LibriTheme {
    let prefs = UiPreferences::load();
    let key = prefs.theme_key()
                   .and_then(ThemeKey::from_persisted_key)
                   .unwrap_or_default();

    let installed = cx.text_system().all_font_names();
    let body_font = resolve_installed_font(&installed, prefs.body_font_name(), DEFAULT_BODY_FONT);
    let value_font =
        resolve_installed_font(&installed, prefs.value_font_name(), DEFAULT_VALUE_FONT);
    let label_font =
        resolve_installed_font(&installed, prefs.label_font_name(), DEFAULT_LABEL_FONT);
    let mono_font = resolve_installed_font(&installed, prefs.mono_font_name(), DEFAULT_MONO_FONT);
    // Persisted as a scale multiplier (1.0 = normal), not an absolute pixel
    // size, so it round-trips to exactly what Settings > Appearance displays.
    let text_scale = prefs.text_scale().unwrap_or(1.0);
    let ui_text_size = px(text_scale * DEFAULT_UI_TEXT_SIZE);

    let fonts = FontSelections { body_font,
                                 value_font,
                                 label_font,
                                 mono_font,
                                 ui_text_size };
    LibriTheme::new(key, Density::default(), fonts)
}

/// Resolves a persisted font family name against `installed` (the system's
/// actually-available font names), falling back to `default` if the
/// persisted name is `None` or no longer installed.
fn resolve_installed_font(installed: &[String], persisted: Option<&str>, default: &'static str)
                          -> SharedString {
    match persisted {
        Some(name) if installed.iter().any(|f| f == name) => SharedString::from(name.to_string()),
        _ => SharedString::from(default),
    }
}
