//! How cover bytes are fetched over the network.

use std::{sync::Arc, time::Duration};

use reqwest::{blocking::Client, header::RETRY_AFTER};

use super::CoverFetchError;
use crate::{data::constants::DTRPG_API_HOST, services::network_monitor::NetworkMonitor};

/// The network side of cover fetching.
pub trait CoverTransport: Send + Sync {
    /// Whether the cover host looks reachable right now. Checked once before
    /// a fetch, so an offline device fails fast instead of retrying.
    fn is_reachable(&self) -> bool;

    /// Fetches `url` once, with no retries.
    ///
    /// # Errors
    ///
    /// Returns a [`CoverFetchError`] if the request fails, or if the server
    /// answers HTTP 429 (with its `Retry-After` wait, when it sent one).
    fn get(&self, url: &str) -> Result<Vec<u8>, CoverFetchError>;
}

/// A [`CoverTransport`] that uses blocking HTTP.
///
/// Blocking HTTP is safe to call from a work-queue thread, which is not inside
/// an async context.
pub struct HttpCoverTransport {
    monitor: Arc<NetworkMonitor>,
}

impl HttpCoverTransport {
    /// Creates a transport that asks `monitor` whether the API host is
    /// reachable.
    #[must_use]
    pub const fn new(monitor: Arc<NetworkMonitor>) -> Self {
        Self { monitor }
    }
}

impl CoverTransport for HttpCoverTransport {
    fn is_reachable(&self) -> bool {
        self.monitor.check_endpoint(DTRPG_API_HOST)
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, CoverFetchError> {
        let client = Client::builder().build()
                                      .map_err(|error| CoverFetchError::new(error.to_string()))?;
        get_bytes(&client, url)
    }
}

/// Fetches `url` with `client`, returning the body bytes.
pub(super) fn get_bytes(client: &Client, url: &str) -> Result<Vec<u8>, CoverFetchError> {
    let response = client.get(url)
                         .send()
                         .map_err(|error| CoverFetchError::new(error.to_string()))?;
    if response.status().as_u16() == 429 {
        let retry_after = response.headers()
                                  .get(RETRY_AFTER)
                                  .and_then(|value| value.to_str().ok())
                                  .and_then(|value| value.trim().parse::<u64>().ok())
                                  .map(Duration::from_secs);
        return Err(CoverFetchError { message: "rate limited (HTTP 429)".to_owned(),
                                     retry_after });
    }
    let body = response.bytes()
                       .map_err(|error| CoverFetchError::new(error.to_string()))?;
    Ok(body.to_vec())
}
