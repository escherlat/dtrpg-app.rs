//! Auth state: signing in, the cached avatar, and logout.

use super::*;

impl SettingsController {
    /// Marks the user as signed in.
    ///
    /// When `email` is `Some`, spawns a background task to fetch the Gravatar
    /// avatar. Emits [`SettingsChanged`] immediately and again once avatar
    /// bytes arrive (if applicable).
    pub fn set_logged_in(&mut self, email: Option<String>, api_key: Option<&str>,
                         cx: &mut Context<Self>) {
        self.is_authenticated = true;
        self.auth_state = AuthState::LoggedIn { email:        email.clone(),
                                                avatar_bytes: None, };
        self.api_key_hint = api_key.map(mask_api_key);
        cx.emit(SettingsChanged);

        if let Some(addr) = email {
            cx.spawn(async move |this, async_cx| {
                  let bytes = async_cx.background_executor()
                                      .spawn(async move { fetch_avatar_bytes(addr) })
                                      .await;
                  this.update(async_cx, |ctrl, cx| ctrl.set_avatar_bytes(bytes, cx))
                      .ok();
              })
              .detach();
        }
    }

    /// Stores fetched avatar bytes and re-renders.
    ///
    /// No-op if the user is not currently signed in.
    pub fn set_avatar_bytes(&mut self, bytes: Option<Vec<u8>>, cx: &mut Context<Self>) {
        if let AuthState::LoggedIn { avatar_bytes, .. } = &mut self.auth_state {
            *avatar_bytes = bytes.map(Arc::new);
            cx.emit(SettingsChanged);
        }
    }

    /// Clears the auth state and triggers the full logout flow.
    ///
    /// Emits [`SettingsChanged`] to update the UI, then [`LogoutRequested`] to
    /// prompt the root view to delete credentials and open the login window.
    pub fn logout(&mut self, cx: &mut Context<Self>) {
        self.is_authenticated = false;
        self.auth_state = AuthState::LoggedOut;
        self.api_key_hint = None;
        self.token_status.clear();
        cx.emit(SettingsChanged);
        cx.emit(LogoutRequested);
    }
}

/// Returns a masked representation of an API key: first 4 chars + bullets +
/// last 1 char. For keys 5 characters or shorter, returns only bullets.
fn mask_api_key(key: &str) -> String {
    if key.len() > 5 {
        format!("{}••••••••{}", &key[..4], &key[key.len() - 1..])
    }
    else {
        "••••••••".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_api_key_long_key() {
        assert_eq!(mask_api_key("abcdefghij1"), "abcd••••••••1");
    }

    #[test]
    fn mask_api_key_short_key() {
        assert_eq!(mask_api_key("abc"), "••••••••");
    }

    #[test]
    fn mask_api_key_exactly_five() {
        assert_eq!(mask_api_key("abcde"), "••••••••");
    }
}
