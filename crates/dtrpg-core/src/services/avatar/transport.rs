//! How avatar bytes are fetched over the network.

use std::{sync::Arc, time::Duration};

use reqwest::blocking::Client;

use super::AvatarFetchError;
use crate::{data::constants::GRAVATAR_HOST, services::network_monitor::NetworkMonitor};

/// How long one avatar request may take before it counts as a transient
/// failure.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// The network side of avatar fetching.
pub trait AvatarTransport: Send + Sync {
    /// Whether the avatar host looks reachable right now. Checked once before
    /// a fetch, so an offline device fails fast instead of retrying.
    fn is_reachable(&self) -> bool;

    /// Fetches `url` once, with no retries.
    ///
    /// # Errors
    ///
    /// Returns [`AvatarFetchError::Transient`] if the request fails in a way
    /// worth retrying, and [`AvatarFetchError::Final`] if the server answers
    /// with a non-success status.
    fn get(&self, url: &str) -> Result<Vec<u8>, AvatarFetchError>;
}

/// An [`AvatarTransport`] that uses blocking HTTP.
///
/// Blocking HTTP is safe to call from a background thread that is not inside
/// an async context.
pub struct HttpAvatarTransport {
    monitor: Arc<NetworkMonitor>,
}

impl HttpAvatarTransport {
    /// Creates a transport that asks `monitor` whether Gravatar is reachable.
    #[must_use]
    pub const fn new(monitor: Arc<NetworkMonitor>) -> Self {
        Self { monitor }
    }
}

impl AvatarTransport for HttpAvatarTransport {
    fn is_reachable(&self) -> bool {
        self.monitor.check_endpoint(GRAVATAR_HOST)
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, AvatarFetchError> {
        let client = Client::builder().timeout(REQUEST_TIMEOUT)
                                      .build()
                                      .map_err(|error| AvatarFetchError::Transient(error.to_string()))?;
        get_bytes(&client, url)
    }
}

/// Fetches `url` with `client`, returning the body bytes.
pub(super) fn get_bytes(client: &Client, url: &str) -> Result<Vec<u8>, AvatarFetchError> {
    let response = client.get(url)
                         .send()
                         .map_err(|error| AvatarFetchError::Transient(error.to_string()))?;
    if !response.status().is_success() {
        return Err(AvatarFetchError::Final);
    }
    response.bytes()
            .map(|body| body.to_vec())
            .map_err(|error| AvatarFetchError::Transient(error.to_string()))
}
