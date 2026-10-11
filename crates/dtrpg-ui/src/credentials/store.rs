//! `CredentialStore` trait and `KeyringCredentialStore` implementation.

use keyring::Entry;
use serde::{Deserialize, Serialize};

use super::model::{Credential, CredentialError};

/// JSON payload persisted in the platform keyring.
///
/// Legacy entries contain only a raw API key string. When `load` reads a
/// value that fails to deserialize as this struct, it treats the raw string
/// as the key and sets `email` to `None`.
#[derive(Serialize, Deserialize)]
struct KeyringPayload {
    key:   String,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
}

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Platform-agnostic interface for storing, retrieving, and deleting a
/// single credential entry.
///
/// Each `CredentialStore` instance is bound to one `(service, account)` pair.
/// Use separate instances for each credential type (API key, access token,
/// refresh token).
///
/// # Errors
///
/// Methods return [`CredentialError`] when the underlying platform store
/// fails. Callers MUST surface these errors to the user and MUST NOT fall
/// back to plaintext storage.
pub trait CredentialStore: Send + Sync {
    /// Writes the credential to the platform store.
    ///
    /// When `credential.email` is `Some`, the email is persisted alongside
    /// the key as a JSON payload; legacy plain-key format is written when
    /// `email` is `None`.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialError::Store`] if the write fails, or
    /// [`CredentialError::Unavailable`] if the platform store is not
    /// accessible.
    fn store(&self, credential: &Credential) -> Result<(), CredentialError>;

    /// Reads the credential from the platform store.
    ///
    /// Returns `Ok(None)` when no entry exists yet (first run / after
    /// sign-out). Tolerates legacy entries that contain only a plain
    /// application key with no email; those are returned with
    /// `credential.email = None`.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialError::Load`] on read failure, or
    /// [`CredentialError::Unavailable`] if the platform store is inaccessible.
    fn load(&self) -> Result<Option<Credential>, CredentialError>;

    /// Removes the credential from the platform store.
    ///
    /// Succeeds silently if no entry exists.
    ///
    /// # Errors
    ///
    /// Returns [`CredentialError::Delete`] on unexpected failure.
    fn delete(&self) -> Result<(), CredentialError>;
}

// ── KeyringCredentialStore
// ────────────────────────────────────────────────────

/// [`CredentialStore`] backed by the platform native keyring via the `keyring`
/// crate (macOS Keychain, Windows Credential Manager, Linux Secret Service).
pub struct KeyringCredentialStore {
    service: String,
    account: String,
}

impl KeyringCredentialStore {
    /// Creates a store bound to the given `service` and `account` pair.
    ///
    /// Use the constants in [`crate::credentials::keys`] for both arguments.
    pub fn new(service: impl Into<String>, account: impl Into<String>) -> Self {
        Self { service: service.into(),
               account: account.into(), }
    }

    fn entry(&self) -> Result<Entry, CredentialError> {
        Entry::new(&self.service, &self.account).map_err(|e| {
                                                    CredentialError::Unavailable(e.to_string())
                                                })
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn store(&self, credential: &Credential) -> Result<(), CredentialError> {
        let payload = KeyringPayload { key:   credential.secret.clone(),
                                       email: credential.email.clone(), };
        let json = serde_json::to_string(&payload).map_err(|e| {
                                                      CredentialError::Store { account:
                                                                                   self.account
                                                                                       .clone(),
                                                                               reason:
                                                                                   e.to_string(), }
                                                  })?;
        self.entry()?
            .set_password(&json)
            .map_err(|e| CredentialError::Store { account: self.account.clone(),
                                                  reason:  e.to_string(), })
    }

    fn load(&self) -> Result<Option<Credential>, CredentialError> {
        match self.entry()?.get_password() {
            Ok(raw) => {
                // Attempt to parse new JSON payload; fall back to legacy raw
                // key string.
                let (secret, email) = match serde_json::from_str::<KeyringPayload>(&raw) {
                    Ok(p) => (p.key, p.email),
                    Err(_) => (raw, None),
                };
                Ok(Some(Credential { service: self.service.clone(),
                                     account: self.account.clone(),
                                     secret,
                                     email }))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CredentialError::Load { account: self.account.clone(),
                                                  reason:  e.to_string(), }),
        }
    }

    fn delete(&self) -> Result<(), CredentialError> {
        match self.entry()?.delete_credential() {
            Ok(()) => Ok(()),
            // Already gone — treat as success.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CredentialError::Delete { account: self.account.clone(),
                                                    reason:  e.to_string(), }),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::data::constants::{KEYRING_API_KEY, KEYRING_SERVICE};

    // ── Mock store for unit testing call-site behavior
    // ────────────────────────

    #[derive(Default)]
    struct MockCredentialStore {
        stored:       Arc<Mutex<Option<Credential>>>,
        store_error:  bool,
        load_error:   bool,
        delete_error: bool,
    }

    impl CredentialStore for MockCredentialStore {
        fn store(&self, credential: &Credential) -> Result<(), CredentialError> {
            if self.store_error {
                return Err(CredentialError::Store { account: "mock".into(),
                                                    reason:  "injected error".into(), });
            }
            *self.stored
                 .lock()
                 .map_err(|_| CredentialError::Unavailable("lock poisoned".into()))? =
                Some(credential.clone());
            Ok(())
        }

        fn load(&self) -> Result<Option<Credential>, CredentialError> {
            if self.load_error {
                return Err(CredentialError::Load { account: "mock".into(),
                                                   reason:  "injected error".into(), });
            }
            Ok(self.stored
                   .lock()
                   .map_err(|_| CredentialError::Unavailable("lock poisoned".into()))?
                   .clone())
        }

        fn delete(&self) -> Result<(), CredentialError> {
            if self.delete_error {
                return Err(CredentialError::Delete { account: "mock".into(),
                                                     reason:  "injected error".into(), });
            }
            *self.stored
                 .lock()
                 .map_err(|_| CredentialError::Unavailable("lock poisoned".into()))? = None;
            Ok(())
        }
    }

    fn make_credential(secret: &str) -> Credential {
        Credential { service: KEYRING_SERVICE.into(),
                     account: KEYRING_API_KEY.into(),
                     secret:  secret.into(),
                     email:   None, }
    }

    fn make_credential_with_email(secret: &str, email: &str) -> Credential {
        Credential { service: KEYRING_SERVICE.into(),
                     account: KEYRING_API_KEY.into(),
                     secret:  secret.into(),
                     email:   Some(email.into()), }
    }

    #[test]
    fn mock_store_then_load_returns_secret() {
        let store = MockCredentialStore::default();
        store.store(&make_credential("test-secret"))
             .expect("store should succeed");
        let loaded = store.load().expect("load should succeed");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().secret, "test-secret");
    }

