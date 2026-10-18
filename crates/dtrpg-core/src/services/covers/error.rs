//! The error a cover fetch reports.

use std::{fmt, time::Duration};

/// Why a cover fetch failed.
///
/// Carries the server's `Retry-After` wait when the failure was an HTTP 429
/// response, so the retry loop can honor it instead of its default backoff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverFetchError {
    /// A description of the failure, suitable for logging as-is.
    pub message:     String,
    /// The wait the server asked for, when the failure was an HTTP 429
    /// response that included a `Retry-After` header.
    pub retry_after: Option<Duration>,
}

impl CoverFetchError {
    /// An error with no server-specified wait.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self { message:     message.into(),
               retry_after: None, }
    }
}

impl fmt::Display for CoverFetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CoverFetchError {}
