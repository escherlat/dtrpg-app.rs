//! Avatar image fetching: the on-disk cache first, then the network, with
//! retries.
//!
//! The network sits behind [`AvatarTransport`], so the fetch logic can be
//! tested without HTTP and callers outside the core never touch `reqwest`.

mod error;
mod fetcher;
mod transport;

#[cfg(test)]
mod tests;

pub use error::AvatarFetchError;
pub use fetcher::AvatarFetcher;
pub use transport::{AvatarTransport, HttpAvatarTransport};
