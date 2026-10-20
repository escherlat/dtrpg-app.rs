//! Settings controller: owns open/closed state and file-opener overrides.
//!
//! The struct and its constructors live here; its methods are grouped by
//! concern into sibling files (`auth`, `storage`, `sign_in`, `panel`,
//! `file_openers`, `snapshot`, `wiring`), each its own `impl
//! SettingsController` block doing `use super::*;`.

use std::path::PathBuf;
use std::sync::Arc;

use dtrpg_core::credentials::{Credential, CredentialStore, KeyringCredentialStore};
use dtrpg_core::data::avatar::{avatar_cached, fetch_avatar_bytes};
use dtrpg_core::data::catalog_cache::load_cache_metadata;
use dtrpg_core::data::constants::{KEYRING_API_KEY, KEYRING_SERVICE};
use dtrpg_core::data::file_openers::{AddOutcome, FileOpenerConfig, FileOpenerEntry};
use dtrpg_core::data::native_preferences::{NativePreferences, NativePreferencesFile};
use dtrpg_core::data::paths::{app_cache_dir, cache_dir, covers_dir};
use dtrpg_core::data::profile::ProfileConfig;
use dtrpg_core::data::storage::placement::PlacementCondition;
use dtrpg_core::data::storage::placement::PlacementRule;
use dtrpg_core::data::storage::roots::StorageRoot;
use dtrpg_core::data::storage::{StorageConfig, StorageError, validate_writable};
use dtrpg_core::data::token_status::{TokenStatus, TokenStatusHandle};
use dtrpg_core::services::LoginService;
use gpui::{Context, Entity, EventEmitter};
use gpui_kit::component::input::InputState;
use gpui_kit::component::select::SelectState;
use rust_i18n::t;

use crate::data::events::{
    CacheCleared, LogoutRequested, SettingsChanged, SignInSucceeded, StartupAuthBegun,
    StartupAuthFailed,
};
use crate::data::token_status_global::TokenStatusGlobal;

/// State for one Appearance-page font picker: a searchable list of every
/// font name installed on the user's system (see
/// `cx.text_system().all_font_names()`), not a curated list.
pub type FontSelectState = Entity<SelectState<Vec<String>>>;

/// Placement rule condition currently being edited in Downloads settings.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RuleEditorType {
    Publisher,
    Collection,
    Size,
}

/// Per-type counts of regenerable app cache data, computed from disk and
/// cached on the controller — see [`SettingsController::refresh_cache_counts`].
#[derive(Clone, Copy, Default)]
pub struct CacheCounts {
    /// Number of items in the cached catalog/collections metadata.
    pub metadata_items:             usize,
    /// Number of cached cover thumbnail files.
    pub cover_thumbnails:           usize,
    /// `true` if an avatar image is currently cached.
    pub avatar_cached:              bool,
    /// Unix timestamp (seconds) when the catalog/collections metadata cache
    /// was last written. `None` when no cache metadata exists yet.
    pub metadata_saved_at_secs:     Option<i64>,
    /// Unix timestamp (seconds) of the last per-item availability check
    /// batch (manual or automatic). `None` when no batch has run yet, or
    /// predates the field being tracked.
    pub last_item_check_batch_secs: Option<i64>,
}

// ── AuthState
// ─────────────────────────────────────────────────────────────────

/// Tracks the current authentication state and cached avatar data.
#[derive(Clone)]
pub enum AuthState {
    /// No user is signed in.
    LoggedOut,
    /// A user is signed in; avatar bytes are fetched asynchronously after
    /// login.
    LoggedIn {
        /// Account email address, if known.
        email:        Option<String>,
        /// Cached Gravatar image bytes, or `None` while the fetch is in flight
        /// or unavailable.
        avatar_bytes: Option<Arc<Vec<u8>>>,
    },
}

/// Snapshot of auth state for a single render pass.
#[derive(Clone)]
pub struct AuthStateSnapshot {
    /// `true` when a user is signed in.
    pub is_logged_in:    bool,
    /// Account email address when signed in.
    pub email:           Option<String>,
    /// First character of the email, uppercased — used as the avatar fallback
    /// initial.
    pub display_initial: Option<char>,
    /// Cached avatar image bytes from Gravatar, or `None`.
    pub avatar_bytes:    Option<Arc<Vec<u8>>>,
    /// Masked API key hint (first 4 + bullets + last 1), set at sign-in time.
    pub api_key_hint:    Option<String>,
    /// When the current access token was acquired and when it expires, or
    /// `None` when signed out or no token has been issued yet.
    pub token_status:    Option<TokenStatus>,
}

// ── SettingsController
// ────────────────────────────────────────────────────────

