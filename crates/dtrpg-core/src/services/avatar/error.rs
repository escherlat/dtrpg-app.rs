//! The error an avatar fetch reports.

/// Why one avatar request failed.
///
/// Only [`Transient`](Self::Transient) failures are retried. A non-success
/// HTTP status, such as the `404` Gravatar returns for an unregistered email,
/// is final: asking again would not change the answer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AvatarFetchError {
    /// The request failed in a way that might not happen again, such as a
    /// timeout or a dropped connection. Carries a description for logging.
    Transient(String),
    /// The server answered, and the answer will not change.
    Final,
}
