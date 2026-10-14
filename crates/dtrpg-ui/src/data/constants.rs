//! Constants

//! Well-known service name and account key constants for the credential store.
//!
//! All credential store call sites MUST use these constants rather than
//! inline strings to ensure consistent namespacing and enable targeted
//! deletion on uninstall.

/// Default number of days within which an item's `date_added` or
/// `date_updated` counts as "recently updated", when no user preference has
/// been saved.
pub const RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS: u32 = 30;
/// Lower bound for the "Recently Updated window" stepper, in days.
pub const RECENTLY_UPDATED_WINDOW_MIN_DAYS: u32 = 7;
/// Upper bound for the "Recently Updated window" stepper, in days.
pub const RECENTLY_UPDATED_WINDOW_MAX_DAYS: u32 = 90;

pub const MONTH_ABBRS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub const APP_NAME: &str = "dtrpg";

/// Reverse-domain bundle identifier.
///
/// macOS convention names filesystem locations (Application Support,
/// Preferences, Caches) with the reverse-domain bundle ID rather than the bare
/// app name. Used by [`crate::data::paths`] for those directories; also reused
/// as the keyring service namespace via [`KEYRING_SERVICE`].
pub const MACOS_BUNDLE_ID: &str = "com.pilgrimagesoftware.dtrpg";

pub const COLLECTIONS_CACHE_FILE: &str = "collections_cache.json";
pub const COLLECTIONS_CACHE_TMP: &str = "collections_cache.json.tmp";
pub const CATALOG_CACHE_FILE: &str = "catalog_cache.json";
pub const CATALOG_CACHE_TMP: &str = "catalog_cache.json.tmp";
pub const CATALOG_CACHE_METADATA_FILE: &str = "catalog_cache_meta.json";
pub const AVATAR_CACHE_FILE: &str = "avatar";

/// 7 days in seconds — caches older than this are considered stale.
pub const STALE_SECS: u64 = 7 * 24 * 60 * 60;

/// Number of live catalog pages received before the accumulating page buffer
/// is checkpointed to disk, whichever of this or
/// [`CATALOG_CHECKPOINT_MIN_INTERVAL_SECS`] elapses first.
///
/// See `catalog-cache-checkpointing`: without a periodic checkpoint, an app
/// quit or crash partway through a large live load leaves nothing on disk
/// even though many pages may already have been fetched.
pub const CATALOG_CHECKPOINT_PAGE_INTERVAL: u32 = 5;

/// Minimum time between catalog cache checkpoints during a live load, in
/// seconds — see [`CATALOG_CHECKPOINT_PAGE_INTERVAL`].
pub const CATALOG_CHECKPOINT_MIN_INTERVAL_SECS: u64 = 10;

/// Minimum interval between user-requested full catalog reloads ("Catalog >
/// Reload"), keyed off `CacheMetadata::saved_at_secs`.
///
/// Distinct from the on-disk cache's 7-day passive staleness window (see
/// [`STALE_SECS`]): that constant answers "is the cached data old enough
/// that a *passive* load should refresh it," while this one answers "was a
/// *manual* reload already attempted moments ago." 60 seconds is long enough
/// to absorb accidental double-invocations (a stuck keybinding or an
/// impatient double-click) without meaningfully delaying a deliberate second
/// reload.
pub const FORCE_RELOAD_COOLDOWN_SECS: u64 = 60;

/// Minimum interval between re-checking the same catalog item's availability
/// against the server (on-demand, via viewing its details).
///
/// Short enough that a check still feels "fresh" relative to a browsing
/// session; long enough to absorb a user repeatedly reopening the same
/// item's detail view without issuing a redundant network call each time.
pub const ITEM_CHECK_COOLDOWN_SECS: u64 = 300;

/// Minimum interval between per-item availability check batches, whether
/// triggered manually or by the automatic periodic timer. Shared by both
/// triggers so neither can flood the API by stacking on top of the other.
pub const ITEM_CHECK_BATCH_COOLDOWN_SECS: u64 = 900;

/// Maximum number of items selected into a single availability check batch.
pub const ITEM_CHECK_BATCH_SIZE: usize = 50;

/// Interval between wake-ups of the automatic periodic check-batch timer
/// loop. Independent of `ITEM_CHECK_BATCH_COOLDOWN_SECS` — this is how often
/// the loop *wakes up to ask*, not how often a batch is actually allowed to
/// run; each wake calls `request_check_batch`, which applies the real
/// cooldown gate.
pub const ITEM_CHECK_BATCH_TIMER_SECS: u64 = 300;

