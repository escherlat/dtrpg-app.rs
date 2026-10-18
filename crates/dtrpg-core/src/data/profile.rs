//! User profile config: optional email address persisted for avatar lookup.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::data::paths::app_preferences_dir;

#[derive(Serialize, Deserialize, Default)]
struct ProfileConfigFile {
    email: Option<String>,
}

/// Reads and writes `{app_preferences_dir}/profile.toml`.
///
/// Contains user-preference data (email for Gravatar) that is not a credential
/// and therefore belongs in the preferences directory rather than the keyring.
pub struct ProfileConfig {
    email: Option<String>,
}

impl ProfileConfig {
    fn path() -> PathBuf {
        app_preferences_dir().join("profile.toml")
    }

    /// Loads the profile from disk, returning defaults on any error.
    pub fn load() -> Self {
        let email =
            std::fs::read_to_string(Self::path()).ok()
                                                 .and_then(|s| {
                                                     toml::from_str::<ProfileConfigFile>(&s).ok()
                                                 })
                                                 .and_then(|f| f.email)
                                                 .filter(|s| !s.trim().is_empty());

        Self { email }
    }

    /// Returns the stored email address, if any.
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    /// Saves `email` to disk.
    ///
    /// Passing `None` clears the stored value. Write failures are logged as
    /// warnings.
    pub fn save(email: Option<&str>) {
        let path = Self::path();
        if let Some(parent) = path.parent()
           && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::warn!("profile: failed to create config dir: {e}");
            return;
        }
        let file =
            ProfileConfigFile { email: email.filter(|s| !s.trim().is_empty()).map(str::to_owned), };
        match toml::to_string_pretty(&file) {
            Ok(contents) => {
                if let Err(e) = std::fs::write(&path, contents) {
                    tracing::warn!("profile: failed to write {}: {e}", path.display());
                }
            }
            Err(e) => tracing::warn!("profile: serialization failed: {e}"),
        }
    }
}
