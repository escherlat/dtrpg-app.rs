//! Controller events emitted when state changes.

use gpui::EventEmitter;

use crate::controllers::activity::ActivityController;
use crate::controllers::auth_state::AuthStateController;
use crate::controllers::library::LibraryController;
use crate::controllers::settings::SettingsController;
use crate::controllers::tabs::TabsController;
use crate::services::LoginTokens;

// ── LibraryChanged
// ────────────────────────────────────────────────────────────

/// Emitted when any library state changes that should trigger a re-render.
pub struct LibraryChanged;

impl EventEmitter<LibraryChanged> for LibraryController {}

// ── ActivityChanged
// ───────────────────────────────────────────────────────────

/// Emitted when the activity item list changes (item added, completed, or
/// errored).
pub struct ActivityChanged;

impl EventEmitter<ActivityChanged> for ActivityController {}

// ── DownloadComplete
// ──────────────────────────────────────────────────────────

/// Emitted when a download activity item transitions to `Complete`.
pub struct DownloadComplete {
    /// Display label of the completed item.
    pub title: std::sync::Arc<str>,
}

impl EventEmitter<DownloadComplete> for ActivityController {}

// ── DownloadError
// ─────────────────────────────────────────────────────────────

/// Emitted when a download activity item transitions to `Error`.
pub struct DownloadError {
    /// Display label of the failed item.
    pub title:   std::sync::Arc<str>,
    /// Error message.
    pub message: String,
}

impl EventEmitter<DownloadError> for ActivityController {}

// ── SettingsChanged
// ───────────────────────────────────────────────────────────

/// Emitted when any settings state changes that should trigger a re-render.
pub struct SettingsChanged;

// ── LogoutRequested
// ───────────────────────────────────────────────────────────

/// Emitted by `SettingsController` when the user requests to log out.
pub struct LogoutRequested;

// ── AuthStateChanged
// ──────────────────────────────────────────────────────────

/// Emitted when authentication state or the active notice list changes.
pub struct AuthStateChanged;

impl EventEmitter<AuthStateChanged> for AuthStateController {}

// ── SignInSucceeded
// ───────────────────────────────────────────────────────────

/// Emitted by `SettingsController` when the user successfully signs in from the
/// Account tab.
///
/// The receiver should update `AuthStateController` and replace the
/// `LibraryService`.
pub struct SignInSucceeded(pub LoginTokens);

impl EventEmitter<SignInSucceeded> for SettingsController {}

// ── StartupAuthBegun
// ──────────────────────────────────────────────────────────

/// Emitted by `SettingsController` when a background startup re-authentication
/// begins.
///
/// The receiver should set `AuthStateController` to the auth-pending state so
/// the banner shows "Signing in..." instead of "Not signed in".
pub struct StartupAuthBegun;

impl EventEmitter<StartupAuthBegun> for SettingsController {}

// ── CollectionCreateFailed
// ────────────────────────────────────────────────────

/// Emitted by `LibraryController` when a background collection create call
/// fails.
///
/// The receiver should push an error `Notification` to the window.
pub struct CollectionCreateFailed {
    /// Human-readable error message.
    pub message: String,
}

impl EventEmitter<CollectionCreateFailed> for LibraryController {}

// ── CollectionMemberAddFailed
// ─────────────────────────────────────────

/// Emitted by `LibraryController` when a background add-member-to-collection
/// call fails.
///
/// The receiver should push an error `Notification` to the window. The
/// controller has already rolled back its optimistic local update by the time
/// this is emitted.
pub struct CollectionMemberAddFailed {
    /// Human-readable error message.
    pub message: String,
}

impl EventEmitter<CollectionMemberAddFailed> for LibraryController {}

// ── CollectionMemberRemoveFailed
// ──────────────────────────────────────

/// Emitted by `LibraryController` when a background
/// remove-member-from-collection call fails.
///
/// The receiver should push an error `Notification` to the window. The
/// controller has already rolled back its optimistic local update by the time
/// this is emitted.
pub struct CollectionMemberRemoveFailed {
    /// Human-readable error message.
    pub message: String,
}

