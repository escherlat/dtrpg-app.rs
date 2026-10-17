//! What the app knows about the current access token, for display in Settings.
//!
//! Tokens are renewed in the background by the SDK services, so the services
//! publish each new token's timing into a shared [`TokenStatusHandle`] and the
//! Settings window reads it when it renders.

use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;

/// When the current access token was acquired and when it expires, as Unix
/// timestamps in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenStatus {
    /// When the token was issued.
    pub acquired_at: i64,
    /// When the token stops being accepted, or `None` if the token does not
    /// say.
    pub expires_at: Option<i64>,
}

/// The timing claims of an access token's JWT payload.
#[derive(Deserialize)]
struct TokenClaims {
    iat: Option<i64>,
    exp: Option<i64>,
}

impl TokenStatus {
    /// Reads the timing out of `access_token`, a JWT, without verifying it.
    ///
    /// The `iat` claim is the acquisition time and `exp` the expiry. If the
    /// token is not a readable JWT or has no `iat`, `received_at` (when the
    /// app got the token) is used as the acquisition time instead.
    pub fn from_access_token(access_token: &str, received_at: i64) -> Self {
        let claims = decode_claims(access_token);
        Self {
            acquired_at: claims.as_ref().and_then(|c| c.iat).unwrap_or(received_at),
            expires_at: claims.and_then(|c| c.exp),
        }
    }
}

/// Decodes the payload segment of a JWT (`header.payload.signature`).
fn decode_claims(jwt: &str) -> Option<TokenClaims> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// A shared, thread-safe slot holding the current [`TokenStatus`].
///
/// Cloning gives another handle to the same slot, so the services that renew
/// tokens and the Settings window that displays them see the same value.
#[derive(Clone, Default)]
pub struct TokenStatusHandle(Arc<Mutex<Option<TokenStatus>>>);

impl TokenStatusHandle {
    /// Records `status` as the current token's.
    pub fn set(&self, status: TokenStatus) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(status);
    }

    /// Forgets the current token, e.g. on sign-out.
    pub fn clear(&self) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Returns the current token's status, if there is one.
    pub fn get(&self) -> Option<TokenStatus> {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an unsigned JWT-shaped string with the given payload JSON.
    fn jwt_with_payload(payload: &str) -> String {
        format!("header.{}.signature", URL_SAFE_NO_PAD.encode(payload))
    }

    #[test]
    fn reads_issue_and_expiry_from_jwt_claims() {
        let token = jwt_with_payload(r#"{"iat":1768955233,"exp":1768976833,"email":"a@b.c"}"#);

        let status = TokenStatus::from_access_token(&token, 5);

        assert_eq!(
            status,
            TokenStatus {
                acquired_at: 1_768_955_233,
                expires_at: Some(1_768_976_833),
            }
        );
    }

    #[test]
    fn falls_back_to_receipt_time_when_token_is_not_a_jwt() {
        let status = TokenStatus::from_access_token("not-a-jwt", 1_000);

        assert_eq!(
            status,
            TokenStatus {
                acquired_at: 1_000,
                expires_at: None,
            }
        );
    }

    #[test]
    fn falls_back_to_receipt_time_when_iat_is_missing() {
        let token = jwt_with_payload(r#"{"exp":2000}"#);

        let status = TokenStatus::from_access_token(&token, 1_000);

        assert_eq!(
            status,
            TokenStatus {
                acquired_at: 1_000,
                expires_at: Some(2_000),
            }
        );
    }

    #[test]
    fn handle_shares_one_slot_across_clones() {
        let handle = TokenStatusHandle::default();
        let other = handle.clone();
        assert_eq!(handle.get(), None);

        let status = TokenStatus {
            acquired_at: 1,
            expires_at: Some(2),
        };
        other.set(status);
        assert_eq!(handle.get(), Some(status));

        handle.clear();
        assert_eq!(other.get(), None);
    }
}
