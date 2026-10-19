//! Content summary models and provider contracts.

use thiserror::Error;

/// A persisted summary for one catalog item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentSummary {
    /// Stable catalog item id.
    pub item_id:     String,
    /// Summary text shown to callers.
    pub text:        String,
    /// Provider identifier.
    pub model:       String,
    /// Unix timestamp when the summary was generated.
    pub created_at:  i64,
    /// Approximate number of whitespace-delimited tokens in `text`.
    pub token_count: usize,
}

/// Errors returned by summary providers.
#[derive(Debug, Error)]
pub enum SummaryError {
    /// The source text contained no usable content.
    #[error("cannot summarize empty content")]
    EmptyContent,
    /// A provider failed while generating a summary.
    #[error("summary provider failed: {0}")]
    Provider(String),
}

/// Provider boundary for externally generated summaries.
pub trait SummaryProvider: Send + Sync {
    /// Generates summary text for `content`.
    fn summarize(&self, content: &str) -> Result<String, SummaryError>;

    /// Returns the provider identifier stored with generated summaries.
    fn model(&self) -> &str;
}

/// A deterministic local extractive provider for offline use and tests.
#[derive(Debug, Clone, Copy)]
pub struct LocalExtractiveSummary {
    /// Maximum number of sentences to retain.
    pub max_sentences: usize,
    /// Maximum number of characters in the result.
    pub max_chars:     usize,
}

impl Default for LocalExtractiveSummary {
    fn default() -> Self {
        Self { max_sentences: 3,
               max_chars:     600, }
    }
}

impl SummaryProvider for LocalExtractiveSummary {
    fn summarize(&self, content: &str) -> Result<String, SummaryError> {
        let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.is_empty() {
            return Err(SummaryError::EmptyContent);
        }
        let sentences: Vec<String> = normalized.split_inclusive(['.', '!', '?'])
                                               .filter(|sentence| !sentence.trim().is_empty())
                                               .take(self.max_sentences)
                                               .map(str::trim)
                                               .map(str::to_string)
                                               .collect();
        let candidate = if sentences.is_empty() {
            normalized.as_str()
        }
        else {
            &sentences.join(" ")
        };
        let mut summary = candidate.chars().take(self.max_chars).collect::<String>();
        if summary.len() < candidate.len() {
            summary.push_str("...");
        }
        Ok(summary)
    }

    fn model(&self) -> &str {
        "local-extractive"
    }
}

/// Builds a persisted summary using `provider` and the supplied timestamp.
pub fn generate_summary(item_id: impl Into<String>, content: &str,
                        provider: &dyn SummaryProvider, created_at: i64)
                        -> Result<ContentSummary, SummaryError> {
    let text = provider.summarize(content)?;
    let token_count = text.split_whitespace().count();
    Ok(ContentSummary { item_id: item_id.into(),
                        text,
                        model: provider.model().to_string(),
                        created_at,
                        token_count })
}

#[cfg(test)]
// These tests use unwrap for values whose fixtures are known to be valid.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn local_provider_normalizes_and_limits_sentences() {
        let provider = LocalExtractiveSummary { max_sentences: 2,
                                                max_chars:     200, };
        let summary = provider.summarize("  First sentence.   Second sentence! Third sentence? ")
                              .unwrap();
        assert_eq!(summary, "First sentence. Second sentence!");
    }

    #[test]
    fn local_provider_rejects_empty_content() {
        assert!(matches!(LocalExtractiveSummary::default().summarize("  \n"),
                         Err(SummaryError::EmptyContent)));
    }

    #[test]
    fn generated_summary_records_provider_metadata() {
        let summary = generate_summary("item-1",
                                       "A short source.",
                                       &LocalExtractiveSummary::default(),
                                       1_700_000_000).unwrap();
        assert_eq!(summary.item_id, "item-1");
        assert_eq!(summary.model, "local-extractive");
        assert_eq!(summary.token_count, 3);
    }
}
