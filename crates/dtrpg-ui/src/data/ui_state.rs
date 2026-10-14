//! Ephemeral UI state: panel widths, sidebar section-open flags, the
//! last-active settings tab, and the library window's position/size.
//! Regenerated from defaults if lost — not something a user would expect to
//! find or hand-edit, unlike [`crate::data::ui_preferences`].
//!
//! Backed by `{app_data_dir}/ui_state.toml` (Application Support).

use serde::{Deserialize, Serialize};

use crate::data::paths::app_data_dir;

/// Persisted position and size of the library window, in pixels.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct WindowBoundsPref {
    pub x:      f32,
    pub y:      f32,
    pub width:  f32,
    pub height: f32,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct UiStateFile {
    /// Width of the sidebar panel in pixels.
    pub sidebar_width:         Option<f32>,
    /// Width of the detail panel in pixels.
    pub detail_width:          Option<f32>,
    /// Whether the Collections sidebar section is open (`true`) or collapsed
    /// (`false`).
    pub collections_open:      Option<bool>,
    /// Whether the Publishers sidebar section is open (`true`) or collapsed
    /// (`false`).
    pub publishers_open:       Option<bool>,
    /// Index of the last-active settings page (Account/Storage/File
    /// Openers/Advanced/About), so the settings window reopens on the same
    /// tab it was closed on.
    pub settings_page_ix:      Option<usize>,
    /// Position and size the library window was closed at, so it reopens
    /// there instead of always resetting to the default placement.
    pub library_window_bounds: Option<WindowBoundsPref>,
}

/// Persists and restores ephemeral UI state.
///
/// Backed by `{app_data_dir}/ui_state.toml`.
pub struct UiState {
    data: UiStateFile,
}

impl UiState {
    /// Load from disk; returns defaults on any error.
    ///
    /// If no file exists yet (first run), writes the defaults to disk
    /// immediately, mirroring [`crate::data::storage::StorageConfig::load`].
    /// A file that exists but fails to parse is left untouched.
    pub fn load() -> Self {
        let path = state_path();
        let file_existed = path.exists();
        let data =
            std::fs::read_to_string(&path).ok()
                                          .and_then(|text| {
                                              toml::from_str::<UiStateFile>(&text).ok()
                                          })
                                          .unwrap_or_default();
        let state = Self { data };
        if !file_existed {
            state.flush();
        }
        state
    }

    /// Width of the sidebar panel, or `None` if not saved.
    pub fn sidebar_width(&self) -> Option<f32> {
        self.data.sidebar_width
    }

    /// Width of the detail panel, or `None` if not saved.
    pub fn detail_width(&self) -> Option<f32> {
        self.data.detail_width
    }

    /// Persist the sidebar width only.
    pub fn save_sidebar_width(&mut self, sidebar: f32) {
        self.data.sidebar_width = Some(sidebar);
        self.flush();
    }

    /// Persist both sidebar and detail widths atomically.
    pub fn save_panel_widths(&mut self, sidebar: f32, detail: f32) {
        self.data.sidebar_width = Some(sidebar);
        self.data.detail_width = Some(detail);
        self.flush();
    }

    /// Whether the Collections sidebar section is open (defaults to `true`).
    pub fn collections_open(&self) -> bool {
        self.data.collections_open.unwrap_or(true)
    }

    /// Whether the Publishers sidebar section is open (defaults to `true`).
    pub fn publishers_open(&self) -> bool {
        self.data.publishers_open.unwrap_or(true)
    }

    /// Persist the Collections section open state.
    pub fn save_collections_open(&mut self, open: bool) {
        self.data.collections_open = Some(open);
        self.flush();
    }

    /// Persist the Publishers section open state.
    pub fn save_publishers_open(&mut self, open: bool) {
        self.data.publishers_open = Some(open);
        self.flush();
    }

    /// Index of the last-active settings page, or `None` if never saved.
    pub fn settings_page_ix(&self) -> Option<usize> {
        self.data.settings_page_ix
    }

    /// Persist the active settings page index.
    pub fn save_settings_page_ix(&mut self, ix: usize) {
        self.data.settings_page_ix = Some(ix);
        self.flush();
    }

    /// Position and size the library window was last closed at, or `None` if
    /// never saved.
    pub fn library_window_bounds(&self) -> Option<WindowBoundsPref> {
        self.data.library_window_bounds
    }

    /// Persist the library window's position and size.
    pub fn save_library_window_bounds(&mut self, bounds: WindowBoundsPref) {
        self.data.library_window_bounds = Some(bounds);
        self.flush();
    }

    fn flush(&self) {
        let path = state_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = toml::to_string(&self.data) {
            let _ = std::fs::write(&path, text);
        }
    }
}

fn state_path() -> std::path::PathBuf {
    app_data_dir().join("ui_state.toml")
}