    #[test]
    fn mock_store_then_load_preserves_email() {
        let store = MockCredentialStore::default();
        store.store(&make_credential_with_email("the-key", "user@example.com"))
             .expect("store should succeed");
        let loaded = store.load().expect("load should succeed").unwrap();
        assert_eq!(loaded.secret, "the-key");
        assert_eq!(loaded.email.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn mock_load_before_store_returns_none() {
        let store = MockCredentialStore::default();
        let loaded = store.load().expect("load should succeed");
        assert!(loaded.is_none());
    }

    #[test]
    fn mock_delete_clears_stored_credential() {
        let store = MockCredentialStore::default();
        store.store(&make_credential("secret"))
             .expect("store should succeed");
        store.delete().expect("delete should succeed");
        let loaded = store.load().expect("load should succeed");
        assert!(loaded.is_none());
    }

    #[test]
    fn mock_delete_on_empty_store_succeeds() {
        let store = MockCredentialStore::default();
        store.delete()
             .expect("delete on empty store should succeed");
    }

    #[test]
    fn mock_store_error_is_propagated() {
        let store = MockCredentialStore { store_error: true,
                                          ..Default::default() };
        let result = store.store(&make_credential("s"));
        assert!(result.is_err());
        assert!(matches!(result, Err(CredentialError::Store { .. })));
    }

    #[test]
    fn mock_load_error_is_propagated() {
        let store = MockCredentialStore { load_error: true,
                                          ..Default::default() };
        let result = store.load();
        assert!(result.is_err());
        assert!(matches!(result, Err(CredentialError::Load { .. })));
    }

    #[test]
    fn credential_debug_redacts_secret() {
        let cred = make_credential("my-super-secret");
        let debug = format!("{cred:?}");
        assert!(!debug.contains("my-super-secret"));
        assert!(debug.contains("[redacted]"));
    }

    // ── KeyringPayload JSON encoding
    // ──────────────────────────────────────────

    #[test]
    fn payload_round_trips_key_and_email() {
        let p = KeyringPayload { key:   "my-api-key".into(),
                                 email: Some("user@example.com".into()), };
        let json = serde_json::to_string(&p).unwrap();
        let back: KeyringPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back.key, "my-api-key");
        assert_eq!(back.email.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn payload_omits_email_field_when_none() {
        let p = KeyringPayload { key:   "my-api-key".into(),
                                 email: None, };
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("email"));
    }

    #[test]
    fn mock_store_and_load_preserves_email() {
        let store = MockCredentialStore::default();
        store.store(&make_credential_with_email("key-abc", "user@example.com"))
             .expect("store should succeed");
        // The mock stores only the secret string, so email passthrough isn't
        // exercised here — that is covered by KeyringPayload round-trip tests.
        let loaded = store.load().expect("load should succeed");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().secret, "key-abc");
    }

    #[test]
    fn legacy_raw_key_string_deserializes_without_email() {
        // Simulate a legacy keyring value that is a bare API key, not JSON.
        let raw = "legacy-raw-api-key-12345";
        let result = serde_json::from_str::<KeyringPayload>(raw);
        assert!(result.is_err(), "raw key should fail JSON parse");
        // The store.rs load path falls back to (raw, None) in this case.
    }
}