impl EventEmitter<CollectionMemberRemoveFailed> for LibraryController {}

// ── CollectionMemberAlreadyPresent
// ─────────────────────────────────

/// Emitted by `LibraryController` when a background add-member-to-collection
/// call finds the item is already a member server-side (`HTTP 409`).
///
/// Unlike [`CollectionMemberAddFailed`], this is not treated as a failure:
/// the optimistic local `member_ids` update is left in place (it already
/// matches server state) and the receiver should show a low-severity,
/// auto-hiding `Notification` rather than a persistent error.
pub struct CollectionMemberAlreadyPresent {
    /// Human-readable message from the API describing the conflict.
    pub message: String,
}

impl EventEmitter<CollectionMemberAlreadyPresent> for LibraryController {}

// ── ThumbnailRefreshStarted
// ────────────────────────────────────────────────

/// Emitted by `LibraryController` when the "Refresh Thumbnails" catalog menu
/// action starts re-fetching a non-empty batch of covers.
///
/// The receiver should show a toast indicating the refresh has started.
pub struct ThumbnailRefreshStarted {
    /// Number of items queued for re-fetch.
    pub count: usize,
}

impl EventEmitter<ThumbnailRefreshStarted> for LibraryController {}

// ── ThumbnailRefreshCompleted
// ──────────────────────────────────────────

/// Emitted by `LibraryController` when a "Refresh Thumbnails" batch started by
/// [`ThumbnailRefreshStarted`] has finished draining.
///
/// The receiver should show a toast summarizing success/failure counts.
pub struct ThumbnailRefreshCompleted {
    /// Number of covers that re-fetched successfully.
    pub succeeded: usize,
    /// Number of covers that failed to re-fetch.
    pub failed:    usize,
}

impl EventEmitter<ThumbnailRefreshCompleted> for LibraryController {}

// ── ThumbnailRefreshNoOp
// ────────────────────────────────────────────────────

/// Emitted by `LibraryController` when the "Refresh Thumbnails" catalog menu
/// action is invoked but no catalog item has a `cover_url`.
///
/// The receiver should show a brief "nothing to refresh" toast instead of
/// leaving the action silently doing nothing.
pub struct ThumbnailRefreshNoOp;

impl EventEmitter<ThumbnailRefreshNoOp> for LibraryController {}

// ── StartupAuthFailed
// ─────────────────────────────────────────────────────────

/// Emitted by `SettingsController` when background startup re-authentication
/// fails.
///
/// The receiver should clear the auth-pending state so the banner transitions
/// to the normal "Not signed in" notice.
pub struct StartupAuthFailed;

impl EventEmitter<StartupAuthFailed> for SettingsController {}

// ── CacheCleared
// ───────────────────────────────────────────────────────────────

/// Emitted by `SettingsController` after the on-disk app cache has been
/// deleted.
///
/// The receiver should clear `LibraryController`'s in-memory catalog and
/// collections and force a fresh live fetch, so cleared content disappears from
/// the UI immediately instead of lingering until the next unrelated reload.
pub struct CacheCleared;

impl EventEmitter<CacheCleared> for SettingsController {}

// ── LowDiskSpaceWarning
// ────────────────────────────────────────────────────

/// Emitted by `LibraryController` when a `request_*` gating wrapper finds
/// free disk space insufficient for the download it was about to queue.
///
/// The receiver should show a confirmation dialog naming the shortfall,
/// calling `confirm_pending_download`/`cancel_pending_download` on the
/// controller from the dialog's ok/cancel handlers.
pub struct LowDiskSpaceWarning {
    /// Total size, in megabytes, of the files the pending action would
    /// enqueue.
    pub needed_mb: f64,
    /// Free disk space, in megabytes, at the storage root.
    pub free_mb:   f64,
}

impl EventEmitter<LowDiskSpaceWarning> for LibraryController {}

// ── TabsChanged
// ───────────────────────────────────────────────────────────────

/// Emitted when the open tab list or the active tab changes.
pub struct TabsChanged;

impl EventEmitter<TabsChanged> for TabsController {}
