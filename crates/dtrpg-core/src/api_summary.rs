//! Content-summary generation exposed by the library facade.

use chrono::Local;
use thiserror::Error;

use crate::api::LibraryApi;
use crate::credentials::{CredentialError, load_anthropic_api_key};
use crate::data::anthropic_summary::AnthropicSummary;
use crate::data::index::IndexError;
use crate::data::storage::{StorageConfig, resolved_file_path};
use crate::data::summary::{
    ContentSummary, LocalExtractiveSummary, SummaryError, SummaryProvider, generate_summary,
};
use crate::data::summary_settings::{SummaryProviderKind, SummarySettingsFile};
use crate::util::datetime::{now_secs, start_of_month_secs};
use crate::util::text_extract::extract_text_for_summary;

/// Errors from generating and saving an item's summary.
#[derive(Debug, Error)]
pub enum SummarizeError {
    /// No item with the given id is in the local index.
    #[error("item {0} not found")]
    ItemNotFound(String),
    /// None of the item's files is downloaded in a format text can be read
    /// from.
    #[error("no downloaded file of this item can be summarized")]
    NoSummarizableFile,
    /// This month's summaries have already used the configured token ceiling.
    #[error("monthly summary token ceiling reached ({used} of {ceiling} tokens used)")]
    CeilingReached {
        /// Tokens spent on summaries so far this month.
        used:    u64,
        /// The configured monthly ceiling.
        ceiling: u64,
    },
    /// The Anthropic provider is selected but no API key has been saved.
    #[error("no Anthropic API key is saved; add one in Settings")]
    MissingApiKey,
    /// The local library index failed.
    #[error(transparent)]
    Index(#[from] IndexError),
    /// The platform keyring failed.
    #[error(transparent)]
    Credential(#[from] CredentialError),
    /// The summary provider failed.
    #[error(transparent)]
    Summary(#[from] SummaryError),
}

/// Builds the provider that `settings` selects.
///
/// # Errors
///
/// Returns [`SummarizeError::MissingApiKey`] if the Anthropic provider is
/// selected and no key is saved, or [`SummarizeError::Credential`] if the
/// keyring cannot be read.
pub fn provider_for(settings: &SummarySettingsFile)
                    -> Result<Box<dyn SummaryProvider>, SummarizeError> {
    match settings.provider {
        SummaryProviderKind::Local => Ok(Box::new(LocalExtractiveSummary::default())),
        SummaryProviderKind::Anthropic => {
            let api_key = load_anthropic_api_key()?.ok_or(SummarizeError::MissingApiKey)?;
            Ok(Box::new(AnthropicSummary::new(api_key, settings.model.clone())))
        }
    }
}

impl LibraryApi {
    /// Generates and saves a summary of `content` for the item `item_id`.
    ///
    /// Refuses once this calendar month's summaries have used
    /// `settings.monthly_token_ceiling` tokens. An existing summary for the
    /// item is replaced.
    ///
    /// # Errors
    ///
    /// Returns [`SummarizeError::CeilingReached`] when the ceiling has been
    /// reached, [`SummarizeError::Summary`] if `provider` fails, and
    /// [`SummarizeError::Index`] if the index cannot be read or written.
    pub fn summarize_text(&self, item_id: &str, content: &str, settings: &SummarySettingsFile,
                          provider: &dyn SummaryProvider)
                          -> Result<ContentSummary, SummarizeError> {
        let month_start = start_of_month_secs(Local::now());
        let used = self.open_index()?.tokens_used_since(month_start)?;
        if used >= settings.monthly_token_ceiling {
            return Err(SummarizeError::CeilingReached { used,
                                                        ceiling:
                                                            settings.monthly_token_ceiling });
        }
        let summary = generate_summary(item_id, content, provider, now_secs())?;
        self.index_save_summary(&summary)?;
        Ok(summary)
    }

    /// Generates and saves a summary of the item `item_id`, reading the text
    /// of the first of its files that is on disk and supported.
    ///
    /// # Errors
    ///
    /// Returns [`SummarizeError::ItemNotFound`] for an unknown item,
    /// [`SummarizeError::NoSummarizableFile`] if no file yields text, and the
    /// errors of [`Self::summarize_text`] otherwise.
    pub fn summarize_item(&self, item_id: &str, storage: &StorageConfig,
                          settings: &SummarySettingsFile, provider: &dyn SummaryProvider)
                          -> Result<ContentSummary, SummarizeError> {
        let item = self.index_get_item(item_id)?
                       .ok_or_else(|| SummarizeError::ItemNotFound(item_id.to_string()))?;
        let content = item.files
                          .iter()
                          .find_map(|file| {
                              let path = resolved_file_path(storage, &item, file);
                              extract_text_for_summary(&path, &file.format)
                          })
                          .ok_or(SummarizeError::NoSummarizableFile)?;
        self.summarize_text(item_id, &content, settings, provider)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::data::enums::ItemStatus;
    use crate::data::index::LibraryIndex;
    use crate::data::library::LibraryItem;
    use crate::services::sdk::{RustSdkCollectionsService, RustSdkLibraryService};

    /// An API over a fresh index holding one item, `item-1`, plus the folder
    /// it lives in.
    fn api() -> (LibraryApi, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let item = LibraryItem::new("item-1",
                                    "Title",
                                    "Publisher",
                                    "",
                                    "Core",
                                    "PDF",
                                    10,
                                    1.0,
                                    2024,
                                    0,
                                    ItemStatus::Cloud,
                                    "#000000",
                                    "",
                                    None);
        LibraryIndex::open_and_migrate(dir.path()).unwrap()
                                                  .replace_all_items(&[item])
                                                  .unwrap();
        let api = LibraryApi::new(Box::new(RustSdkLibraryService::unauthenticated()),
                                  Box::new(RustSdkCollectionsService::unauthenticated()),
                                  dir.path());
        (api, dir)
    }

    #[test]
    fn summarizing_text_saves_the_summary() {
        let (api, _dir) = api();

        let summary = api.summarize_text("item-1",
                                         "A haunted lighthouse. Dark secrets.",
                                         &SummarySettingsFile::default(),
                                         &LocalExtractiveSummary::default())
                         .unwrap();

        assert_eq!(api.index_get_summary("item-1").unwrap(), Some(summary));
    }

    #[test]
    fn summarizing_stops_once_the_monthly_ceiling_is_used() {
        let (api, _dir) = api();
        let settings = SummarySettingsFile { monthly_token_ceiling: 3,
                                             ..SummarySettingsFile::default() };
        let provider = LocalExtractiveSummary::default();
        api.summarize_text("item-1", "One two three four.", &settings, &provider)
           .unwrap();

        let second = api.summarize_text("item-1", "Another summary.", &settings, &provider);

        assert!(matches!(second,
                         Err(SummarizeError::CeilingReached { used:    4,
                                                              ceiling: 3, })));
    }

    #[test]
    fn an_unknown_item_is_reported() {
        let (api, dir) = api();
        let storage = StorageConfig::for_test(dir.path().to_path_buf());

        let result = api.summarize_item("missing",
                                        &storage,
                                        &SummarySettingsFile::default(),
                                        &LocalExtractiveSummary::default());

        assert!(matches!(result, Err(SummarizeError::ItemNotFound(id)) if id == "missing"));
    }

    #[test]
    fn an_item_without_a_readable_file_is_reported() {
        let (api, dir) = api();
        let storage = StorageConfig::for_test(dir.path().to_path_buf());

        let result = api.summarize_item("item-1",
                                        &storage,
                                        &SummarySettingsFile::default(),
                                        &LocalExtractiveSummary::default());

        assert!(matches!(result, Err(SummarizeError::NoSummarizableFile)));
    }

    #[test]
    fn the_local_provider_needs_no_api_key() {
        let provider = provider_for(&SummarySettingsFile::default()).unwrap();

        assert_eq!(provider.model(), "local-extractive");
    }
}