/// Minimum interval between cover thumbnail fetch attempts for the same item.
///
/// Absorbs re-render churn (e.g. scrolling an item back into view) without
/// hammering the thumbnail source on every frame the item happens to be
/// visible.
pub const THUMBNAIL_COOLDOWN_SECS: u64 = 300;

/// Reverse-DNS service namespace used for all keyring entries.
pub const KEYRING_SERVICE: &str = MACOS_BUNDLE_ID;

/// Account key for the DriveThruRPG API key credential.
///
/// This is the only credential persisted to the keychain. Access tokens and
/// refresh tokens are kept in memory and re-acquired at startup.
pub const KEYRING_API_KEY: &str = "api-key";

pub const RECENT_CAP: usize = 25;
pub const EXPIRY_SECS: u64 = 15;
pub const ERROR_EXPIRY_SECS: u64 = 120;

/// Maximum number of entries retained in the durable alert history log.
///
/// Unlike the transient activity panel's `recent` list, entries here never
/// expire on a timer — the oldest entry is evicted only once this cap is
/// reached.
pub const ALERT_LOG_CAP: usize = 100;

/// Default width of the detail panel, in pixels.
pub const DETAIL_PANEL_DEFAULT_WIDTH: f32 = 320.0;
/// Minimum allowed detail panel width, in pixels.
pub const DETAIL_PANEL_MIN_WIDTH: f32 = 240.0;
/// Maximum allowed detail panel width, in pixels.
pub const DETAIL_PANEL_MAX_WIDTH: f32 = 600.0;
/// Maximum width of the detail panel's cover thumbnail, in pixels.
///
/// The thumbnail no longer grows with the panel past this size; instead it
/// stays centered horizontally at the top of the panel.
pub const DETAIL_PANEL_COVER_MAX_WIDTH: f32 = DETAIL_PANEL_DEFAULT_WIDTH;

/// Maximum character count for a detail tab's title before it is truncated
/// with an ellipsis.
///
/// The tab strip has no fixed per-tab width, so a long catalog item title
/// otherwise stretches its tab (and the whole strip) rather than eliding —
/// see [`crate::util::text::truncate_with_ellipsis`].
pub const DETAIL_TAB_TITLE_MAX_CHARS: usize = 40;

/// Width of the single-click item popover, in pixels.
pub const ITEM_POPOVER_WIDTH: f32 = 260.0;
/// Gap between the item popover and the catalog entry it's anchored beside.
pub const ITEM_POPOVER_MARGIN: f32 = 8.0;

/// Default body-font family, serif-leaning to match the app's old-style body
/// text. Only a starting point — the Appearance settings page lets the user
/// pick any font actually installed on their system (see
/// `cx.text_system().all_font_names()`), not just this default.
#[cfg(target_os = "macos")]
pub const DEFAULT_BODY_FONT: &str = "Hoefler Text";
/// Default body-font family, serif-leaning to match the app's old-style body
/// text.
#[cfg(target_os = "windows")]
pub const DEFAULT_BODY_FONT: &str = "Georgia";
/// Default body-font family, serif-leaning to match the app's old-style body
/// text.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const DEFAULT_BODY_FONT: &str = "Liberation Serif";

/// Default value-font family — the same family as [`DEFAULT_BODY_FONT`],
/// distinguished from body text by rendering at [`VALUE_FONT_SIZE_RATIO`] of
/// its size rather than by a different family.
pub const DEFAULT_VALUE_FONT: &str = DEFAULT_BODY_FONT;

/// Default label-font family, sans-serif, used to visually distinguish
/// field/row labels (e.g. the detail tab's metadata labels) from the default
/// serif body font.
#[cfg(target_os = "macos")]
pub const DEFAULT_LABEL_FONT: &str = "Gotham";
/// Default label-font family, sans-serif, used to visually distinguish
/// field/row labels from the default serif body font.
#[cfg(target_os = "windows")]
pub const DEFAULT_LABEL_FONT: &str = "Segoe UI";
/// Default label-font family, sans-serif, used to visually distinguish
/// field/row labels from the default serif body font.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const DEFAULT_LABEL_FONT: &str = "DejaVu Sans";

/// Default monospace-font family, used for fixed-width data such as the
/// masked API key hint.
#[cfg(target_os = "macos")]
pub const DEFAULT_MONO_FONT: &str = "Menlo";
/// Default monospace-font family, used for fixed-width data such as the
/// masked API key hint.
#[cfg(target_os = "windows")]
pub const DEFAULT_MONO_FONT: &str = "Consolas";
/// Default monospace-font family, used for fixed-width data such as the
/// masked API key hint.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const DEFAULT_MONO_FONT: &str = "Liberation Mono";