/// Snapshot of settings state needed by the views for a single render pass.
pub struct SettingsSnapshot {
    pub is_open:                        bool,
    pub file_openers:                   Vec<FileOpenerEntry>,
    /// `true` when credentials are present in the keyring.
    pub is_authenticated:               bool,
    /// Resolved storage root path (override or platform default).
    pub storage_root_path:              PathBuf,
    /// Configured download roots, including the immutable default root.
    pub storage_roots:                  Vec<StorageRoot>,
    /// Ordered placement rules used for future downloads.
    pub storage_rules:                  Vec<PlacementRule>,
    /// Input state for a new publisher placement rule.
    pub publisher_rule_input:           Option<Entity<InputState>>,
    pub collection_rule_input:          Option<Entity<InputState>>,
    pub size_rule_input:                Option<Entity<InputState>>,
    pub rule_target_root_id:            u32,
    pub rule_editor_type:               RuleEditorType,
    /// `true` when the configured storage root is unreachable (e.g. unmounted
    /// volume).
    pub storage_unavailable:            bool,
    /// `true` when the configured storage root exists on disk.
    pub storage_path_exists:            bool,
    /// Current auth state for the toolbar avatar button.
    pub auth:                           AuthStateSnapshot,
    /// Current value of the email draft field in the Account tab.
    pub email_draft:                    String,
    /// Current value of the password draft field in the Account tab.
    pub password_draft:                 String,
    /// `true` while a sign-in request is in flight.
    pub sign_in_in_progress:            bool,
    /// Error message from the last failed sign-in attempt, if any.
    pub sign_in_error:                  Option<String>,
    /// Shared input state for the email text field in the Account tab.
    pub email_input:                    Option<Entity<InputState>>,
    /// Shared input state for the password text field in the Account tab.
    pub password_input:                 Option<Entity<InputState>>,
    /// Current draft value of the storage path text field.
    pub storage_path_draft:             String,
    /// Shared input state for the storage path text field in the Storage tab.
    pub storage_path_input:             Option<Entity<InputState>>,
    /// Application path picked via the native file dialog, awaiting an
    /// extension typed inline in the File Openers list. `None` when no add
    /// is in progress.
    pub pending_file_opener:            Option<PathBuf>,
    /// Index of the currently active settings page.
    pub active_page_ix:                 usize,
    /// Current per-type counts of cached data, for the Advanced section's
    /// "Cache details" area.
    pub cache_counts:                   CacheCounts,
    /// Shared maximum number of concurrent thumbnail/download fetches.
    pub max_concurrent_downloads:       usize,
    /// Shared input state for the "Max concurrent downloads" number field in
    /// the Storage tab.
    pub max_concurrent_downloads_input: Option<Entity<InputState>>,
    /// `true` when completing a download should also symlink the item under
    /// `{root}/collections/{collection name}/` for each collection it
    /// belongs to.
    pub create_collections:             bool,
    /// The Native Integration switches: whether tags move between the library
    /// and files' tags, and whether that is checked at launch.
    pub native:                         NativePreferencesFile,
    /// User-configured "Recently Updated" sidebar window, in days.
    pub recently_updated_window_days:   u32,
    /// Shared input state for the "Recently Updated window" number field in
    /// the Advanced tab.
    pub recently_updated_window_input:  Option<Entity<InputState>>,
    /// Font picker state for the Appearance page's body-font row.
    pub body_font_select:               Option<FontSelectState>,
    /// Font picker state for the Appearance page's value-font row.
    pub value_font_select:              Option<FontSelectState>,
    /// Font picker state for the Appearance page's label-font row.
    pub label_font_select:              Option<FontSelectState>,
    /// Font picker state for the Appearance page's monospace-font row.
    pub mono_font_select:               Option<FontSelectState>,
}

