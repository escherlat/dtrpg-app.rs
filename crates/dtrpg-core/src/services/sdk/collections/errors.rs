//! Translates Rust SDK / HTTP client / connection errors into
//! [`CollectionsServiceError`].

use dtrpg_sdk::{ClientError, SdkError};
use dtrpg_ui::services::collections::{CollectionsServiceError, CollectionsServiceErrorKind};

use super::super::connection::ConnectionError;
use crate::constants::{ACCESS_TOKEN_ENV, APPLICATION_KEY_ENV};

pub(super) fn map_client_error(error: ClientError) -> CollectionsServiceError {
    match error {
        ClientError::Sdk(e) => map_sdk_error(e),
        ClientError::DecodeFailed { url, status, cause, .. } => {
            let kind = match status {
                401 | 403 => CollectionsServiceErrorKind::Session,
                _ => CollectionsServiceErrorKind::Network,
            };
            CollectionsServiceError::new(kind,
                                         format!("Response from {url} (HTTP {status}) could not be decoded: {cause}"))
        }
        ClientError::ApiError { url,
                                status,
                                message,
                                payload,
                                retry_after, } => {
            let detail = message.unwrap_or(payload);
            // 409 on this service's endpoints means the request conflicts with
            // existing state (e.g. the item is already a member of the product
            // list) rather than a transient/network failure — surface just the
            // server's own message, not the URL/status wrapper used for genuine
            // failures below.
            if status == 409 {
                return CollectionsServiceError::new(CollectionsServiceErrorKind::Conflict, detail);
            }
            let kind = match status {
                401 | 403 => CollectionsServiceErrorKind::Session,
                429 => CollectionsServiceErrorKind::RateLimited,
                _ => CollectionsServiceErrorKind::Network,
            };
            CollectionsServiceError::new(kind,
                                         format!("Request to {url} failed (HTTP {status}): {detail}"))
                .with_retry_after(retry_after)
        }
        ClientError::Http(error) => {
            let status = error.status().map(|s| s.as_u16());
            let kind = match status {
                Some(401) | Some(403) => CollectionsServiceErrorKind::Session,
                _ => CollectionsServiceErrorKind::Network,
            };
            let mut msg = String::from("Collections request failed");
            if let Some(url) = error.url() {
                msg.push_str(&format!(" [{url}]"));
            }
            if let Some(code) = status {
                msg.push_str(&format!(" (HTTP {code})"));
            }
            msg.push_str(&format!(": {error}"));
            CollectionsServiceError::new(kind, msg)
        }
        // These variants are only produced by credential_login, never by
        // library/collections requests.
        ClientError::InvalidCredentials | ClientError::ApplicationKeyRequestFailed { .. } => {
            CollectionsServiceError::new(CollectionsServiceErrorKind::Session,
                                         "Unexpected credential exchange error in collections request")
        }
    }
}

pub(super) fn map_sdk_error(error: SdkError) -> CollectionsServiceError {
    let kind = match error {
        SdkError::Unauthenticated | SdkError::AuthSession(_) => {
            CollectionsServiceErrorKind::Session
        }
        SdkError::Unconfigured => CollectionsServiceErrorKind::Network,
    };
    CollectionsServiceError::new(kind, format!("Not ready: {error}"))
}

pub(super) fn map_connection_error(error: ConnectionError) -> CollectionsServiceError {
    match error {
        ConnectionError::MissingApiKey => {
            CollectionsServiceError::new(CollectionsServiceErrorKind::Session,
                                         "No API key found in keyring or environment. Sign in to continue.")
        }
        ConnectionError::MissingApplicationKeyEnv => {
            CollectionsServiceError::new(CollectionsServiceErrorKind::Network,
                                         format!("{APPLICATION_KEY_ENV} is required to load SDK-backed collections data."))
        }
        ConnectionError::MissingAccessTokenEnv => {
            CollectionsServiceError::new(CollectionsServiceErrorKind::Session,
                                         format!("{ACCESS_TOKEN_ENV} is required to load SDK-backed collections data."))
        }
        ConnectionError::Sdk(error) => map_sdk_error(error),
        ConnectionError::RuntimeInit(error) => {
            CollectionsServiceError::new(CollectionsServiceErrorKind::Network,
                                         format!("Unable to start Rust SDK runtime: {error}"))
        }
    }
}
