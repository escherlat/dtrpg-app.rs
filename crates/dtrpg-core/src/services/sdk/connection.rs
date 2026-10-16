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
    data::{
        constants::{KEYRING_API_KEY, KEYRING_SERVICE},
        token_status::{TokenStatus, TokenStatusHandle},
    },
    services::LoginTokens,
    util::datetime::now_secs,
};
use tokio::runtime::{Builder, Runtime};

use crate::constants::{
    ACCESS_TOKEN_ENV, API_BASE_URL_ENV, APPLICATION_KEY_ENV, REFRESH_TOKEN_ENV,
    REFRESH_TOKEN_TTL_ENV,
};

/// An SDK session plus the runtime that drives it.
///
/// When a request fails with HTTP 401 (expired access token), the connection
/// renews the session and retries the request once, so callers never see an
/// expiry that can be repaired silently. It first spends the session's refresh
/// token; if the API rejects that too, it exchanges the stored application key
/// for a new session. The application key itself is never regenerated.
///
/// Every new access token's timing is published to `token_status` so the
/// Settings window can show when the current token was acquired and expires.
pub(super) struct SdkConnection {
    application_key: String,
    session:         Mutex<Session>,
    token_status:    TokenStatusHandle,
    runtime:         Runtime,
}

/// The SDK's auth state and the client built from its current access token.
struct Session {
    sdk:    DriveThruRpgSdk,
    client: Arc<SdkLibraryClient>,
}

impl SdkConnection {
    /// Runs `operation` against the current client, renewing the session and
    /// retrying once if the server rejects the access token as unauthorized.
    ///
    /// # Errors
    ///
    /// Returns the operation's error, or the renewal's error when the session
    /// cannot be renewed.
    pub(super) fn run<T, F, Fut>(&self, operation: F) -> Result<T, ClientError>
        where F: Fn(Arc<SdkLibraryClient>) -> Fut,
              Fut: Future<Output = Result<T, ClientError>> {
        let client = self.current_client();
        match self.runtime.block_on(operation(Arc::clone(&client))) {
            Err(error) if is_unauthorized(&error) => {
                tracing::info!("access token rejected; renewing the session and retrying once");
                let renewed = self.renew_client(&client)?;
                self.runtime.block_on(operation(renewed))
            }
            result => result,
        }
    }

    fn current_client(&self) -> Arc<SdkLibraryClient> {
        Arc::clone(&self.lock_session().client)
    }

    fn lock_session(&self) -> std::sync::MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Replaces `stale` with a client holding a renewed access token.
    ///
    /// The lock is held across the renewal so concurrent callers that hit the
    /// same expiry renew once, not once each: whoever arrives second finds the
    /// client already replaced and reuses it.
    fn renew_client(&self, stale: &Arc<SdkLibraryClient>)
                    -> Result<Arc<SdkLibraryClient>, ClientError> {
        let mut session = self.lock_session();
        if !Arc::ptr_eq(&session.client, stale) {
            return Ok(Arc::clone(&session.client));
        }

        self.runtime
            .block_on(renew_session(&mut session.sdk, &self.application_key))?;
        let renewed = Arc::new(session.sdk.library_client()?);
        self.token_status
            .set(TokenStatus::from_access_token(session.sdk.require_session()?.token(),
                                                now_secs()));
        session.client = Arc::clone(&renewed);
        Ok(renewed)
    }
}

