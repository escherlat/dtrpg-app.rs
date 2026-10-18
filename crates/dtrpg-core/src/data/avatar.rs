//! Gravatar URL computation and avatar image fetching.

use std::{path::PathBuf, sync::Arc};

use crate::{
    data::{constants::AVATAR_CACHE_FILE, paths::app_cache_dir},
    services::{
        avatar::{AvatarFetcher, HttpAvatarTransport},
        network_monitor::NetworkMonitor,
    },
};

fn avatar_cache_path() -> PathBuf {
    app_cache_dir().join(AVATAR_CACHE_FILE)
}

/// Returns `true` if an avatar image is currently cached on disk.
#[must_use]
pub fn avatar_cached() -> bool {
    avatar_cache_path().exists()
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
    let transport = HttpAvatarTransport::new(Arc::new(NetworkMonitor::new()));
    AvatarFetcher::new(Arc::new(transport), avatar_cache_path()).fetch(&email)
}
