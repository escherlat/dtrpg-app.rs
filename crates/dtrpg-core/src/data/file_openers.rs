//! File-opener override configuration: persists extension → app-path mappings.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::data::enums::CatalogPresentation;
use crate::util::filter::SidebarFilter;
use crate::util::sort::SortMethod;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A single extension → application path override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOpenerEntry {
    /// File extension without a leading dot, lower-cased (e.g. `"pdf"`).
    pub extension: String,
    /// Absolute path to the application used to open files of this type.
    pub app_path:  PathBuf,
}

/// Outcome of calling [`FileOpenerConfig::add`].
#[derive(Debug, PartialEq, Eq)]
pub enum AddOutcome {
    /// A new entry was appended.
    Added,
    /// An existing entry for the same extension was replaced.
    Replaced,
}

/// Ordered list of file-opener overrides.
///
/// Serializes as a TOML `[[file_openers]]` array inside the shared app config
/// file (see [`config_path`]).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileOpenerConfig {
    #[serde(default, rename = "file_openers")]
    entries: Vec<FileOpenerEntry>,
}

/// Persisted catalog view preferences: sidebar filter, sort, grouping, and
/// presentation mode. Serializes as the `[catalog_view]` section of the
/// shared app config file (see [`config_path`]). The search query is
/// intentionally absent — it is never persisted.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogViewPrefs {
    #[serde(default)]
    sort:             Option<String>,
    #[serde(default)]
    pub grouped:      Option<bool>,
    #[serde(default)]
    presentation:     Option<String>,
    #[serde(default)]
    filter:           Option<String>,
    #[serde(default)]
    filter_publisher: Option<String>,
}

impl CatalogViewPrefs {
    /// Loads catalog view preferences from the shared app config file,
    /// defaulting any absent or unparseable value.
    pub fn load() -> Self {
        load_app_config().catalog_view
    }

    /// Persists `self` as the `[catalog_view]` section of the shared app
    /// config file, leaving other sections untouched. Silently ignores I/O
    /// errors (logged at `WARN`).
    pub fn save(&self) {
        let mut cfg = load_app_config();
        cfg.catalog_view = self.clone();
        save_app_config(&cfg);
    }

    /// Builds preferences from the controller's current view state.
    pub fn from_state(filter: &SidebarFilter, sort: SortMethod, grouped: bool,
                      presentation: CatalogPresentation)
                      -> Self {
        let (filter_name, filter_publisher) = match filter {
            SidebarFilter::AllTitles => (Some("AllTitles"), None),
            SidebarFilter::RecentlyUpdated => (Some("RecentlyUpdated"), None),
            SidebarFilter::OnDevice => (Some("OnDevice"), None),
            SidebarFilter::InCloud => (Some("InCloud"), None),
            SidebarFilter::Publisher(name) => (Some("Publisher"), Some(name.to_string())),
            // Collection filters carry a numeric id with no stable name to
            // validate against the library on next launch (unlike
            // Publisher); out of scope for this persistence layer, so they
            // simply aren't remembered.
            SidebarFilter::Collection(..) => (None, None),
        };
        Self { sort: Some(match sort {
                              SortMethod::Title => "Title",
                              SortMethod::Publisher => "Publisher",
                              SortMethod::DateAdded => "DateAdded",
                              SortMethod::PageCount => "PageCount",
                              SortMethod::Custom { .. } => "Custom",
                          }.to_string()),
               grouped: Some(grouped),
               presentation: Some(match presentation {
                                      CatalogPresentation::List => "List",
                                      CatalogPresentation::Thumbs => "Thumbs",
                                      CatalogPresentation::Grid => "Grid",
                                  }.to_string()),
               filter: filter_name.map(str::to_string),
               filter_publisher }
    }

    /// Resolves the persisted sort method, defaulting (with a `WARN` log)
    /// for unrecognized values. Absent values silently default.
    pub fn to_sort(&self) -> SortMethod {
        match self.sort.as_deref() {
            None => SortMethod::default(),
            Some("Title") => SortMethod::Title,
            Some("Publisher") => SortMethod::Publisher,
            Some("DateAdded") => SortMethod::DateAdded,
            Some("PageCount") => SortMethod::PageCount,
            Some(other) => {
                tracing::warn!(value = other,
                               "unrecognized catalog sort preference; using default");
                SortMethod::default()
            }
        }
    }

    /// Resolves the persisted presentation mode, defaulting (with a `WARN`
    /// log) for unrecognized values. Absent values silently default.
    pub fn to_presentation(&self) -> CatalogPresentation {
        match self.presentation.as_deref() {
            None => CatalogPresentation::default(),
            Some("List") => CatalogPresentation::List,
            Some("Thumbs") => CatalogPresentation::Thumbs,
            Some("Grid") => CatalogPresentation::Grid,
            Some(other) => {
                tracing::warn!(value = other,
                               "unrecognized catalog presentation preference; using default");
                CatalogPresentation::default()
            }
        }
    }

