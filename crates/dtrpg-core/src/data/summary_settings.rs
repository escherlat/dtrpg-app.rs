//! Persisted preferences for content-summary generation: which provider and
//! model to use, the monthly token ceiling, and whether to summarize
//! automatically on download. These are the Summaries fields in Settings >
//! Agent & MCP.
//!
//! Backed by `{app_preferences_dir}/summaries.toml`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::data::paths::app_preferences_dir;

/// Which summary provider is configured. `Anthropic` needs an API key
/// (stored separately in the platform keyring, not here); `Local` never
/// does.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SummaryProviderKind {
    /// The deterministic local extractive summarizer
    /// ([`crate::data::summary::LocalExtractiveSummary`]). Free, offline,
    /// lower quality.
    #[default]
    Local,
    /// Anthropic's Messages API, using the user's own API key.
    Anthropic,
}

fn default_model() -> String {
    "claude-sonnet-5".to_string()
}

fn default_monthly_token_ceiling() -> u64 {
    500_000
}

/// The preferences as stored. Missing fields take the same defaults as a
/// fresh install, so a file written by an older version keeps working.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SummarySettingsFile {
    /// Which provider generates new summaries.
    pub provider:                   SummaryProviderKind,
    /// Model identifier passed to the provider (ignored by `Local`).
    pub model:                      String,
    /// Maximum tokens [`crate::data::summary::ContentSummary::token_count`]
    /// may sum to in a calendar month before generation stops. Manual and
    /// automatic generation both stop once reached; existing summaries are
    /// unaffected.
    pub monthly_token_ceiling:      u64,
    /// Whether a summary is generated automatically when an item finishes
    /// downloading. Off by default: every summary spends tokens, so this is
    /// an opt-in, not an opt-out.
    pub auto_summarize_on_download: bool,
}

impl Default for SummarySettingsFile {
    fn default() -> Self {
        Self { provider:                   SummaryProviderKind::default(),
               model:                      default_model(),
               monthly_token_ceiling:      default_monthly_token_ceiling(),
               auto_summarize_on_download: false, }
    }
}

/// Persists and restores the content-summary preferences.
pub struct SummarySettings {
    /// Where the preferences are saved, or `None` if they are never
    /// written.
    path: Option<PathBuf>,
    data: SummarySettingsFile,
}

fn settings_path() -> PathBuf {
    app_preferences_dir().join("summaries.toml")
}

impl SummarySettings {
    /// Loads the preferences from disk; returns the defaults on any error.
    ///
    /// Loading never writes: the file is created by the first choice saved.
    /// A file that exists but fails to parse is left untouched.
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(&settings_path())
    }

    /// Loads the preferences from `path`, as [`Self::load`] does.
    #[must_use]
    pub fn load_from(path: &Path) -> Self {
        let data =
            std::fs::read_to_string(path).ok()
                                         .and_then(|text| {
                                             toml::from_str::<SummarySettingsFile>(&text).ok()
                                         })
                                         .unwrap_or_default();
        Self { path: Some(path.to_owned()),
               data }
    }

    /// The defaults, never written to disk, for tests and previews.
    #[must_use]
    pub fn in_memory() -> Self {
        Self { path: None,
               data: SummarySettingsFile::default(), }
    }

    /// The preferences as they are now.
    #[must_use]
    pub fn snapshot(&self) -> SummarySettingsFile {
        self.data.clone()
    }

    /// Persists the configured provider.
    pub fn save_provider(&mut self, provider: SummaryProviderKind) {
        self.data.provider = provider;
        self.flush();
    }

    /// Persists the configured model identifier.
    pub fn save_model(&mut self, model: impl Into<String>) {
        self.data.model = model.into();
        self.flush();
    }

    /// Persists the configured monthly token ceiling.
    pub fn save_monthly_token_ceiling(&mut self, ceiling: u64) {
        self.data.monthly_token_ceiling = ceiling;
        self.flush();
    }

    /// Persists whether summaries generate automatically on download.
    pub fn save_auto_summarize_on_download(&mut self, enabled: bool) {
        self.data.auto_summarize_on_download = enabled;
        self.flush();
    }

    /// Writes the preferences to disk, creating the directory if needed. A
    /// failure is logged and otherwise ignored, as a preference is not
    /// worth failing for.
    fn flush(&self) {
        let Some(path) = &self.path
        else {
            return;
        };
        let result =
            toml::to_string_pretty(&self.data).map_err(|error| error.to_string())
                                              .and_then(|text| {
                                                  if let Some(dir) = path.parent() {
                                                      std::fs::create_dir_all(dir).map_err(|e| {
                                                                                      e.to_string()
                                                                                  })?;
                                                  }
                                                  std::fs::write(path, text).map_err(|e| {
                                                                                e.to_string()
                                                                            })
                                              });
        if let Err(error) = result {
            tracing::warn!(%error, path = %path.display(), "could not save summary settings");
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn in_temp_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings").join("summaries.toml");
        (dir, path)
    }

    #[test]
    fn defaults_are_local_provider_and_auto_summarize_off() {
        let (_dir, path) = in_temp_dir();

        let settings = SummarySettings::load_from(&path);

        assert_eq!(settings.snapshot().provider, SummaryProviderKind::Local);
        assert!(!settings.snapshot().auto_summarize_on_download);
        assert_eq!(settings.snapshot().monthly_token_ceiling, 500_000);
        assert!(!path.exists());
    }

    #[test]
    fn a_saved_choice_is_there_after_loading_again() {
        let (_dir, path) = in_temp_dir();
        let mut settings = SummarySettings::load_from(&path);

        settings.save_provider(SummaryProviderKind::Anthropic);
        settings.save_model("claude-opus-5");
        settings.save_monthly_token_ceiling(250_000);
        settings.save_auto_summarize_on_download(true);
        let reloaded = SummarySettings::load_from(&path);

        let snapshot = reloaded.snapshot();
        assert_eq!(snapshot.provider, SummaryProviderKind::Anthropic);
        assert_eq!(snapshot.model, "claude-opus-5");
        assert_eq!(snapshot.monthly_token_ceiling, 250_000);
        assert!(snapshot.auto_summarize_on_download);
    }

    #[test]
    fn a_field_missing_from_the_file_takes_its_default() {
        let (_dir, path) = in_temp_dir();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "provider = \"anthropic\"\n").unwrap();

        let settings = SummarySettings::load_from(&path);

        let snapshot = settings.snapshot();
        assert_eq!(snapshot.provider, SummaryProviderKind::Anthropic);
        assert_eq!(snapshot.model, default_model());
        assert_eq!(snapshot.monthly_token_ceiling,
                   default_monthly_token_ceiling());
        assert!(!snapshot.auto_summarize_on_download);
    }

    #[test]
    fn a_file_that_cannot_be_read_gives_the_defaults_and_is_left_alone() {
        let (_dir, path) = in_temp_dir();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "this is = not [ valid").unwrap();

        let settings = SummarySettings::load_from(&path);

        assert_eq!(settings.snapshot().provider, SummaryProviderKind::Local);
        assert_eq!(std::fs::read_to_string(&path).unwrap(),
                   "this is = not [ valid");
    }

    #[test]
    fn settings_that_are_in_memory_are_never_written() {
        let mut settings = SummarySettings::in_memory();

        settings.save_provider(SummaryProviderKind::Anthropic);

        assert_eq!(settings.snapshot().provider, SummaryProviderKind::Anthropic);
    }
}
