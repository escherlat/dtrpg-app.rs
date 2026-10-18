//! Auth-related notification types displayed in the `NotificationBanner`.

/// Identifies the kind of auth notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// The user has never signed in; no credentials are present.
    NotSignedIn,
    /// A session token was present but has expired.
    SessionExpired,
}

/// A single notice entry managed by `AuthStateController`.
#[derive(Debug, Clone)]
pub struct Notice {
    /// The kind of auth notice.
    pub kind:      NoticeKind,
    /// Whether the user has dismissed this notice for the current session.
    pub dismissed: bool,
    /// The primary action associated with this notice.
    pub action:    NoticeAction,
}

/// The action triggered by the primary button on a notice row.
#[derive(Debug, Clone, Copy)]
pub enum NoticeAction {
    /// Open the settings panel.
    OpenSettings,
}
