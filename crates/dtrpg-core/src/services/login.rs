//! SDK-backed implementation of [`LoginService`].

use dtrpg_sdk::auth::{credential_login, key_exchange};
use dtrpg_sdk::{ClientError, config::Config};
use dtrpg_ui::services::{LoginError, LoginService, LoginTokens};
use tokio::runtime::{Builder, Runtime};

use crate::constants::API_BASE_URL_ENV;

/// [`LoginService`] implementation backed by the DriveThruRPG SDK auth
/// endpoint.
pub struct SdkLoginService {
    config:  Config,
    runtime: Runtime,
}

impl SdkLoginService {
    /// Creates a new `SdkLoginService`.
    ///
    /// The API key in `config` is used only for the base URL and version; the
    /// per-request API key is supplied at
    /// [`authenticate`][SdkLoginService::authenticate] call time.
    ///
    /// # Errors
    ///
    /// Returns a [`LoginError`] if the Tokio runtime cannot be started.
    pub fn new() -> Result<Self, LoginError> {
        let config = match std::env::var(API_BASE_URL_ENV) {
            Ok(base_url) => Config::with_base_url("", base_url),
            Err(_) => Config::new(""),
        };
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| LoginError(format!("Unable to start login runtime: {e}")))?;
        Ok(Self { config, runtime })
    }
}

impl LoginService for SdkLoginService {
    fn login_with_credentials(&self, email: &str, password: &str) -> Result<String, LoginError> {
        let email = email.to_string();
        let password = password.to_string();
        let config = self.config.clone();
        self.runtime
            .block_on(credential_login::login_with_credentials(&email, &password, &config))
            .map_err(|e| match e {
                ClientError::InvalidCredentials => {
                    LoginError("Invalid email or password.".to_owned())
                }
                ClientError::ApplicationKeyRequestFailed { status } => {
                    LoginError(format!("Sign-in failed: server returned status '{status}'."))
                }
                ClientError::Http(err) => {
                    LoginError(format!("Network error during sign-in: {err}"))
                }
                other => LoginError(format!("Sign-in failed: {other:?}")),
            })
    }

    fn authenticate(&self, api_key: &str) -> Result<LoginTokens, LoginError> {
        let config = Config::with_base_url(api_key, self.config.base_url());
        let key = api_key.to_string();
        self.runtime
            .block_on(key_exchange::authenticate(&key, &config))
            .map(|r| LoginTokens { access_token:      r.token,
                                   refresh_token:     r.refresh_token,
                                   refresh_token_ttl: r.refresh_token_ttl, })
            .map_err(|e| LoginError(format!("Authentication failed: {e}")))
    }
}

/// A [`LoginService`] that always fails with the given error message.
///
/// Used when the Tokio runtime cannot be constructed at startup.
pub struct UnavailableLoginService {
    error: LoginError,
}

impl UnavailableLoginService {
    pub fn new(error: LoginError) -> Self {
        Self { error }
    }
}

impl LoginService for UnavailableLoginService {
    fn login_with_credentials(&self, _email: &str, _password: &str) -> Result<String, LoginError> {
        Err(self.error.clone())
    }

    fn authenticate(&self, _api_key: &str) -> Result<LoginTokens, LoginError> {
        Err(self.error.clone())
    }
}

/// Builds a boxed [`LoginService`] from the platform environment.
pub fn build_login_service() -> Box<dyn LoginService> {
    match SdkLoginService::new() {
        Ok(svc) => Box::new(svc),
        Err(e) => {
            tracing::error!(error = %e, "failed to create login service");
            Box::new(UnavailableLoginService::new(e))
        }
    }
}
