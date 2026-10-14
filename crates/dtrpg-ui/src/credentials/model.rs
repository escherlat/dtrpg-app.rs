//! Credential value type and error enum.

use thiserror::Error;

/// A single credential entry scoped to a service + account pair.
///
/// The `service` and `account` fields identify the keyring entry; `secret`
/// carries the sensitive value (application key). `email` carries the
/// account email stored alongside the key so the sign-in form can pre-fill it.
#[derive(Clone)]
pub struct Credential {
    /// Reverse-DNS service name, e.g. `com.pilgrimagesoftware.dtrpg`.
    pub service: String,
    /// Account sub-key distinguishing credential type, e.g. `api-key`.
    pub account: String,
    /// The application key.
    pub secret:  String,
    /// Account email, if known. `None` for legacy entries written before this
    /// field was introduced.
    pub email:   Option<String>,
}

impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
         .field("service", &self.service)
         .field("account", &self.account)
         .field("secret", &"[redacted]")
         .field("email", &self.email)
         .finish()
    }
}

/// Errors returned by [`crate::credentials::CredentialStore`] operations.
#[derive(Debug, Error)]
pub enum CredentialError {
    /// The platform keyring could not store the credential.
    #[error("Failed to store credential ({account}): {reason}")]
    Store {
        /// The account key that failed.
        account: String,
        /// Underlying error description.
        reason:  String,
    },
    /// The platform keyring could not retrieve the credential.
    #[error("Failed to load credential ({account}): {reason}")]
    Load {
        /// The account key that failed.
        account: String,
        /// Underlying error description.
        reason:  String,
    },
    /// The platform keyring could not delete the credential.
    #[error("Failed to delete credential ({account}): {reason}")]
    Delete {
        /// The account key that failed.
        account: String,
        /// Underlying error description.
        reason:  String,
    },
    /// The platform keyring service is not available (e.g. no Secret Service
    /// daemon on Linux).
    #[error("Platform keyring unavailable: {0}")]
    Unavailable(String),
}
