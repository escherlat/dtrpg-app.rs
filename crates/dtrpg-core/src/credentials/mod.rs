//! Secure credential storage abstraction over platform-native keyring.
//!
//! Use [`KeyringCredentialStore`] to store, retrieve, and delete DriveThruRPG
//! credentials. All entries are namespaced under
//! `"com.pilgrimagesoftware.dtrpg"`.
//!
//! # Example
//!
//! ```rust,no_run
//! use dtrpg_core::credentials::{Credential, CredentialStore, KeyringCredentialStore};
//! use dtrpg_core::data::constants::{KEYRING_API_KEY, KEYRING_SERVICE};
//! let store = KeyringCredentialStore::new(KEYRING_SERVICE, KEYRING_API_KEY);
//! let credential = Credential { service: KEYRING_SERVICE.into(),
//!                               account: KEYRING_API_KEY.into(),
//!                               secret:  "my-api-key".into(),
//!                               email:   None, };
//! store.store(&credential)
//!      .expect("failed to store credential");
//! let loaded = store.load().expect("failed to load credential");
//! store.delete().expect("failed to delete credential");
//! ```

mod model;
mod store;

pub use model::{Credential, CredentialError};
pub use store::{CredentialStore, KeyringCredentialStore};
