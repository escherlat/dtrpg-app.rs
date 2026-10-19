//! Keyring storage for the user's own Anthropic API key, used by the
//! content-summary feature ([`crate::data::summary_settings`]).
//!
//! The key lives in its own service namespace
//! ([`KEYRING_ANTHROPIC_SERVICE`]), apart from the DriveThruRPG credentials.

use super::{Credential, CredentialError, CredentialStore, KeyringCredentialStore};
use crate::data::constants::{KEYRING_ANTHROPIC_API_KEY, KEYRING_ANTHROPIC_SERVICE};

fn store() -> KeyringCredentialStore {
    KeyringCredentialStore::new(KEYRING_ANTHROPIC_SERVICE, KEYRING_ANTHROPIC_API_KEY)
}

/// Saves `api_key` as the Anthropic API key, replacing any existing one.
///
/// # Errors
///
/// Returns [`CredentialError`] if the platform keyring rejects the write.
pub fn save_anthropic_api_key(api_key: &str) -> Result<(), CredentialError> {
    store().store(&Credential { service: KEYRING_ANTHROPIC_SERVICE.into(),
                                account: KEYRING_ANTHROPIC_API_KEY.into(),
                                secret:  api_key.into(),
                                email:   None, })
}

/// Loads the saved Anthropic API key, or `None` if none has been saved.
///
/// # Errors
///
/// Returns [`CredentialError`] if the platform keyring cannot be read.
pub fn load_anthropic_api_key() -> Result<Option<String>, CredentialError> {
    Ok(store().load()?.map(|credential| credential.secret))
}

/// Removes the saved Anthropic API key, if there is one.
///
/// # Errors
///
/// Returns [`CredentialError`] if the platform keyring rejects the delete.
pub fn delete_anthropic_api_key() -> Result<(), CredentialError> {
    store().delete()
}