/// Renews `sdk`'s session with its refresh token, falling back to an
/// application-key exchange when the refresh token is rejected or unusable
/// (expired, revoked, or absent).
async fn renew_session(sdk: &mut DriveThruRpgSdk, application_key: &str)
                       -> Result<(), ClientError> {
    let Err(error) = sdk.refresh_session().await
    else {
        return Ok(());
    };

    tracing::info!(?error,
                   "refresh token not accepted; exchanging the application key instead");
    let tokens = key_exchange::authenticate(application_key, sdk.require_config()?).await?;
    sdk.apply_auth_response(tokens)?;
    Ok(())
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
pub(super) fn connect_from_keyring_with_tokens(tokens: LoginTokens,
                                               token_status: TokenStatusHandle)
                                               -> Result<SdkConnection, ConnectionError> {
    let application_key = resolve_keyring_api_key()?;
    build_connection(application_key,
                     tokens.access_token,
                     tokens.refresh_token,
                     tokens.refresh_token_ttl,
                     token_status)
}

/// Builds a connection reading every credential from environment variables.
#[allow(dead_code)]
pub(super) fn connect_from_environment(token_status: TokenStatusHandle)
                                       -> Result<SdkConnection, ConnectionError> {
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
                     refresh_token_ttl,
                     token_status)
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
                    refresh_token_ttl: u64, token_status: TokenStatusHandle)
                    -> Result<SdkConnection, ConnectionError> {
    let config = match std::env::var(API_BASE_URL_ENV) {
        Ok(base_url) => Config::with_base_url(application_key.clone(), base_url),
        Err(_) => Config::new(application_key.clone()),
    };

    token_status.set(TokenStatus::from_access_token(&access_token, now_secs()));

    let mut sdk = DriveThruRpgSdk::with_config(config);
    sdk.apply_auth_response(AuthTokenResponse::new(access_token, refresh_token, refresh_token_ttl))
       .map_err(ConnectionError::Sdk)?;

    let client = sdk.library_client().map_err(ConnectionError::Sdk)?;
    let runtime = Builder::new_multi_thread().enable_all()
                                             .build()
                                             .map_err(ConnectionError::RuntimeInit)?;

    let session = Session { sdk,
                            client: Arc::new(client) };

    Ok(SdkConnection { application_key,
                       session: Mutex::new(session),
                       token_status,
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

    type RequestPaths = Arc<Mutex<Vec<String>>>;

    /// Serves `responses` (status line + JSON body) one per connection and
    /// returns the base URL plus the request paths seen so far.
    fn serve_auth_responses(responses: Vec<(&'static str, &'static str)>)
                            -> (String, RequestPaths) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let paths = RequestPaths::default();
        let recorded = Arc::clone(&paths);
        std::thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let read = stream.read(&mut buf).unwrap();
                let request = String::from_utf8_lossy(&buf[..read]);
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                recorded.lock().unwrap().push(path.to_string());
                let reply = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                                    body.len());
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        (base_url, paths)
    }

    fn connection_for(base_url: String) -> SdkConnection {
        let mut sdk = DriveThruRpgSdk::with_config(Config::with_base_url("app-key", base_url));
        sdk.apply_auth_response(AuthTokenResponse::new("stale-token",
                                                       "stale-refresh",
                                                       1_000_000_000))
           .unwrap();
        let client = Arc::new(sdk.library_client().unwrap());
        SdkConnection { application_key: "app-key".to_string(),
                        session:         Mutex::new(Session { sdk, client }),
                        token_status:    TokenStatusHandle::default(),
                        runtime:         Builder::new_multi_thread().enable_all().build().unwrap(), }
    }

    #[test]
    fn is_unauthorized_matches_only_401() {
        assert!(is_unauthorized(&api_error(401)));
        assert!(!is_unauthorized(&api_error(403)));
        assert!(!is_unauthorized(&api_error(500)));
    }

    #[test]
    fn run_renews_with_the_refresh_token_and_retries_once_on_401() {
        let (base_url, paths) = serve_auth_responses(vec![("200 OK", TOKEN_BODY)]);
        let connection = connection_for(base_url);
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
        let paths = paths.lock().unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("/auth/refresh"),
                "unexpected path {}",
                paths[0]);
    }

    #[test]
    fn run_falls_back_to_the_application_key_when_the_refresh_token_is_rejected() {
        let (base_url, paths) = serve_auth_responses(vec![("401 Unauthorized",
                                                           r#"{"errorCode":"refresh_expired"}"#),
                                                          ("200 OK", TOKEN_BODY)]);
        let connection = connection_for(base_url);
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
        let paths = paths.lock().unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("/auth/refresh"),
                "unexpected path {}",
                paths[0]);
        assert!(paths[1].contains("/auth_key"),
                "unexpected path {}",
                paths[1]);
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
    fn run_surfaces_the_renewal_failure_without_retrying() {
        let rejection = ("401 Unauthorized", r#"{"error":"rejected"}"#);
        let (base_url, _paths) = serve_auth_responses(vec![rejection, rejection]);
        let connection = connection_for(base_url);
        let calls = AtomicU32::new(0);

        let result: Result<(), ClientError> = connection.run(|_client| async {
                                                            calls.fetch_add(1, Ordering::SeqCst);
                                                            Err(api_error(401))
                                                        });

        assert!(matches!(result, Err(ClientError::DecodeFailed { status: 401, .. })));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn renew_client_reuses_a_client_another_caller_already_replaced() {
        let connection = connection_for("http://127.0.0.1:1".to_string());
        let stale = connection.current_client();
        let replacement = Arc::new(SdkLibraryClient::new(Config::new("app-key"),
                                                         "other-thread-token".to_string()));
        connection.lock_session().client = Arc::clone(&replacement);

        let renewed = connection.renew_client(&stale).unwrap();

        assert!(Arc::ptr_eq(&renewed, &replacement));
    }

    #[test]
    fn run_publishes_the_renewed_tokens_timing() {
        let (base_url, _paths) = serve_auth_responses(vec![("200 OK", TOKEN_BODY)]);
        let connection = connection_for(base_url);
        assert_eq!(connection.token_status.get(), None);
        let before = now_secs();
        let calls = AtomicU32::new(0);

        connection.run(|_client| async {
                      if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                          Err(api_error(401))
                      }
                      else {
                          Ok(())
                      }
                  })
                  .unwrap();

        // The stub's token is not a JWT, so the acquisition time falls back to
        // when the renewal happened.
        let status = connection.token_status.get().unwrap();
        assert!(status.acquired_at >= before);
        assert_eq!(status.expires_at, None);
    }
}
