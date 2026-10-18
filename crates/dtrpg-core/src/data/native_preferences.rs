//! Persisted preferences for how Libri works with the operating system:
//! whether a tag moves between the library and a file's tags, and whether that
//! is checked at launch. These are the Tags switches in Settings > Native
//! Integration.
//!
//! Backed by `{app_preferences_dir}/native.toml`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::data::paths::app_preferences_dir;

/// The preferences as stored. A missing field is on, so a file written by an
/// older version, or by hand, keeps working.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(default)]
pub struct NativePreferencesFile {
    /// Whether the library's tags are written to files' tags.
    pub write_tags_to_files:  bool,
    /// Whether tags added or removed on files outside Libri are read back into
    /// the library.
    pub read_tags_from_files: bool,
    /// Whether files' tags are compared with the library's at launch.
    pub reconcile_on_launch:  bool,
}

impl Default for NativePreferencesFile {
    fn default() -> Self {
        Self { write_tags_to_files:  true,
               read_tags_from_files: true,
               reconcile_on_launch:  true, }
    }
}

/// Persists and restores the native-integration preferences.
pub struct NativePreferences {
    path: PathBuf,
    data: NativePreferencesFile,
}

fn preferences_path() -> PathBuf {
    app_preferences_dir().join("native.toml")
}

impl NativePreferences {
    /// Loads the preferences from disk; returns the defaults on any error.
    ///
    /// Loading never writes: the file is created by the first choice saved. A
    /// file that exists but fails to parse is left untouched.
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(&preferences_path())
    }

    /// Loads the preferences from `path`, as [`Self::load`] does.
    #[must_use]
    pub fn load_from(path: &Path) -> Self {
        let data =
            std::fs::read_to_string(path).ok()
                                         .and_then(|text| {
                                             toml::from_str::<NativePreferencesFile>(&text).ok()
                                         })
                                         .unwrap_or_default();
        Self { path: path.to_owned(),
               data }
    }

    /// Whether the library's tags are written to files' tags.
    #[must_use]
    pub fn write_tags_to_files(&self) -> bool {
        self.data.write_tags_to_files
    }

    /// Whether tags changed on files outside Libri are read back.
    #[must_use]
    pub fn read_tags_from_files(&self) -> bool {
        self.data.read_tags_from_files
    }

    /// Whether files' tags are compared with the library's at launch.
    #[must_use]
    pub fn reconcile_on_launch(&self) -> bool {
        self.data.reconcile_on_launch
    }

    /// Persists whether the library's tags are written to files' tags.
    pub fn save_write_tags_to_files(&mut self, enabled: bool) {
        self.data.write_tags_to_files = enabled;
        self.flush();
    }

    /// Persists whether tags changed on files outside Libri are read back.
    pub fn save_read_tags_from_files(&mut self, enabled: bool) {
        self.data.read_tags_from_files = enabled;
        self.flush();
    }

    /// Persists whether files' tags are compared with the library's at launch.
    pub fn save_reconcile_on_launch(&mut self, enabled: bool) {
        self.data.reconcile_on_launch = enabled;
        self.flush();
    }

    /// Writes the preferences to disk, creating the directory if needed. A
    /// failure is logged and otherwise ignored, as a preference is not worth
    /// failing for.
    fn flush(&self) {
        let result = toml::to_string_pretty(&self.data).map_err(|error| error.to_string())
                                                       .and_then(|text| {
                                                           if let Some(dir) = self.path.parent() {
                                                               std::fs::create_dir_all(dir)
                                                                   .map_err(|e| e.to_string())?;
                                                           }
                                                           std::fs::write(&self.path, text)
                                                               .map_err(|e| e.to_string())
                                                       });
        if let Err(error) = result {
            tracing::warn!(%error, path = %self.path.display(), "could not save native preferences");
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn in_temp_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings").join("native.toml");
        (dir, path)
    }

    #[test]
    fn everything_is_on_by_default_and_loading_writes_nothing() {
        let (_dir, path) = in_temp_dir();

        let preferences = NativePreferences::load_from(&path);

        assert!(preferences.write_tags_to_files());
        assert!(preferences.read_tags_from_files());
        assert!(preferences.reconcile_on_launch());
        assert!(!path.exists());
    }

    #[test]
    fn a_saved_choice_is_there_after_loading_again() {
        let (_dir, path) = in_temp_dir();
        let mut preferences = NativePreferences::load_from(&path);

        preferences.save_read_tags_from_files(false);
        preferences.save_reconcile_on_launch(false);
        let reloaded = NativePreferences::load_from(&path);

        assert!(reloaded.write_tags_to_files());
        assert!(!reloaded.read_tags_from_files());
        assert!(!reloaded.reconcile_on_launch());
    }

    #[test]
    fn a_field_missing_from_the_file_is_on() {
        let (_dir, path) = in_temp_dir();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "write_tags_to_files = false\n").unwrap();

        let preferences = NativePreferences::load_from(&path);

        assert!(!preferences.write_tags_to_files());
        assert!(preferences.read_tags_from_files());
        assert!(preferences.reconcile_on_launch());
    }

    #[test]
    fn a_file_that_cannot_be_read_gives_the_defaults_and_is_left_alone() {
        let (_dir, path) = in_temp_dir();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "this is = not [ valid").unwrap();

        let preferences = NativePreferences::load_from(&path);

        assert!(preferences.write_tags_to_files());
        assert_eq!(std::fs::read_to_string(&path).unwrap(),
                   "this is = not [ valid");
    }
}