/// Shared UI text size, in points/pixels, that corresponds to a "Text Scale"
/// of `1.0` — matches `gpui`'s and `gpui_kit`'s own stock default so
/// nothing visibly changes until the user adjusts it in Settings >
/// Appearance.
pub const DEFAULT_UI_TEXT_SIZE: f32 = 16.0;
/// Minimum shared UI text size the Appearance page's stepper allows.
pub const MIN_UI_TEXT_SIZE: f32 = 12.0;
/// Maximum shared UI text size the Appearance page's stepper allows —
/// generous enough to meaningfully help low-vision users without breaking
/// layouts sized around the default.
pub const MAX_UI_TEXT_SIZE: f32 = 28.0;
/// Minimum "Text Scale" value the Appearance page's stepper allows.
pub const MIN_UI_TEXT_SCALE: f32 = MIN_UI_TEXT_SIZE / DEFAULT_UI_TEXT_SIZE;
/// Maximum "Text Scale" value the Appearance page's stepper allows.
pub const MAX_UI_TEXT_SCALE: f32 = MAX_UI_TEXT_SIZE / DEFAULT_UI_TEXT_SIZE;
/// Amount the Appearance page's +/- stepper adjusts the "Text Scale" value
/// by on each click.
pub const UI_TEXT_SCALE_STEP: f32 = 0.1;
/// Rendered size, in points, of the "body" font role at a "Text Scale" of
/// `1.0`.
pub const BODY_FONT_SIZE_PT: f32 = 14.0;
/// The "value" font role renders at this fraction of the "body" role's size
/// — same family by default (see [`DEFAULT_VALUE_FONT`]), distinguished from
/// body text by being slightly smaller rather than by a different typeface.
pub const VALUE_FONT_SIZE_RATIO: f32 = 0.9;
/// Rendered size, in points, of the "label" and "monospace" font roles at a
/// "Text Scale" of `1.0` — matching `gpui_kit::component::Theme`'s stock
/// 13px-on-16px default ratio, so code-like content (e.g. the masked API key
/// hint) stays visually smaller than body text even as the shared scale
/// changes.
pub const LABEL_MONO_FONT_SIZE_PT: f32 = 13.0;

/// Host (with port) the network monitor checks for DriveThruRPG API
/// endpoint-specific reachability. See
/// [`crate::services::network_monitor::NetworkMonitor::check_endpoint`].
pub const DTRPG_API_HOST: &str = "api.drivethrurpg.com:443";

/// Host (with port) the network monitor checks for Gravatar endpoint-specific
/// reachability before an avatar fetch.
pub const GRAVATAR_HOST: &str = "www.gravatar.com:443";

/// Retry attempts (including the first) for catalog synchronization requests.
/// See [`crate::services::retry`].
pub const CATALOG_SYNC_MAX_ATTEMPTS: u32 = 4;
/// Base backoff delay (seconds) for catalog synchronization retries.
pub const CATALOG_SYNC_RETRY_BASE_DELAY_SECS: u64 = 2;
/// Maximum backoff delay (seconds) for catalog synchronization retries.
pub const CATALOG_SYNC_RETRY_MAX_DELAY_SECS: u64 = 30;

/// Retry attempts (including the first) for cover/avatar image-cache fetches.
pub const IMAGE_CACHE_MAX_ATTEMPTS: u32 = 4;
/// Base backoff delay (seconds) for image-cache fetch retries.
pub const IMAGE_CACHE_RETRY_BASE_DELAY_SECS: u64 = 2;
/// Maximum backoff delay (seconds) for image-cache fetch retries.
pub const IMAGE_CACHE_RETRY_MAX_DELAY_SECS: u64 = 30;

/// Minimum interval between fresh-install catalog initialization requests,
/// gating redundant requests.
pub const CATALOG_FRESH_INSTALL_MIN_REQUEST_INTERVAL_SECS: u64 = 60;

/// How long a network-monitor reachability check result stays cached before a
/// new check is triggered.
pub const NETWORK_MONITOR_CACHE_TTL_SECS: u64 = 5;

/// Interval for the recurring long-running-session catalog-staleness timer.
pub const CATALOG_REFRESH_TIMER_INTERVAL_SECS: u64 = 3600;

/// Selection size above which a bulk-action button (Download, Remove
/// Download, Fetch Thumbnail, Add/Remove Collection, Open) asks for
/// confirmation before dispatching, per `catalog-bulk-actions`.
///
/// `Open` in particular can launch one external-app process per selected
/// item; a selection in the thousands has been observed to lock up the
/// default viewer app under that many simultaneous launches.
pub const BULK_ACTION_CONFIRM_THRESHOLD: usize = 20;
