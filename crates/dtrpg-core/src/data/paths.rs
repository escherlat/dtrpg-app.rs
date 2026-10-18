//! Platform directory resolution for the application.
//!
//! Centralizes the mapping from "data" / "preferences" / "cache" / "downloads"
//! concepts to the underlying platform directories, so no other module needs to
//! reason about `dirs::*_dir()` semantics directly.
//!
//! - **App data** ([`app_data_dir`]): locally-generated application data that
//!   is not a cache and not user-editable preferences — e.g. `ui_state.toml`
//!   (panel widths, window bounds, last-active settings tab: regenerated from
//!   defaults if lost, not something a user would look for or hand-edit).
//! - **App cache** ([`app_cache_dir`], [`cache_dir`]): regenerable data such as
//!   the catalog/collections cache and the avatar cache.
//! - **App preferences** ([`app_preferences_dir`]): small user preference files
//!   (profile, download location override, `ui.toml`'s theme/fonts/
//!   text-scale/Collections-sort). On macOS this resolves to
//!   `~/Library/Preferences`, distinct from Application Support —
//!   `dirs::config_dir()` maps to Application Support on macOS and would
//!   otherwise collide with cache data.
//! - **Default download directory** ([`default_download_dir`]): where catalog
//!   content is downloaded by default, before any user override. This is
//!   user-visible content and must not live alongside app cache or preferences.
//!
//! All four of the above are always directories, never bare files — individual
//! data files (cache JSON, preference TOML, downloaded items) live inside them.
//!
//! On macOS, [`app_data_dir`], [`app_cache_dir`], and [`app_preferences_dir`]
//! use the reverse-domain bundle identifier ([`MACOS_BUNDLE_ID`]) as the
//! directory name, per macOS convention (e.g. `~/Library/Application
//! Support/com.example.app/`). The download directory is user-visible content,
//! not an app-managed location, so it keeps the plain app name on every
//! platform.

use std::path::PathBuf;

#[allow(unused_imports)]
use crate::data::constants::{APP_NAME, MACOS_BUNDLE_ID};

/// The directory name used under platform data/cache/preferences roots.
///
/// Reverse-domain bundle identifier on macOS, per platform convention; the
/// plain app name everywhere else.
#[cfg(target_os = "macos")]
const APP_DIR_NAME: &str = MACOS_BUNDLE_ID;
#[cfg(not(target_os = "macos"))]
const APP_DIR_NAME: &str = APP_NAME;

/// Returns the directory for locally-generated application data.
///
/// Falls back to the current directory if the platform data directory cannot be
/// determined.
pub fn app_data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from("."))
                    .join(APP_DIR_NAME)
}

/// Returns the directory for regenerable application cache data: catalog
/// metadata ([`cache_dir`]), cover thumbnails ([`covers_dir`]), and the
/// avatar cache.
///
/// On macOS this resolves to `~/Library/Caches/{MACOS_BUNDLE_ID}` — the
/// platform convention for exactly this kind of data, deliberately: the OS
/// may clear this directory under disk pressure, and everything here is
/// re-fetchable from the DriveThruRPG API or Gravatar, so that's an
/// acceptable trade-off rather than a bug. It is not the same directory as
/// [`app_data_dir`] (Application Support), which holds state that isn't
/// safe to have cleared out from under the app.
///
/// Falls back to the current directory if the platform cache directory cannot
/// be determined.
pub fn app_cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."))
                     .join(APP_DIR_NAME)
}

/// Returns the directory for small user preference files.
///
/// On macOS this resolves to `~/Library/Preferences`, kept distinct from
/// [`app_data_dir`] and [`app_cache_dir`] (which map to Application Support and
/// Caches respectively). On Linux and Windows, `dirs` has no separate
/// preferences concept, so this falls back to the platform config directory.
pub fn app_preferences_dir() -> PathBuf {
    dirs::preference_dir().unwrap_or_else(|| PathBuf::from("."))
                          .join(APP_DIR_NAME)
}

/// Returns the platform default directory for downloaded catalog content.
///
/// Used only as the initial default; the user may override this via Settings.
/// Always named with the plain app name — this is user-visible content, not an
/// app-managed location, so it does not use the macOS bundle identifier.
pub fn default_download_dir() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| PathBuf::from("."))
                        .join(APP_NAME)
}

/// Returns the directory used for cached catalog/collections metadata.
///
/// Always lives under [`app_cache_dir`], independent of the user's chosen
/// download location — cache lifetime is tied to the app installation, not to
/// where the user stores downloaded files.
pub fn cache_dir() -> PathBuf {
    app_cache_dir().join("metadata")
}

/// Returns the directory used for cached cover thumbnail image bytes.
///
/// Always lives under [`app_cache_dir`] — cover thumbnails are regenerable
/// content fetched from the DriveThruRPG image CDN, not user-downloaded catalog
/// files, so they belong alongside the catalog/collections metadata cache
/// rather than in the user's chosen download location.
pub fn covers_dir() -> PathBuf {
    app_cache_dir().join("covers")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_data_dir_ends_with_app_dir_name() {
        assert!(app_data_dir().ends_with(APP_DIR_NAME));
    }

    #[test]
    fn app_cache_dir_ends_with_app_dir_name() {
        assert!(app_cache_dir().ends_with(APP_DIR_NAME));
    }

    #[test]
    fn app_preferences_dir_ends_with_app_dir_name() {
        assert!(app_preferences_dir().ends_with(APP_DIR_NAME));
    }

    #[test]
    fn default_download_dir_ends_with_app_name() {
        assert!(default_download_dir().ends_with(APP_NAME));
    }

    #[test]
    fn cache_dir_is_under_app_cache_dir() {
        assert_eq!(cache_dir(), app_cache_dir().join("metadata"));
    }

    #[test]
    fn covers_dir_is_under_app_cache_dir() {
        assert_eq!(covers_dir(), app_cache_dir().join("covers"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_uses_reverse_domain_bundle_id() {
        assert_eq!(APP_DIR_NAME, MACOS_BUNDLE_ID);
        assert!(APP_DIR_NAME.contains('.'));
    }
}
