//! [`SummaryProvider`] backed by Anthropic's Messages API, using the user's
//! own API key.

use serde::{Deserialize, Serialize};

use crate::data::summary::{SummaryError, SummaryProvider};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Maximum characters of source content sent in the prompt. Anthropic bills
/// (and bounds context) by tokens, not characters, but this is a simple,
/// conservative cap that keeps a single request well within any current
/// model's context window regardless of model choice.
const MAX_CONTENT_CHARS: usize = 60_000;

const MAX_RESPONSE_TOKENS: u32 = 512;

fn summary_prompt(content: &str) -> String {
    let truncated: String = content.chars().take(MAX_CONTENT_CHARS).collect();
    format!("Write a concise, neutral summary (3-5 sentences) of what the following RPG \
            product covers -- its subject, setting, and what a reader would find inside. Do \
            not include a title or preamble, just the summary text.\n\n{truncated}")
}

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model:      &'a str,
    max_tokens: u32,
    messages:   Vec<MessageIn<'a>>,
}

#[derive(Serialize)]
struct MessageIn<'a> {
    role:    &'a str,
    content: String,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    error: ApiErrorDetail,
}

#[derive(Deserialize)]
struct ApiErrorDetail {
    message: String,
}

/// Summarizes content via Anthropic's Messages API.
pub struct AnthropicSummary {
    api_key: String,
    model:   String,
}

impl AnthropicSummary {
    /// Creates a provider that authenticates with `api_key` and requests
    /// `model` for each summary.
    #[must_use]
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self { api_key: api_key.into(), model: model.into() }
    }
}

impl SummaryProvider for AnthropicSummary {
    fn summarize(&self, content: &str) -> Result<String, SummaryError> {
        if content.trim().is_empty() {
            return Err(SummaryError::EmptyContent);
        }

        let body = MessagesRequest { model:      &self.model,
                                     max_tokens: MAX_RESPONSE_TOKENS,
                                     messages:   vec![MessageIn { role:    "user",
                                                                  content: summary_prompt(
                                                                      content,
                                                                  ), }], };

        let client = reqwest::blocking::Client::new();
        let response = client.post(API_URL)
                             .header("x-api-key", &self.api_key)
                             .header("anthropic-version", ANTHROPIC_VERSION)
                             .json(&body)
                             .send()
                             .map_err(|e| SummaryError::Provider(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let message = response.json::<ApiErrorBody>()
                                  .map(|b| b.error.message)
                                  .unwrap_or_else(|_| status.to_string());
            return Err(SummaryError::Provider(format!("Anthropic API error ({status}): \
                                                       {message}")));
        }

        let parsed: MessagesResponse =
            response.json().map_err(|e| SummaryError::Provider(e.to_string()))?;
        let text = parsed.content
                         .into_iter()
                         .map(|block| block.text)
                         .collect::<Vec<_>>()
                         .join("")
                         .trim()
                         .to_string();
        if text.is_empty() {
            return Err(SummaryError::Provider("Anthropic returned an empty response".to_string()));
        }
        Ok(text)
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_content_without_a_network_call() {
        let provider = AnthropicSummary::new("test-key", "claude-sonnet-5");
        assert!(matches!(provider.summarize("   "), Err(SummaryError::EmptyContent)));
    }

    #[test]
    fn prompt_truncates_very_long_content() {
        let long = "x".repeat(MAX_CONTENT_CHARS + 1_000);
        let prompt = summary_prompt(&long);
        // The prompt's own instructional text plus exactly MAX_CONTENT_CHARS
        // of source content, not the full input.
        assert!(prompt.len() < long.len());
    }

    #[test]
    fn model_returns_the_configured_model() {
        let provider = AnthropicSummary::new("test-key", "claude-opus-5");
        assert_eq!(provider.model(), "claude-opus-5");
    }
}