    /// Resolves the persisted sidebar filter, defaulting to
    /// [`SidebarFilter::AllTitles`] (with a `WARN` log) for unrecognized
    /// values. Absent values silently default. A `Publisher` filter with no
    /// stored name also falls back to `AllTitles`.
    pub fn to_filter(&self) -> SidebarFilter {
        match self.filter.as_deref() {
            None => SidebarFilter::default(),
            Some("AllTitles") => SidebarFilter::AllTitles,
            Some("RecentlyUpdated") => SidebarFilter::RecentlyUpdated,
            Some("OnDevice") => SidebarFilter::OnDevice,
            Some("InCloud") => SidebarFilter::InCloud,
            Some("Publisher") => match self.filter_publisher.as_deref() {
                Some(name) => SidebarFilter::Publisher(Arc::from(name)),
                None => SidebarFilter::AllTitles,
            },
            Some(other) => {
                tracing::warn!(value = other,
                               "unrecognized catalog filter preference; using default");
                SidebarFilter::AllTitles
            }
        }
    }
}

// ── Wrapper used for TOML round-trips
// ─────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
pub(crate) struct AppConfigFile {
    #[serde(default)]
    file_openers: Vec<FileOpenerEntry>,
    #[serde(default)]
    catalog_view: CatalogViewPrefs,
}

