//! Service traits and error types for library and collections data access.

pub mod collections;
pub mod network_monitor;
pub mod retry;

use crate::data::library::LibraryItem;

/// The type of service failure returned by library operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibraryServiceErrorKind {
    /// Request failed due to transient connectivity or SDK configuration.
    Network,
    /// Request failed due to session or authentication state.
    Session,
    /// Request referenced a non-existent item.
    NotFound,
    /// Session has expired and re-authentication is required.
    ///
    /// # TODO
    ///
    /// Full token-refresh handling is deferred until `connect-sdk-to-rust-app`
    /// lands.
    NeedsReauth,
    /// Request failed due to an HTTP 429 (Too Many Requests) response.
    RateLimited,
}

/// Error returned by library service operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryServiceError {
    /// The machine-classified failure kind.
    pub kind:        LibraryServiceErrorKind,
    /// Human-readable baseline error message.
    pub message:     String,
    /// The server-specified wait duration from a `Retry-After` header, when
    /// the failure was a [`LibraryServiceErrorKind::RateLimited`] response
    /// that included one.
    pub retry_after: Option<std::time::Duration>,
}

impl LibraryServiceError {
    /// Creates a new service error.
    pub fn new(kind: LibraryServiceErrorKind, message: impl Into<String>) -> Self {
        Self { kind,
               message: message.into(),
               retry_after: None }
    }

    /// Attaches a `Retry-After`-derived wait duration to this error.
    #[must_use]
    pub fn with_retry_after(mut self, retry_after: Option<std::time::Duration>) -> Self {
        self.retry_after = retry_after;
        self
    }

    /// Returns a multi-line string suitable for display in the activity panel,
    /// combining the error message with a kind-specific user hint.
    pub fn panel_detail(&self) -> String {
        let hint = match self.kind {
            LibraryServiceErrorKind::Network => "Check your internet connection and try again.",
            LibraryServiceErrorKind::Session => {
                "Your access token may be missing or expired. Try signing out and back in."
            }
            LibraryServiceErrorKind::NotFound => "The requested item could not be found.",
            LibraryServiceErrorKind::NeedsReauth => {
                "Your session has expired. Please sign out and sign back in."
            }
            LibraryServiceErrorKind::RateLimited => {
                "DriveThruRPG is rate-limiting requests. Retrying automatically."
            }
        };
        format!("{self}\n{hint}")
    }
}

impl std::fmt::Display for LibraryServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({:?})", self.message, self.kind)
    }
}

impl std::error::Error for LibraryServiceError {}

