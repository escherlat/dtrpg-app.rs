//! Theme file operations backing the Settings > Appearance gallery
//! (ADR-0005): duplicate the active theme, import an external theme file,
//! reveal the themes folder, and reload the catalog on demand.
//!
//! Every method that changes the theme set ends with
//! [`ThemeCatalog::refresh`], which also rebuilds the gallery's preview
//! cache, so cards reflect the change without waiting for the file watcher.

use super::*;
use crate::data::theme_catalog::{ThemeCatalog, ThemeId, theme_label};
use crate::data::theme_loader;
use crate::util::reveal::reveal_in_file_manager;
use crate::util::theme_watcher;

/// Errors from the gallery's theme-file actions.
///
/// Views log these as warnings (the settings window has no notification
/// surface). Messages name the theme id or the picked file, nothing else.
#[derive(Debug, thiserror::Error)]
pub enum ThemeFileError {
    /// The theme has no readable TOML source: removed, unreadable, or not
    /// parseable as TOML.
    #[error("theme {0} has no readable TOML source")]
    UnreadableSource(String),
    /// The imported file failed to parse as a Libri theme.
    #[error("{0} is not a valid Libri theme file")]
    InvalidTheme(String),
    /// The imported file's stem cannot serve as a theme id.
    #[error("cannot use {0} as a theme id")]
    InvalidStem(String),
    /// Filesystem failure while copying or writing a theme file.
    #[error("theme file I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

impl LibraryController {
    /// Duplicates the active theme into the themes directory as
    /// `<id>-copy.toml` (uniquified with `-2`, `-3`, …), rewrites the copy's
    /// display name to read as a copy, refreshes the catalog, selects the
    /// copy, and reveals the new file so the user can edit it — the themes
    /// watcher applies edits without a restart.
    pub fn duplicate_active_theme(&self, cx: &mut Context<Self>)
                                  -> Result<PathBuf, ThemeFileError> {
        let active = cx.global::<LibriTheme>().id.clone();
        let source = theme_loader::theme_source(&active)
            .ok_or_else(|| ThemeFileError::UnreadableSource(active.to_string()))?;
        let new_name = t!("settings.appearance_theme_copy_name",
                          name = theme_label(&active)).to_string();
        let rewritten = with_display_name(&source, &new_name)
            .ok_or_else(|| ThemeFileError::UnreadableSource(active.to_string()))?;
        let dir = dtrpg_core::data::paths::themes_dir();
        let _ = std::fs::create_dir_all(&dir);
        let stem = unique_theme_stem(&dir, &format!("{}-copy", active));
        let path = dir.join(format!("{stem}.toml"));
        std::fs::write(&path, rewritten)?;
        ThemeCatalog::refresh();
        self.set_theme(ThemeId::from(stem.as_str()), cx);
        if let Err(e) = reveal_in_file_manager(&path) {
            tracing::warn!("could not reveal duplicated theme {}: {e}", path.display());
        }
        Ok(path)
    }

    /// Copies an external theme file into the themes directory under a free
    /// id — its file stem, uniquified when the stem names a built-in
    /// (built-ins win over user files, so a shadowed id is never chosen) or
    /// an existing file — refreshes the catalog, and selects the import.
    ///
    /// The file is validated before copying, so an invalid theme never lands
    /// in the themes directory.
    pub fn import_theme_file(&self, source: PathBuf, cx: &mut Context<Self>)
                             -> Result<ThemeId, ThemeFileError> {
        if theme_loader::load_from_path(&source).is_none() {
            return Err(ThemeFileError::InvalidTheme(source.display().to_string()));
        }
        let stem = source.file_stem()
                         .and_then(|s| s.to_str())
                         .unwrap_or_default();
        if stem.is_empty() || stem.contains(['/', '\\']) || stem.contains("..") {
            return Err(ThemeFileError::InvalidStem(stem.to_string()));
        }
        let dir = dtrpg_core::data::paths::themes_dir();
        let _ = std::fs::create_dir_all(&dir);
        let stem = unique_theme_stem(&dir, stem);
        let dest = dir.join(format!("{stem}.toml"));
        std::fs::copy(&source, &dest)?;
        ThemeCatalog::refresh();
        let id = ThemeId::from(stem.as_str());
        self.set_theme(id.clone(), cx);
        Ok(id)
    }