/// Loads the shared app config file from disk, returning defaults if the
/// file is absent or unparseable.
pub(crate) fn load_app_config() -> AppConfigFile {
    let Some(path) = config_path()
    else {
        return AppConfigFile::default();
    };
    let Ok(text) = std::fs::read_to_string(&path)
    else {
        return AppConfigFile::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

/// Saves the shared app config file to disk, creating parent directories if
/// needed. Silently ignores I/O errors.
pub(crate) fn save_app_config(cfg: &AppConfigFile) {
    let Some(path) = config_path()
    else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = toml::to_string(cfg) {
        let _ = std::fs::write(path, text);
    }
}

// ── FileOpenerConfig impl
// ─────────────────────────────────────────────────────

impl FileOpenerConfig {
    // ── Persistence
    // ───────────────────────────────────────────────────────────

    /// Loads the config from disk, returning a default if the file is absent or
    /// unparseable.
    pub fn load() -> Self {
        Self { entries: load_app_config().file_openers, }
    }

    /// Saves the config to disk, creating parent directories if needed.
    ///
    /// Silently ignores I/O errors.
    pub fn save(&self) {
        let mut cfg = load_app_config();
        cfg.file_openers = self.entries.clone();
        save_app_config(&cfg);
    }

    // ── Read ──────────────────────────────────────────────────────────────────

    /// Returns all entries in the configured order.
    pub fn entries(&self) -> &[FileOpenerEntry] {
        &self.entries
    }

    /// Finds the override app path for `extension`.
    ///
    /// The lookup is case-insensitive and tolerates a leading dot in the input.
    pub fn find_override(&self, extension: &str) -> Option<&Path> {
        let normalized = normalize_ext(extension);
        self.entries
            .iter()
            .find(|e| e.extension == normalized)
            .map(|e| e.app_path.as_path())
    }

    // ── Write ─────────────────────────────────────────────────────────────────

    /// Adds `entry`, normalizing the extension.  Replaces an existing entry for
    /// the same extension.  Returns whether the entry was new or a replacement.
    pub fn add(&mut self, entry: FileOpenerEntry) -> AddOutcome {
        let normalized = FileOpenerEntry { extension: normalize_ext(&entry.extension),
                                           app_path:  entry.app_path, };
        if let Some(existing) = self.entries
                                    .iter_mut()
                                    .find(|e| e.extension == normalized.extension)
        {
            existing.app_path = normalized.app_path;
            return AddOutcome::Replaced;
        }
        self.entries.push(normalized);
        AddOutcome::Added
    }

    /// Removes the entry whose extension matches `extension` (case-insensitive,
    /// dot-tolerant).
    pub fn remove(&mut self, extension: &str) {
        let normalized = normalize_ext(extension);
        self.entries.retain(|e| e.extension != normalized);
    }

    /// Updates the `app_path` of an existing entry.  No-op if the extension is
    /// not found.
    pub fn update_app_path(&mut self, extension: &str, new_path: PathBuf) {
        let normalized = normalize_ext(extension);
        if let Some(entry) = self.entries.iter_mut().find(|e| e.extension == normalized) {
            entry.app_path = new_path;
        }
    }

    // ── Validation
    // ────────────────────────────────────────────────────────────

    /// Returns entries whose `app_path` does not exist on disk.
    pub fn validate_all(&self) -> Vec<&FileOpenerEntry> {
        self.entries
            .iter()
            .filter(|e| !e.app_path.exists())
            .collect()
    }
}

// ── Helpers
// ───────────────────────────────────────────────────────────────────

/// Trims whitespace, strips a leading dot, and lower-cases the extension
/// string.
fn normalize_ext(ext: &str) -> String {
    ext.trim().trim_start_matches('.').to_lowercase()
}

/// Returns the path to `~/.config/dtrpg/app_config.toml` (macOS/Linux) or
/// `%APPDATA%\dtrpg\app_config.toml` (Windows).
pub(crate) fn config_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var("APPDATA").ok().map(PathBuf::from)?;
    #[cfg(not(target_os = "windows"))]
    let base = std::env::var("HOME").ok()
                                    .map(|h| PathBuf::from(h).join(".config"))?;
    Some(base.join("dtrpg").join("app_config.toml"))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ext: &str, path: &str) -> FileOpenerEntry {
        FileOpenerEntry { extension: ext.to_owned(),
                          app_path:  PathBuf::from(path), }
    }

    fn config_with(entries: Vec<FileOpenerEntry>) -> FileOpenerConfig {
        FileOpenerConfig { entries }
    }

    // ── find_override
    // ─────────────────────────────────────────────────────────

    #[test]
    fn find_override_exact_match() {
        let cfg = config_with(vec![entry("pdf", "/Applications/Preview.app")]);
        assert_eq!(cfg.find_override("pdf"),
                   Some(Path::new("/Applications/Preview.app")));
    }

    #[test]
    fn find_override_case_insensitive() {
        let cfg = config_with(vec![entry("pdf", "/Applications/Preview.app")]);
        assert_eq!(cfg.find_override("PDF"),
                   Some(Path::new("/Applications/Preview.app")));
        assert_eq!(cfg.find_override("Pdf"),
                   Some(Path::new("/Applications/Preview.app")));
    }

    #[test]
    fn find_override_dot_prefixed_input() {
        let cfg = config_with(vec![entry("pdf", "/Applications/Preview.app")]);
        assert_eq!(cfg.find_override(".pdf"),
                   Some(Path::new("/Applications/Preview.app")));
        assert_eq!(cfg.find_override(".PDF"),
                   Some(Path::new("/Applications/Preview.app")));
    }

    #[test]
    fn find_override_no_match() {
        let cfg = config_with(vec![entry("pdf", "/Applications/Preview.app")]);
        assert_eq!(cfg.find_override("epub"), None);
        assert_eq!(cfg.find_override(""), None);
    }

    // ── add ───────────────────────────────────────────────────────────────────

    #[test]
    fn add_new_entry() {
        let mut cfg = FileOpenerConfig::default();
        let result = cfg.add(entry("pdf", "/Applications/Preview.app"));
        assert_eq!(result, AddOutcome::Added);
        assert_eq!(cfg.entries().len(), 1);
        assert_eq!(cfg.entries()[0].extension, "pdf");
    }

    #[test]
    fn add_normalizes_extension() {
        let mut cfg = FileOpenerConfig::default();
        cfg.add(entry(".PDF", "/Applications/Preview.app"));
        assert_eq!(cfg.entries()[0].extension, "pdf");
    }

    #[test]
    fn add_trims_whitespace_in_extension() {
        let mut cfg = FileOpenerConfig::default();
        cfg.add(entry("  pdf  ", "/Applications/Preview.app"));
        assert_eq!(cfg.entries()[0].extension, "pdf");
    }

    #[test]
    fn add_duplicate_replaces() {
        let mut cfg = FileOpenerConfig::default();
        cfg.add(entry("pdf", "/Applications/Preview.app"));
        let result = cfg.add(entry("pdf", "/Applications/Acrobat.app"));
        assert_eq!(result, AddOutcome::Replaced);
        assert_eq!(cfg.entries().len(), 1);
        assert_eq!(cfg.entries()[0].app_path,
                   PathBuf::from("/Applications/Acrobat.app"));
    }

    // ── validate_all
    // ──────────────────────────────────────────────────────────

    #[test]
    fn validate_all_flags_missing_paths() {
        let cfg = config_with(vec![entry("pdf", "/nonexistent/Preview.app"),
                                   entry("epub", "/nonexistent/Calibre.app"),]);
        let stale = cfg.validate_all();
        assert_eq!(stale.len(), 2);
    }

    #[test]
    fn validate_all_accepts_existing_paths() {
        let tmp = std::env::temp_dir();
        let cfg = config_with(vec![entry("pdf", tmp.to_str().unwrap_or("/tmp"))]);
        let stale = cfg.validate_all();
        assert_eq!(stale.len(), 0);
    }
}
