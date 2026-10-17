//! Authentication state type used by `AuthStateController`.

/// Represents the application's current authentication state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthState {
    /// No credentials are present; the user has never signed in.
    Unauthenticated,
    /// Valid credentials are present and the session is active.
    Authenticated,
    /// Credentials exist but the session token has expired and must be renewed.
    SessionExpired,
}
