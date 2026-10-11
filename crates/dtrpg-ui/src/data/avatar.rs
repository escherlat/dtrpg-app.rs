//! Gravatar URL computation and avatar image fetching.

use std::path::PathBuf;
use std::time::Duration;

use crate::data::constants::{
    AVATAR_CACHE_FILE, GRAVATAR_HOST, IMAGE_CACHE_MAX_ATTEMPTS, IMAGE_CACHE_RETRY_BASE_DELAY_SECS,
    IMAGE_CACHE_RETRY_MAX_DELAY_SECS,
};
use crate::data::paths::app_cache_dir;
use crate::services::network_monitor::NetworkMonitor;
use crate::services::retry::{RetryConfig, retry_with_backoff};

/// Internal fetch-failure classification: only [`AvatarFetchError::Transient`]
/// is retried. A non-success HTTP status (e.g. `404` for an unregistered
/// Gravatar email) retrying wouldn't fix, so it's treated as final.
#[derive(Debug)]
enum AvatarFetchError {
    Transient(String),
    Final,
}

fn avatar_cache_path() -> PathBuf {
    app_cache_dir().join(AVATAR_CACHE_FILE)
}

/// Returns `true` if an avatar image is currently cached on disk.
#[must_use]
pub fn avatar_cached() -> bool {
    avatar_cache_path().exists()
}

fn load_cached_avatar() -> Option<Vec<u8>> {
    let bytes = std::fs::read(avatar_cache_path()).ok()?;
    if bytes.is_empty() { None } else { Some(bytes) }
}

fn save_cached_avatar(bytes: &[u8]) {
    let path = avatar_cache_path();
    if let Some(parent) = path.parent()
       && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::warn!("avatar cache: failed to create dir: {e}");
        return;
    }
    if let Err(e) = std::fs::write(&path, bytes) {
        tracing::warn!("avatar cache: failed to write {}: {e}", path.display());
    }
}

/// Computes a Gravatar avatar URL for the given email address.
///
/// Trims, lowercases, and MD5-hashes the email per the Gravatar specification.
/// `d=404` returns HTTP 404 for unregistered emails, allowing callers to detect
/// missing avatars rather than receiving a generic fallback image.
pub fn gravatar_url(email: &str) -> String {
    let hash = format!("{:x}", md5::compute(email.trim().to_lowercase()));
    format!("https://www.gravatar.com/avatar/{hash}?d=404&s=64")
}

/// Fetches raw avatar image bytes for the given email.
///
/// Checks the disk cache first; returns cached bytes if available. On a cache
/// miss, fetches from Gravatar, writes the result to disk, and returns the
/// bytes.
///
/// Uses a blocking HTTP client — safe to call from gpui background executor
/// threads, which do not run a Tokio reactor. Returns `None` if the request
/// fails, times out, or the server returns a non-success status (including 404
/// for unregistered emails).
pub fn fetch_avatar_bytes(email: String) -> Option<Vec<u8>> {
    if let Some(cached) = load_cached_avatar() {
        return Some(cached);
    }

    let monitor = NetworkMonitor::new();
    if !monitor.check_endpoint(GRAVATAR_HOST) {
        tracing::debug!("avatar fetch skipped: Gravatar endpoint unreachable");
        return None;
    }

    let url = gravatar_url(&email);
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let config = RetryConfig { max_attempts: IMAGE_CACHE_MAX_ATTEMPTS,
                               base_secs:    IMAGE_CACHE_RETRY_BASE_DELAY_SECS,
                               max_secs:     IMAGE_CACHE_RETRY_MAX_DELAY_SECS, };
    let mut on_retry = |attempt: u32, delay: Duration, reason: &AvatarFetchError| {
        if let AvatarFetchError::Transient(reason) = reason {
            tracing::debug!(url = %url,
                            attempt,
                            delay_secs = delay.as_secs(),
                            %reason,
                            "avatar fetch retrying");
        }
    };
    let result = retry_with_backoff(config,
                                    &cancel,
                                    || -> Result<Vec<u8>, AvatarFetchError> {
                                        let client =
                                            reqwest::blocking::Client::builder()
                                                .timeout(Duration::from_secs(5))
                                                .build()
                                                .map_err(|e| {
                                                    AvatarFetchError::Transient(e.to_string())
                                                })?;
                                        let response = client.get(&url).send().map_err(|e| {
                                            AvatarFetchError::Transient(e.to_string())
                                        })?;
                                        if !response.status().is_success() {
                                            return Err(AvatarFetchError::Final);
                                        }
                                        response.bytes()
                                                .map(|b| b.to_vec())
                                                .map_err(|e| {
                                                    AvatarFetchError::Transient(e.to_string())
                                                })
                                    },
                                    |e| matches!(e, AvatarFetchError::Transient(_)),
                                    |_| None,
                                    || AvatarFetchError::Final,
                                    Some(&mut on_retry));

    match result {
        Ok(bytes) => {
            save_cached_avatar(&bytes);
            Some(bytes)
        }
        Err(AvatarFetchError::Transient(reason)) => {
            tracing::warn!(reason = %reason, "avatar fetch failed after retries");
            None
        }
        Err(AvatarFetchError::Final) => None,
    }
}