/// Service boundary consumed by the library view model.
///
/// Implementations may be SDK-backed HTTP adapters or deterministic test stubs.
/// The service is responsible for fetching and mapping all data; callers
/// receive fully-formed [`LibraryItem`] values ready for presentation.
pub trait LibraryService: Send + Sync + 'static {
    /// Loads the full library item list.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryServiceError`] if the request fails or the session is
    /// invalid.
    fn list_items(&self) -> Result<Vec<LibraryItem>, LibraryServiceError>;

    /// Loads library items page-by-page, invoking `on_page` after each page
    /// arrives.
    ///
    /// The default implementation calls [`list_items`] and delivers all items
    /// in a single `on_page` call. Implementations that have access to
    /// pagination should override this to call `on_page` incrementally so
    /// callers can update the UI without waiting for all pages.
    ///
    /// `on_total` is called at most once, before the first `on_page` delivery,
    /// with an estimated total item count. Implementations that cannot
    /// determine the total pass `None` or simply never call `on_total`. The
    /// default implementation never calls it.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryServiceError`] if any page request fails. Items
    /// delivered to `on_page` before the failure are not rolled back.
    ///
    /// [`list_items`]: LibraryService::list_items
    fn list_items_paged(&self, on_page: &mut dyn FnMut(Vec<LibraryItem>),
                        on_total: Option<&mut dyn FnMut(usize)>)
                        -> Result<(), LibraryServiceError> {
        let _ = on_total;
        on_page(self.list_items()?);
        Ok(())
    }

    /// Returns the total number of items in the user's library without fetching
    /// their content.
    ///
    /// Returns `None` when a cheap count is not supported by this
    /// implementation; callers should treat `None` as "count unknown" and
    /// fall back to trusting the local cache.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] if the request fails or the session is
    /// invalid.
    fn count_items(&self) -> Option<Result<usize, LibraryServiceError>> {
        None
    }

    /// Loads detail data for a selected item by its numeric API identifier.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] with kind
    /// [`LibraryServiceErrorKind::NotFound`] if the id does not match any
    /// item, or a network/session error if the request fails.
    fn get_item(&self, id: u64) -> Result<LibraryItem, LibraryServiceError>;

    /// Fetches items added or changed since `since_iso8601` (an ISO 8601
    /// timestamp), calling `on_page` incrementally as pages arrive — a
    /// cheaper alternative to a full
    /// [`list_items_paged`](Self::list_items_paged) fetch when only a few
    /// items changed.
    ///
    /// Returns `None` when a date-filtered fetch is not supported by this
    /// implementation; callers should treat `None` as "unsupported" and fall
    /// back to a full paginated fetch, mirroring
    /// [`count_items`](Self::count_items)'s convention.
    ///
    /// # Errors
    ///
    /// The inner `Result` is a [`LibraryServiceError`] if any page request
    /// fails. Items delivered to `on_page` before the failure are not rolled
    /// back.
    fn list_items_updated_since(&self, since_iso8601: &str,
                                on_page: &mut dyn FnMut(Vec<LibraryItem>))
                                -> Option<Result<(), LibraryServiceError>> {
        let _ = (since_iso8601, on_page);
        None
    }

    /// Downloads a single file to `dest`, writing to a `{dest}.part` temp
    /// path and renaming to `dest` only once the transfer completes in full.
    ///
    /// `order_product_id` and `index` identify the file per the download
    /// preparation API (`index` is the file's position within its entry's
    /// file list, i.e. [`crate::data::library::LibraryItemFile::index`]).
    ///
    /// `cancel` is polled during the transfer; once set, the transfer stops
    /// promptly, `{dest}.part` is deleted, and an error is returned. Callers
    /// that already track cancellation via the same flag (see
    /// `download-queue`) can treat any error alongside an already-set `cancel`
    /// as an expected cancellation rather than a real failure.
    ///
    /// On a retryable transfer failure, an implementation may retry the
    /// download with exponential backoff before returning an error. `on_retry`,
    /// if present, is called once per retry with the attempt number about to
    /// be retried and the delay before it, so callers can surface
    /// retry-in-progress state (e.g. an activity label). Implementations that
    /// never fail transiently (e.g. test stubs) may ignore this parameter.
    ///
    /// # Errors
    ///
    /// Returns a [`LibraryServiceError`] on any preparation, network, or I/O
    /// failure, or if the transfer is cancelled. `dest` is left untouched
    /// (never partially written) on any error path.
    fn download_item(&self, order_product_id: u64, index: u32, dest: &std::path::Path,
                     cancel: &std::sync::atomic::AtomicBool,
                     on_retry: Option<&mut dyn FnMut(u32, std::time::Duration)>)
                     -> Result<(), LibraryServiceError>;
}

// ── LoginService
// ──────────────────────────────────────────────────────────────

/// Tokens returned by a successful login.
#[derive(Clone)]
pub struct LoginTokens {
    /// Short-lived JWT bearer token for API requests.
    pub access_token:      String,
    /// Long-lived token used to refresh the access token.
    pub refresh_token:     String,
    /// Unix timestamp (seconds) at which the refresh token expires.
    pub refresh_token_ttl: u64,
}

/// Error returned by [`LoginService::authenticate`].
#[derive(Debug, Clone)]
pub struct LoginError(pub String);

impl std::fmt::Display for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Service boundary for the login flow.
///
/// Implementations first exchange an email/password pair for an application
/// key via [`login_with_credentials`], then exchange that key for session
/// tokens via [`authenticate`].
///
/// [`login_with_credentials`]: LoginService::login_with_credentials
/// [`authenticate`]: LoginService::authenticate
pub trait LoginService: Send + Sync + 'static {
    /// Exchanges an email/password pair for a DriveThruRPG application key.
    ///
    /// Returns the application key string on success. The caller must then
    /// pass that key to [`authenticate`] to obtain session tokens.
    ///
    /// # Errors
    ///
    /// Returns [`LoginError`] on invalid credentials, network failure, or
    /// if the server declines to issue an application key.
    ///
    /// [`authenticate`]: LoginService::authenticate
    fn login_with_credentials(&self, email: &str, password: &str) -> Result<String, LoginError>;

    /// Authenticates with the given application key and returns session tokens.
    ///
    /// Used for silent startup re-authentication from a stored key.
    ///
    /// # Errors
    ///
    /// Returns [`LoginError`] if authentication fails (network error, invalid
    /// key, etc.).
    fn authenticate(&self, api_key: &str) -> Result<LoginTokens, LoginError>;
}

#[cfg(test)]
pub mod stub;
