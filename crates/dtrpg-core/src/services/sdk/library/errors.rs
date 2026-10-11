//! Translates Rust SDK / HTTP client / connection errors into
//! [`LibraryServiceError`].

use std::error::Error as StdError;

use dtrpg_sdk::{ClientError, SdkError};
use dtrpg_ui::services::{LibraryServiceError, LibraryServiceErrorKind};

use super::super::connection::ConnectionError;
use crate::constants::{ACCESS_TOKEN_ENV, APPLICATION_KEY_ENV};

pub(super) fn map_client_error(error: ClientError) -> LibraryServiceError {
    match error {
        ClientError::Sdk(error) => map_sdk_error(error),
        ClientError::DecodeFailed { url, status, cause, .. } => {
            let kind = match status {
                401 => LibraryServiceErrorKind::NeedsReauth,
                403 => LibraryServiceErrorKind::Session,
                _ => LibraryServiceErrorKind::Network,
            };
            LibraryServiceError::new(kind,
                                     format!("Response from {url} (HTTP {status}) could not be decoded: {cause}"))
        }
        ClientError::ApiError { url,
                                status,
                                message,
                                payload,
                                retry_after, } => {
            let kind = match status {
                401 => LibraryServiceErrorKind::NeedsReauth,
                403 => LibraryServiceErrorKind::Session,
                429 => LibraryServiceErrorKind::RateLimited,
                _ => LibraryServiceErrorKind::Network,
            };
            let detail = message.unwrap_or(payload);
            LibraryServiceError::new(kind,
                                     format!("Request to {url} failed (HTTP {status}): {detail}"))
                .with_retry_after(retry_after)
        }
        ClientError::Http(error) => {
            let status = error.status().map(|s| s.as_u16());
            let kind = match status {
                Some(401) => LibraryServiceErrorKind::NeedsReauth,
                Some(403) => LibraryServiceErrorKind::Session,
                _ => LibraryServiceErrorKind::Network,
            };

            let mut msg = String::from("Library request failed");
            if let Some(url) = error.url() {
                msg.push_str(&format!(" [{url}]"));
            }
            if let Some(code) = status {
                msg.push_str(&format!(" (HTTP {code})"));
            }
            msg.push_str(&format!(": {error}"));

            let mut source: Option<&dyn StdError> = StdError::source(&error);
            while let Some(cause) = source {
                msg.push_str(&format!(": {cause}"));
                source = cause.source();
            }

            LibraryServiceError::new(kind, msg)
        }
        // These variants are only produced by credential_login, never by
        // library requests.
        ClientError::InvalidCredentials | ClientError::ApplicationKeyRequestFailed { .. } => {
            LibraryServiceError::new(LibraryServiceErrorKind::Session,
                                     "Unexpected credential exchange error in library request")
        }
    }
}

pub(super) fn map_sdk_error(error: SdkError) -> LibraryServiceError {
    let kind = match error {
        SdkError::Unauthenticated | SdkError::AuthSession(_) => LibraryServiceErrorKind::Session,
        SdkError::Unconfigured => LibraryServiceErrorKind::Network,
    };
    LibraryServiceError::new(kind, format!("Not ready: {error}"))
}

pub(super) fn map_connection_error(error: ConnectionError) -> LibraryServiceError {
    match error {
        ConnectionError::MissingApiKey => {
            LibraryServiceError::new(LibraryServiceErrorKind::Session,
                                     "No API key found in keyring or environment. Sign in to continue.")
        }
        ConnectionError::MissingApplicationKeyEnv => {
            LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                     format!("{APPLICATION_KEY_ENV} is required to load SDK-backed library data."))
        }
        ConnectionError::MissingAccessTokenEnv => {
            LibraryServiceError::new(LibraryServiceErrorKind::Session,
                                     format!("{ACCESS_TOKEN_ENV} is required to load SDK-backed library data."))
        }
        ConnectionError::Sdk(error) => map_sdk_error(error),
        ConnectionError::RuntimeInit(error) => {
            LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                     format!("Unable to start Rust SDK runtime: {error}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn map_client_error_maps_429_api_error_to_rate_limited_with_retry_after() {
        let error = ClientError::ApiError { url:         "https://api.example/foo".to_string(),
                                            status:      429,
                                            message:     Some("rate limited".to_string()),
                                            payload:     String::new(),
                                            retry_after: Some(Duration::from_secs(30)), };

        let mapped = map_client_error(error);

        assert_eq!(mapped.kind, LibraryServiceErrorKind::RateLimited);
        assert_eq!(mapped.retry_after, Some(Duration::from_secs(30)));
    }
}
