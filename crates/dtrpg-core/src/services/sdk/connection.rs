//! Shared SDK client + runtime construction, used by both the library and
//! collections gateways.
//!
//! Both domains talk to the same underlying [`SdkLibraryClient`] (it serves
//! the whole DTRPG API surface, not just the "library" endpoints) via an
//! identical credential-resolution and connection-setup sequence — this
//! module is that shared sequence, kept domain-agnostic so each of
//! `library::errors` and `collections::errors` translates its outcome into
//! its own service error type with its own wording.

use dtrpg_sdk::{
    AuthTokenResponse, Config, DriveThruRpgSdk, LibraryClient as SdkLibraryClient, SdkError,
};
use dtrpg_ui::{
    credentials::{CredentialStore, KeyringCredentialStore},
    data::constants::{KEYRING_API_KEY, KEYRING_SERVICE},
    services::LoginTokens,
};
use tokio::runtime::{Builder, Runtime};

use crate::constants::{
    ACCESS_TOKEN_ENV, API_BASE_URL_ENV, APPLICATION_KEY_ENV, REFRESH_TOKEN_ENV,
    REFRESH_TOKEN_TTL_ENV,
};

pub(super) struct SdkConnection {
    pub(super) client:  SdkLibraryClient,
    pub(super) runtime: Runtime,
}

/// Domain-agnostic connection failure. Each domain's `errors` module
/// translates this into its own service error type and wording.
pub(super) enum ConnectionError {
    /// No API key found in the keyring or as a fallback environment variable.
    MissingApiKey,
    /// `APPLICATION_KEY_ENV` was required (environment-only credential path)
    /// but unset.
    MissingApplicationKeyEnv,
    /// `ACCESS_TOKEN_ENV` was required (environment-only credential path) but
    /// unset.
    MissingAccessTokenEnv,
    Sdk(SdkError),
    RuntimeInit(std::io::Error),
}

/// Builds a connection using only the keyring for the API key and the given
/// in-memory `tokens` obtained at startup or login.
pub(super) fn connect_from_keyring_with_tokens(tokens: LoginTokens)
                                               -> Result<SdkConnection, ConnectionError> {
    let application_key = resolve_keyring_api_key()?;
    build_connection(application_key,
                     tokens.access_token,
                     tokens.refresh_token,
                     tokens.refresh_token_ttl)
}

/// Builds a connection reading every credential from environment variables.
#[allow(dead_code)]
pub(super) fn connect_from_environment() -> Result<SdkConnection, ConnectionError> {
    let application_key =
        std::env::var(APPLICATION_KEY_ENV).map_err(|_| ConnectionError::MissingApplicationKeyEnv)?;
    let access_token =
        std::env::var(ACCESS_TOKEN_ENV).map_err(|_| ConnectionError::MissingAccessTokenEnv)?;
    let refresh_token = std::env::var(REFRESH_TOKEN_ENV).unwrap_or_default();
    let refresh_token_ttl =
        std::env::var(REFRESH_TOKEN_TTL_ENV).ok()
                                            .and_then(|value| value.parse::<u64>().ok())
                                            .unwrap_or(u64::MAX);

    build_connection(application_key,
                     access_token,
                     refresh_token,
                     refresh_token_ttl)
}

fn resolve_keyring_api_key() -> Result<String, ConnectionError> {
    KeyringCredentialStore::new(KEYRING_SERVICE, KEYRING_API_KEY)
        .load()
        .ok()
        .flatten()
        .map(|c| c.secret)
        .or_else(|| std::env::var(APPLICATION_KEY_ENV).ok())
        .ok_or(ConnectionError::MissingApiKey)
}

fn build_connection(application_key: String, access_token: String, refresh_token: String,
                    refresh_token_ttl: u64)
                    -> Result<SdkConnection, ConnectionError> {
    let config = match std::env::var(API_BASE_URL_ENV) {
        Ok(base_url) => Config::with_base_url(application_key, base_url),
        Err(_) => Config::new(application_key),
    };

    let mut sdk = DriveThruRpgSdk::with_config(config);
    sdk.apply_auth_response(AuthTokenResponse::new(access_token, refresh_token, refresh_token_ttl))
       .map_err(ConnectionError::Sdk)?;

    let client = sdk.library_client().map_err(ConnectionError::Sdk)?;
    let runtime = Builder::new_multi_thread().enable_all()
                                             .build()
                                             .map_err(ConnectionError::RuntimeInit)?;

    Ok(SdkConnection { client, runtime })
}