    /// Opens the themes directory in the file manager, creating it first so
    /// a first-time user has a place to drop theme files.
    pub fn reveal_themes_folder(&self) {
        let dir = dtrpg_core::data::paths::themes_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Err(e) = reveal_in_file_manager(&dir) {
            tracing::warn!("could not reveal themes folder {}: {e}", dir.display());
        }
    }

    /// Runs the file watcher's catalog refresh + active-theme reinstall on
    /// demand (the gallery's Reload button), without waiting for a file
    /// event.
    pub fn reload_themes(&self, cx: &mut Context<Self>) {
        theme_watcher::apply_theme_changes(cx);
    }
}

/// First free theme id for `base`: `base` itself, then `base-2`, `base-3`, …
///
/// "Taken" means a same-named file already exists in `dir` or the id names a
/// built-in — built-ins win over user files, so an id that would be
/// shadowed is never chosen.
fn unique_theme_stem(dir: &Path, base: &str) -> String {
    let taken =
        |stem: &str| ThemeCatalog::contains(stem) || dir.join(format!("{stem}.toml")).exists();
    if !taken(base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Rewrites a theme file's `name` field to `new_name`, preserving every
/// other field. Returns `None` if the source doesn't parse as TOML.
///
/// Re-serialization drops comments and key order — acceptable in the copy,
/// which exists to be edited; the original file is untouched.
fn with_display_name(source: &str, new_name: &str) -> Option<String> {
    let mut doc: toml::Table = toml::from_str(source).ok()?;
    doc.insert("name".to_string(),
               toml::Value::String(new_name.to_string()));
    toml::to_string(&doc).ok()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures use unwrap/expect for concise assertions.

    use super::{unique_theme_stem, with_display_name};

    #[test]
    fn unique_theme_stem_returns_base_when_free() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(unique_theme_stem(dir.path(), "midnight"), "midnight");
    }

    #[test]
    fn unique_theme_stem_uniquifies_around_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("taken.toml"), "name = \"Taken\"\n").unwrap();
        std::fs::write(dir.path().join("taken-2.toml"), "name = \"Taken 2\"\n").unwrap();
        assert_eq!(unique_theme_stem(dir.path(), "taken"), "taken-3");
    }

    #[test]
    fn unique_theme_stem_uniquifies_around_builtin_ids() {
        // A built-in id is taken even with no file present, because
        // built-ins win over same-named user files.
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(unique_theme_stem(dir.path(), "ink"), "ink-2");
    }

    #[test]
    fn with_display_name_rewrites_only_the_name() {
        let source = "name = \"Midnight\"\nmode = \"dark\"\nsurface = \"#101010\"\n";
        let rewritten = with_display_name(source, "Midnight copy").expect("rewrite should parse");
        let parsed: toml::Table = toml::from_str(&rewritten).expect("rewrite should stay valid");
        assert_eq!(parsed["name"].as_str(), Some("Midnight copy"));
        assert_eq!(parsed["mode"].as_str(), Some("dark"));
        assert_eq!(parsed["surface"].as_str(), Some("#101010"));
    }

    #[test]
    fn with_display_name_adds_a_missing_name() {
        let rewritten =
            with_display_name("mode = \"light\"\n", "Imported").expect("rewrite should parse");
        let parsed: toml::Table = toml::from_str(&rewritten).expect("rewrite should stay valid");
        assert_eq!(parsed["name"].as_str(), Some("Imported"));
    }

    #[test]
    fn with_display_name_rejects_invalid_toml() {
        assert!(with_display_name("name = [broken", "X").is_none());
    }
}
