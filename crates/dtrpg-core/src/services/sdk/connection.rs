//! Shared SDK client + runtime construction, used by both the library and
//! collections gateways.
//!
//! Both domains talk to the same underlying [`SdkLibraryClient`] (it serves
//! the whole DTRPG API surface, not just the "library" endpoints) via an
//! identical credential-resolution and connection-setup sequence — this
//! module is that shared sequence, kept domain-agnostic so each of
//! `library::errors` and `collections::errors` translates its outcome into
//! its own service error type with its own wording.

use std::{
    future::Future,
    sync::{Arc, Mutex, PoisonError},
};

use dtrpg_sdk::{
    AuthTokenResponse, ClientError, Config, DriveThruRpgSdk, LibraryClient as SdkLibraryClient,
    SdkError, auth::key_exchange,
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

/// An SDK client plus the runtime that drives it.
///
/// When a request fails with HTTP 401 (expired access token), the connection
/// exchanges the stored application key for a fresh token and retries the
/// request once, so callers never see an expiry that can be repaired
/// silently. The application key itself is never regenerated.
pub(super) struct SdkConnection {
    config:          Config,
    application_key: String,
    client:          Mutex<Arc<SdkLibraryClient>>,
    runtime:         Runtime,
}

impl SdkConnection {
    /// Runs `operation` against the current client, refreshing the access
    /// token and retrying once if the server rejects it as unauthorized.
    ///
    /// # Errors
    ///
    /// Returns the operation's error, or the token exchange's error when the
    /// refresh itself fails.
    pub(super) fn run<T, F, Fut>(&self, operation: F) -> Result<T, ClientError>
        where F: Fn(Arc<SdkLibraryClient>) -> Fut,
              Fut: Future<Output = Result<T, ClientError>> {
        let client = self.current_client();
        match self.runtime.block_on(operation(Arc::clone(&client))) {
            Err(error) if is_unauthorized(&error) => {
                tracing::info!("access token rejected; refreshing it and retrying once");
                let refreshed = self.refresh_client(&client)?;
                self.runtime.block_on(operation(refreshed))
            }
            result => result,
        }
    }

    fn current_client(&self) -> Arc<SdkLibraryClient> {
        Arc::clone(&self.client.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Replaces `stale` with a client holding a freshly exchanged token.
    ///
    /// The lock is held across the exchange so concurrent callers that hit
    /// the same expiry perform one exchange, not one each: whoever arrives
    /// second finds the client already replaced and reuses it.
    fn refresh_client(&self, stale: &Arc<SdkLibraryClient>)
                      -> Result<Arc<SdkLibraryClient>, ClientError> {
        let mut current = self.client.lock().unwrap_or_else(PoisonError::into_inner);
        if !Arc::ptr_eq(&current, stale) {
            return Ok(Arc::clone(&current));
        }

        let tokens =
            self.runtime
                .block_on(key_exchange::authenticate(&self.application_key, &self.config))?;
        let fresh = Arc::new(SdkLibraryClient::new(self.config.clone(), tokens.token));
        *current = Arc::clone(&fresh);
        Ok(fresh)
    }
}

/// Whether the server rejected the request's credentials (HTTP 401).
fn is_unauthorized(error: &ClientError) -> bool {
    match error {
        ClientError::ApiError { status, .. } | ClientError::DecodeFailed { status, .. } => {
            *status == 401
        }
        ClientError::Http(error) => error.status().is_some_and(|status| status.as_u16() == 401),
        _ => false,
    }
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
        Ok(base_url) => Config::with_base_url(application_key.clone(), base_url),
        Err(_) => Config::new(application_key.clone()),
    };

    let mut sdk = DriveThruRpgSdk::with_config(config.clone());
    sdk.apply_auth_response(AuthTokenResponse::new(access_token, refresh_token, refresh_token_ttl))
       .map_err(ConnectionError::Sdk)?;

    let client = sdk.library_client().map_err(ConnectionError::Sdk)?;
    let runtime = Builder::new_multi_thread().enable_all()
                                             .build()
                                             .map_err(ConnectionError::RuntimeInit)?;

    Ok(SdkConnection { config,
                       application_key,
                       client: Mutex::new(Arc::new(client)),
                       runtime })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::atomic::{AtomicU32, Ordering},
    };

    use super::*;

    const TOKEN_BODY: &str =
        r#"{"token":"fresh-token","refreshToken":"fresh-refresh","refreshTokenTTL":9999999999}"#;

    fn api_error(status: u16) -> ClientError {
        ClientError::ApiError { url: "https://api.example/foo".to_string(),
                                status,
                                message: None,
                                payload: String::new(),
                                retry_after: None }
    }

    /// Serves `responses` (status line + JSON body) one per connection and
    /// returns the base URL.
    fn serve_auth_responses(responses: Vec<(&'static str, &'static str)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let reply = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                    body.len());
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        base_url
    }

    fn connection_for(base_url: String) -> SdkConnection {
        let config = Config::with_base_url("app-key", base_url);
        let client = SdkLibraryClient::new(config.clone(), "stale-token".to_string());
        SdkConnection { config,
                        application_key: "app-key".to_string(),
                        client: Mutex::new(Arc::new(client)),
                        runtime: Builder::new_multi_thread().enable_all().build().unwrap() }
    }

    #[test]
    fn is_unauthorized_matches_only_401() {
        assert!(is_unauthorized(&api_error(401)));
        assert!(!is_unauthorized(&api_error(403)));
        assert!(!is_unauthorized(&api_error(500)));
    }

    #[test]
    fn run_refreshes_token_and_retries_once_on_401() {
        let connection = connection_for(serve_auth_responses(vec![("200 OK", TOKEN_BODY)]));
        let calls = AtomicU32::new(0);

        let result = connection.run(|_client| async {
                                   if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                                       Err(api_error(401))
                                   }
                                   else {
                                       Ok("done")
                                   }
                               });

        assert_eq!(result.unwrap(), "done");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn run_does_not_refresh_on_other_errors() {
        // No server: a refresh attempt would fail with a transport error
        // instead of the operation's own error.
        let connection = connection_for("http://127.0.0.1:1".to_string());
        let calls = AtomicU32::new(0);

        let result: Result<(), ClientError> = connection.run(|_client| async {
                                                            calls.fetch_add(1, Ordering::SeqCst);
                                                            Err(api_error(500))
                                                        });

        assert!(matches!(result, Err(ClientError::ApiError { status: 500, .. })));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn run_surfaces_the_refresh_failure_without_retrying() {
        let connection = connection_for(serve_auth_responses(vec![("401 Unauthorized",
                                                                   r#"{"error":"bad key"}"#)]));
        let calls = AtomicU32::new(0);

        let result: Result<(), ClientError> = connection.run(|_client| async {
                                                            calls.fetch_add(1, Ordering::SeqCst);
                                                            Err(api_error(401))
                                                        });

        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn refresh_client_reuses_a_client_another_caller_already_replaced() {
        let connection = connection_for("http://127.0.0.1:1".to_string());
        let stale = connection.current_client();
        let replacement = Arc::new(SdkLibraryClient::new(connection.config.clone(),
                                                         "other-thread-token".to_string()));
        *connection.client.lock().unwrap() = Arc::clone(&replacement);

        let refreshed = connection.refresh_client(&stale).unwrap();

        assert!(Arc::ptr_eq(&refreshed, &replacement));
    }
}
