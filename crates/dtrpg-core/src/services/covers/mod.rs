//! Cover-image fetching: the on-disk cache first, then the network, with
//! retries.
//!
//! The network sits behind [`CoverTransport`], so the fetch logic can be
//! tested without HTTP and callers outside the core never touch `reqwest`.

mod error;
mod fetcher;
mod transport;

#[cfg(test)]
mod tests;

pub use error::CoverFetchError;
pub use fetcher::CoverFetcher;
pub use transport::{CoverTransport, HttpCoverTransport};
