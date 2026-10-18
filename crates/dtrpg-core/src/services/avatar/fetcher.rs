//! Cache-first avatar fetching with retries.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use super::{AvatarFetchError, AvatarTransport};
use crate::{
    data::{
        avatar::gravatar_url,
        constants::{
            IMAGE_CACHE_MAX_ATTEMPTS, IMAGE_CACHE_RETRY_BASE_DELAY_SECS,
            IMAGE_CACHE_RETRY_MAX_DELAY_SECS,
        },
    },
    services::retry::{RetryConfig, retry_with_backoff},
};

/// Fetches a user's avatar, reading the on-disk cache before the network and
/// saving what it downloads.
pub struct AvatarFetcher {
    transport:  Arc<dyn AvatarTransport>,
    cache_path: PathBuf,
    retry:      RetryConfig,
}

impl AvatarFetcher {
    /// Creates a fetcher that caches the avatar in the file `cache_path`.
    #[must_use]
    pub fn new(transport: Arc<dyn AvatarTransport>, cache_path: PathBuf) -> Self {
        let retry = RetryConfig { max_attempts: IMAGE_CACHE_MAX_ATTEMPTS,
                                  base_secs:    IMAGE_CACHE_RETRY_BASE_DELAY_SECS,
                                  max_secs:     IMAGE_CACHE_RETRY_MAX_DELAY_SECS, };
        Self { transport,
               cache_path,
               retry }
    }

    /// Replaces the retry policy, which defaults to the one cover images use.
    #[must_use]
    pub const fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// Returns the avatar image bytes for `email`, or `None` if there is none.
    ///
    /// The disk cache is read first, so an avatar fetched in an earlier session
    /// costs no request. Otherwise the avatar is fetched from Gravatar, with
    /// retries for transient failures, and saved to the cache. `None` means
    /// the request failed, timed out, or the server had no avatar for the
    /// email (a non-success status, including the `404` for an unregistered
    /// email).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::Arc;
    ///
    /// use dtrpg_core::services::{
    ///     avatar::{AvatarFetcher, HttpAvatarTransport},
    ///     network_monitor::NetworkMonitor,
    /// };
    ///
    /// let transport = HttpAvatarTransport::new(Arc::new(NetworkMonitor::new()));
    /// let fetcher = AvatarFetcher::new(Arc::new(transport), std::env::temp_dir().join("avatar"));
    /// let bytes = fetcher.fetch("someone@example.com");
    /// # let _ = bytes;
    /// ```
    #[must_use]
    pub fn fetch(&self, email: &str) -> Option<Vec<u8>> {
        if let Some(cached) = load_cached(&self.cache_path) {
            return Some(cached);
        }
        if !self.transport.is_reachable() {
            tracing::debug!("avatar fetch skipped: Gravatar endpoint unreachable");
            return None;
        }

        let url = gravatar_url(email);
        let mut on_retry = |attempt: u32, delay: Duration, reason: &AvatarFetchError| {
            if let AvatarFetchError::Transient(reason) = reason {
                tracing::debug!(url = %url, attempt, delay_secs = delay.as_secs(), %reason,
                                "avatar fetch retrying");
            }
        };
        let result = retry_with_backoff(self.retry,
                                        &AtomicBool::new(false),
                                        || self.transport.get(&url),
                                        |error| matches!(error, AvatarFetchError::Transient(_)),
                                        |_| None,
                                        || AvatarFetchError::Final,
                                        Some(&mut on_retry));
        match result {
            Ok(bytes) => {
                save_cached(&self.cache_path, &bytes);
                Some(bytes)
            }
            Err(AvatarFetchError::Transient(reason)) => {
                tracing::warn!(reason = %reason, "avatar fetch failed after retries");
                None
            }
            Err(AvatarFetchError::Final) => None,
        }
    }
}

/// Reads the cached avatar, treating a missing or empty file as no avatar.
fn load_cached(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() { None } else { Some(bytes) }
}

/// Writes the avatar to the cache. A failure only costs a repeat request next
/// time, so it is logged and not reported.
fn save_cached(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent()
       && let Err(error) = std::fs::create_dir_all(parent)
    {
        tracing::warn!("avatar cache: failed to create dir: {error}");
        return;
    }
    if let Err(error) = std::fs::write(path, bytes) {
        tracing::warn!("avatar cache: failed to write {}: {error}", path.display());
    }
}
