//! Persisted user-facing UI preferences: color theme, font selections, text
//! scale, and Collections sort order. Unlike [`crate::data::ui_state`], these
//! are choices a user made deliberately and would expect to survive
//! independently of transient window/panel layout.
//!
//! Backed by `{app_preferences_dir}/ui.toml`.

use serde::{Deserialize, Serialize};

use crate::data::paths::app_preferences_dir;
use crate::util::sort::{CollectionSortMethod, SortDirection};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct UiPreferencesFile {
    /// Persisted key of the active color theme (`ThemeKey`'s variant name,
    /// lowercased). `None` before this preference existed, or if never
    /// changed from the default.
    pub theme_key:                 Option<String>,
    /// Persisted font family name of the active body font. Any font
    /// installed on the user's system is valid, not a curated list.
    pub body_font_name:            Option<String>,
    /// Persisted font family name of the active value font.
    pub value_font_name:           Option<String>,
    /// Persisted font family name of the active label font.
    pub label_font_name:           Option<String>,
    /// Persisted font family name of the active monospace font.
    pub mono_font_name:            Option<String>,
    /// Text scale multiplier (e.g. `1.1`), where `1.0` is the app's normal
    /// text scale — the same value shown and adjusted in Settings >
    /// Appearance, not a derived absolute pixel size.
    pub text_scale:                Option<f32>,
    /// Persisted sidebar Collections sort method (`CollectionSortMethod`'s
    /// variant name: `"name"`, `"date_created"`, or `"item_count"`).
    pub collection_sort:           Option<String>,
    /// Persisted sidebar Collections sort direction (`"ascending"` or
    /// `"descending"`).
    pub collection_sort_direction: Option<String>,
    /// Persisted locale code (`"en"`, `"fr"`, `"de"`), read ahead of
    /// OS-locale detection at startup. `None` before this preference existed,
    /// or if never changed from the OS-detected default.
    pub locale:                    Option<String>,
}

/// Persists and restores user-facing UI preferences.
///
/// Backed by `{app_preferences_dir}/ui.toml`.
pub struct UiPreferences {
    data: UiPreferencesFile,
}

impl UiPreferences {
    /// Load from disk; returns defaults on any error.
    ///
    /// If no file exists yet (first run), writes the defaults to disk
    /// immediately, mirroring [`crate::data::storage::StorageConfig::load`].
    /// A file that exists but fails to parse is left untouched.
    pub fn load() -> Self {
        let path = preferences_path();
        let file_existed = path.exists();
        let data = std::fs::read_to_string(&path).ok()
                                                 .and_then(|text| {
                                                     toml::from_str::<UiPreferencesFile>(&text).ok()
                                                 })
                                                 .unwrap_or_default();
        let prefs = Self { data };
        if !file_existed {
            prefs.flush();
        }
        prefs
    }

    /// Persisted key of the active color theme, or `None` if never saved.
    pub fn theme_key(&self) -> Option<&str> {
        self.data.theme_key.as_deref()
    }

    /// Persist the active color theme's key.
    pub fn save_theme_key(&mut self, key: &str) {
        self.data.theme_key = Some(key.to_string());
        self.flush();
    }

    /// Persisted font family name of the active body font, or `None` if never
    /// saved.
    pub fn body_font_name(&self) -> Option<&str> {
        self.data.body_font_name.as_deref()
    }

    /// Persist the active body font's family name.
    pub fn save_body_font_name(&mut self, name: &str) {
        self.data.body_font_name = Some(name.to_string());
        self.flush();
    }

    /// Persisted font family name of the active value font, or `None` if
    /// never saved.
    pub fn value_font_name(&self) -> Option<&str> {
        self.data.value_font_name.as_deref()
    }

    /// Persist the active value font's family name.
    pub fn save_value_font_name(&mut self, name: &str) {
        self.data.value_font_name = Some(name.to_string());
        self.flush();
    }

    /// Persisted font family name of the active label font, or `None` if
    /// never saved.
    pub fn label_font_name(&self) -> Option<&str> {
        self.data.label_font_name.as_deref()
    }

    /// Persist the active label font's family name.
    pub fn save_label_font_name(&mut self, name: &str) {
        self.data.label_font_name = Some(name.to_string());
        self.flush();
    }

    /// Persisted font family name of the active monospace font, or `None` if
    /// never saved.
    pub fn mono_font_name(&self) -> Option<&str> {
        self.data.mono_font_name.as_deref()
    }

    /// Persist the active monospace font's family name.
    pub fn save_mono_font_name(&mut self, name: &str) {
        self.data.mono_font_name = Some(name.to_string());
        self.flush();
    }

    /// Persisted text scale multiplier, or `None` if never saved.
    pub fn text_scale(&self) -> Option<f32> {
        self.data.text_scale
    }

    /// Persist the text scale multiplier (e.g. `1.1`) — the same value shown
    /// in Settings > Appearance, not an absolute pixel size.
    pub fn save_text_scale(&mut self, scale: f32) {
        self.data.text_scale = Some(scale);
        self.flush();
    }

    /// Sidebar Collections sort method (defaults to `Name`).
    pub fn collection_sort(&self) -> CollectionSortMethod {
        match self.data.collection_sort.as_deref() {
            Some("date_created") => CollectionSortMethod::DateCreated,
            Some("item_count") => CollectionSortMethod::ItemCount,
            _ => CollectionSortMethod::Name,
        }
    }

    /// Sidebar Collections sort direction (defaults to `Ascending`).
    pub fn collection_sort_direction(&self) -> SortDirection {
        match self.data.collection_sort_direction.as_deref() {
            Some("descending") => SortDirection::Descending,
            _ => SortDirection::Ascending,
        }
    }

    /// Persist the sidebar Collections sort method and direction together.
    pub fn save_collection_sort(&mut self, method: CollectionSortMethod, direction: SortDirection) {
        self.data.collection_sort = Some(match method {
                                             CollectionSortMethod::Name => "name",
                                             CollectionSortMethod::DateCreated => "date_created",
                                             CollectionSortMethod::ItemCount => "item_count",
                                         }.to_string());
        self.data.collection_sort_direction = Some(match direction {
                                                       SortDirection::Ascending => "ascending",
                                                       SortDirection::Descending => "descending",
                                                   }.to_string());
        self.flush();
    }

    /// Persisted locale code, or `None` if never saved.
    pub fn locale(&self) -> Option<&str> {
        self.data.locale.as_deref()
    }

    /// Persist the active locale's code.
    pub fn save_locale(&mut self, code: &str) {
        self.data.locale = Some(code.to_string());
        self.flush();
    }

    fn flush(&self) {
        let path = preferences_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = toml::to_string(&self.data) {
            let _ = std::fs::write(&path, text);
        }
    }
}

fn preferences_path() -> std::path::PathBuf {
    app_preferences_dir().join("ui.toml")
}