/// Owns all mutable settings state: panel visibility, file-opener overrides,
/// catalog storage configuration, and sign-in form state.
pub struct SettingsController {
    is_open:                        bool,
    file_openers:                   FileOpenerConfig,
    is_authenticated:               bool,
    auth_state:                     AuthState,
    storage:                        StorageConfig,
    native:                         NativePreferences,
    storage_unavailable:            bool,
    storage_path_exists:            bool,
    login_service:                  Arc<dyn LoginService>,
    email_draft:                    String,
    password_draft:                 String,
    sign_in_in_progress:            bool,
    sign_in_error:                  Option<String>,
    /// Input state for the email text field; set by the root view after
    /// creation.
    email_input:                    Option<Entity<InputState>>,
    /// Input state for the password text field; set by the root view after
    /// creation.
    password_input:                 Option<Entity<InputState>>,
    /// Draft value of the storage path text field.
    storage_path_draft:             String,
    /// Input state for the storage path text field; set by the root view after
    /// creation.
    storage_path_input:             Option<Entity<InputState>>,
    /// Masked API key hint computed at sign-in time (first 4 + bullets + last
    /// 1).
    api_key_hint:                   Option<String>,
    /// Shared slot the SDK services publish the current access token's timing
    /// into; read when the Account page renders.
    token_status:                   TokenStatusHandle,
    /// Application path picked via the native file dialog, awaiting an
    /// extension typed inline in the File Openers list. `None` when no add
    /// is in progress.
    pending_file_opener:            Option<PathBuf>,
    /// Index of the currently active settings page.
    ///
    /// Persisted to [`dtrpg_core::data::ui_state::UiState`] so the settings
    /// window reopens on the same page it was closed on, rather than always
    /// resetting to the first page — gpui-kit's `Settings` widget
    /// tracks page selection in its own per-window state with no way to read
    /// it back, so this is tracked here instead and used to drive our own
    /// page navigation in `render_settings_panel`.
    active_page_ix:                 usize,
    /// Cached per-type cache counts for the Advanced page. Refreshed on the
    /// panel-open, page-change, and cache-clear paths — see
    /// [`Self::refresh_cache_counts`] — rather than recomputed on every
    /// render pass, so rendering performs no disk I/O.
    cache_counts:                   CacheCounts,
    /// `true` when the email input should be focused on the settings
    /// window's next render pass. Set by
    /// [`Self::request_email_focus_on_account_tab`] and consumed (cleared)
    /// by the settings window's render — see `SettingsWindowView::render`.
    focus_email_pending:            bool,
    /// Input state for the "Recently Updated window" number field; set by
    /// the root view after creation.
    recently_updated_window_input:  Option<Entity<InputState>>,
    /// Input state for the "Max concurrent downloads" number field; set by
    /// the root view after creation.
    max_concurrent_downloads_input: Option<Entity<InputState>>,
    publisher_rule_input:           Option<Entity<InputState>>,
    collection_rule_input:          Option<Entity<InputState>>,
    size_rule_input:                Option<Entity<InputState>>,
    rule_target_root_id:            u32,
    rule_editor_type:               RuleEditorType,
    /// Font picker state for the Appearance page's body-font row; set by the
    /// root view after creation.
    body_font_select:               Option<FontSelectState>,
    /// Font picker state for the Appearance page's value-font row; set by the
    /// root view after creation.
    value_font_select:              Option<FontSelectState>,
    /// Font picker state for the Appearance page's label-font row; set by the
    /// root view after creation.
    label_font_select:              Option<FontSelectState>,
    /// Font picker state for the Appearance page's monospace-font row; set by
    /// the root view after creation.
    mono_font_select:               Option<FontSelectState>,
}

impl SettingsController {
    /// Creates a controller, loading the file-opener list from disk.
    ///
    /// Checks the platform keyring to determine initial auth state, and
    /// verifies the configured storage root is accessible. Spawns a
    /// background check for path existence.
    ///
    /// When no storage override is configured (first launch, or after a reset),
    /// the platform default download directory is created automatically if
    /// it does not yet exist, rather than surfacing a "storage folder does
    /// not exist" warning for a location the app itself owns. A user-chosen
    /// override path that is missing is left untouched and still surfaces
    /// the warning — that more likely indicates an unmounted volume than a
    /// fresh install.
    pub fn new(login_service: Box<dyn LoginService>, cx: &mut Context<Self>) -> Self {
        let file_openers = FileOpenerConfig::load();
        let loaded_credential =
            KeyringCredentialStore::new(KEYRING_SERVICE, KEYRING_API_KEY).load()
                                                                         .ok()
                                                                         .flatten();
        let is_authenticated = loaded_credential.is_some();
        // Pre-fill email from the stored credential; fall back to ProfileConfig
        // for users whose email was saved there before this field moved to the
        // keyring entry.
        let email_draft =
            loaded_credential.as_ref()
                             .and_then(|c| c.email.as_deref())
                             .map(str::to_owned)
                             .or_else(|| ProfileConfig::load().email().map(str::to_owned))
                             .unwrap_or_default();

        let storage = StorageConfig::load();
        if storage.is_default()
           && !storage.is_accessible()
           && let Err(e) = storage.ensure_root_exists()
        {
            tracing::warn!(
                error = %e,
                path = %storage.root_path().display(),
                "failed to create default download directory"
            );
        }
        let storage_unavailable = !storage.is_accessible();
        if storage_unavailable {
            tracing::warn!(
                path = %storage.root_path().display(),
                "configured storage root is not accessible"
            );
        }
        let initial_path = storage.root_path();
        let storage_path_draft = initial_path.to_string_lossy().into_owned();

        let mut ctrl = Self { is_open: false,
                              file_openers,
                              is_authenticated,
                              auth_state: AuthState::LoggedOut,
                              storage_path_exists: true,
                              storage,
                              native: NativePreferences::load(),
                              storage_unavailable,
                              login_service: Arc::from(login_service),
                              email_draft,
                              password_draft: String::new(),
                              sign_in_in_progress: false,
                              sign_in_error: None,
                              email_input: None,
                              password_input: None,
                              storage_path_draft,
                              storage_path_input: None,
                              api_key_hint: None,
                              token_status: cx.try_global::<TokenStatusGlobal>()
                                              .map(|g| g.0.clone())
                                              .unwrap_or_default(),
                              pending_file_opener: None,
                              active_page_ix:
                                  dtrpg_core::data::ui_state::UiState::load().settings_page_ix()
                                                                             .unwrap_or(0),
                              cache_counts: CacheCounts::default(),
                              focus_email_pending: false,
                              recently_updated_window_input: None,
                              max_concurrent_downloads_input: None,
                              publisher_rule_input: None,
                              collection_rule_input: None,
                              size_rule_input: None,
                              rule_target_root_id: 0,
                              rule_editor_type: RuleEditorType::Publisher,
                              body_font_select: None,
                              value_font_select: None,
                              label_font_select: None,
                              mono_font_select: None };
        ctrl.check_storage_path_exists(initial_path, cx);
        ctrl.refresh_cache_counts();
        ctrl
    }

