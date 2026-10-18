//! Cache-first cover fetching with retries.

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use super::{CoverFetchError, CoverTransport};
use crate::{
    data::{
        constants::{
            IMAGE_CACHE_MAX_ATTEMPTS, IMAGE_CACHE_RETRY_BASE_DELAY_SECS,
            IMAGE_CACHE_RETRY_MAX_DELAY_SECS,
        },
        cover_cache::{load_cached_cover, save_cached_cover},
    },
    services::retry::{RetryConfig, retry_with_backoff},
};

/// Fetches cover images, reading the on-disk cache before the network and
/// saving what it downloads.
pub struct CoverFetcher {
    transport: Arc<dyn CoverTransport>,
    covers_root: PathBuf,
}

impl CoverFetcher {
    /// Creates a fetcher that caches covers under `covers_root`.
    #[must_use]
    pub fn new(transport: Arc<dyn CoverTransport>, covers_root: PathBuf) -> Self {
        Self { transport, covers_root }
    }

    /// Returns the cover image bytes for `key`, fetching `url` if needed.
    ///
    /// The disk cache is read first, so a cover downloaded in an earlier
    /// session costs no request. `force_network` skips that read, for the
    /// manual "refresh" actions that exist to replace a stale image. A
    /// download is retried with backoff, honoring any `Retry-After` wait, and
    /// is saved to the cache. Setting `cancel` stops further attempts.
    ///
    /// # Errors
    ///
    /// Returns a [`CoverFetchError`] if the host is unreachable, the request
    /// still fails after its retries, or `cancel` is set.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::sync::{Arc, atomic::AtomicBool};
    ///
    /// use dtrpg_core::services::{
    ///     covers::{CoverFetcher, HttpCoverTransport},
    ///     network_monitor::NetworkMonitor,
    /// };
    ///
    /// let transport = HttpCoverTransport::new(Arc::new(NetworkMonitor::new()));
    /// let fetcher = CoverFetcher::new(Arc::new(transport), std::env::temp_dir());
    /// let bytes = fetcher.fetch("item-1", "https://example.com/cover.jpg", false,
    ///                           &AtomicBool::new(false))?;
    /// # let _ = bytes;
    /// # Ok::<(), dtrpg_core::services::covers::CoverFetchError>(())
    /// ```
    pub fn fetch(&self, key: &str, url: &str, force_network: bool, cancel: &AtomicBool)
                 -> Result<Vec<u8>, CoverFetchError> {
        if !force_network && let Some(bytes) = load_cached_cover(&self.covers_root, key) {
            return Ok(bytes);
        }
        if !self.transport.is_reachable() {
            return Err(CoverFetchError::new("DriveThruRPG API endpoint unreachable"));
        }

        let config = RetryConfig { max_attempts: IMAGE_CACHE_MAX_ATTEMPTS,
                                   base_secs: IMAGE_CACHE_RETRY_BASE_DELAY_SECS,
                                   max_secs: IMAGE_CACHE_RETRY_MAX_DELAY_SECS };
        let mut on_retry = |attempt: u32, delay: Duration, reason: &CoverFetchError| {
            tracing::debug!(url, attempt, delay_secs = delay.as_secs(), reason = %reason,
                            "cover thumbnail fetch retrying");
        };
        let bytes = retry_with_backoff(config,
                                       cancel,
                                       || self.transport.get(url),
                                       |_| true,
                                       |error| error.retry_after,
                                       || CoverFetchError::new("cover thumbnail fetch cancelled"),
                                       Some(&mut on_retry))?;
        save_cached_cover(&self.covers_root, key, &bytes);
        Ok(bytes)
    }
}