    /// Builds a controller in-memory, without touching the platform keyring
    /// or the shared on-disk app config files.
    ///
    /// [`new`](Self::new) always reads the real keyring and config files
    /// (see [`StorageConfig::for_test`] for why that's unsafe in tests); this
    /// constructor exists so tests can exercise `cx`-requiring methods
    /// without those side effects.
    #[cfg(test)]
    pub(crate) fn for_test(login_service: Arc<dyn LoginService>) -> Self {
        Self { is_open: false,
               file_openers: FileOpenerConfig::default(),
               is_authenticated: false,
               auth_state: AuthState::LoggedOut,
               storage_path_exists: true,
               storage: StorageConfig::for_test(std::env::temp_dir()),
               native: NativePreferences::in_memory(),
               storage_unavailable: false,
               login_service,
               email_draft: String::new(),
               password_draft: String::new(),
               sign_in_in_progress: false,
               sign_in_error: None,
               email_input: None,
               password_input: None,
               storage_path_draft: String::new(),
               storage_path_input: None,
               api_key_hint: None,
               token_status: TokenStatusHandle::default(),
               pending_file_opener: None,
               active_page_ix: 0,
               cache_counts: CacheCounts::default(),
               focus_email_pending: false,
               recently_updated_window_input: None,
               max_concurrent_downloads_input: None,
               publisher_rule_input: None,
               collection_rule_input: None,
               size_rule_input: None,
               rule_target_root_id: 0,
               rule_editor_type: RuleEditorType::Publisher,
               body_font_select: None,
               value_font_select: None,
               label_font_select: None,
               mono_font_select: None }
    }

    /// Updates the storage path draft field.
    pub fn set_storage_path_draft(&mut self, value: String, cx: &mut Context<Self>) {
        self.storage_path_draft = value;
        cx.emit(SettingsChanged);
    }

    /// Switches the active settings page and persists the choice so the
    /// settings window reopens on the same page next time.
    pub fn set_active_page_ix(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.active_page_ix = ix;
        self.refresh_cache_counts();
        dtrpg_core::data::ui_state::UiState::load().save_settings_page_ix(ix);
        cx.notify();
    }

    /// Switches to the Account tab and marks the email input to be focused
    /// on the settings window's next render pass.
    ///
    /// Used by the "Login to DriveThruRPG" notification banner action so the
    /// user can start typing immediately instead of having to click into the
    /// field first.
    pub fn request_email_focus_on_account_tab(&mut self, cx: &mut Context<Self>) {
        self.set_active_page_ix(0, cx);
        self.focus_email_pending = true;
    }

    /// Returns and clears the pending-focus flag set by
    /// [`Self::request_email_focus_on_account_tab`], so the settings
    /// window's render pass consumes it exactly once.
    pub fn take_focus_email_pending(&mut self) -> bool {
        std::mem::take(&mut self.focus_email_pending)
    }
}

impl EventEmitter<SettingsChanged> for SettingsController {}
impl EventEmitter<LogoutRequested> for SettingsController {}

mod auth;
mod file_openers;
mod native;
mod panel;
mod sign_in;
mod snapshot;
mod storage;
mod wiring;

#[cfg(test)]
mod tests;
