//! The Account tab's sign-in form: email/password drafts, startup
//! re-authentication, and interactive sign-in.

use super::*;

impl SettingsController {
    /// Returns the current email draft value.
    pub fn email_draft(&self) -> &str {
        &self.email_draft
    }

    /// Updates the email draft field and clears any prior sign-in error.
    pub fn set_email_draft(&mut self, value: String, cx: &mut Context<Self>) {
        self.email_draft = value;
        self.sign_in_error = None;
        cx.emit(SettingsChanged);
    }

    /// Updates the password draft field and clears any prior sign-in error.
    pub fn set_password_draft(&mut self, value: String, cx: &mut Context<Self>) {
        self.password_draft = value;
        self.sign_in_error = None;
        cx.emit(SettingsChanged);
    }

    /// Authenticates silently at startup using a stored API key.
    ///
    /// Emits [`StartupAuthBegun`] immediately, then runs authentication on a
    /// background thread. On success, emits [`SignInSucceeded`] so the root
    /// view can replace the library service. On failure, emits
    /// [`StartupAuthFailed`] and logs a warning.
    pub fn startup_auth(&mut self, api_key: String, cx: &mut Context<Self>) {
        let email = {
            let trimmed = self.email_draft.trim();
            if trimmed.is_empty() {
                None
            }
            else {
                Some(trimmed.to_owned())
            }
        };
        let svc = self.login_service.clone();

        cx.emit(StartupAuthBegun);

        cx.spawn(async move |this, async_cx| {
              let api_key_hint = api_key.clone();
              let result = async_cx.background_executor()
                                   .spawn(async move { svc.authenticate(&api_key) })
                                   .await;

              this.update(async_cx, |ctrl, cx| match result {
                      Ok(tokens) => {
                          ctrl.set_logged_in(email, Some(api_key_hint.as_str()), cx);
                          cx.emit(SignInSucceeded(tokens));
                      }
                      Err(e) => {
                          tracing::warn!("startup re-authentication failed: {e}");
                          cx.emit(StartupAuthFailed);
                      }
                  })
                  .ok();
          })
          .detach();
    }

    /// Signs in using the current `email_draft` and `password_draft`.
    ///
    /// First exchanges the email/password pair for an application key via
    /// the SDK credential exchange, then exchanges that key for session tokens
    /// via the existing authentication call. On success, stores the credential
    /// to the keyring and emits [`SignInSucceeded`]. On failure at either step,
    /// sets `sign_in_error` with a message identifying which step failed.
    pub fn sign_in(&mut self, cx: &mut Context<Self>) {
        let email = self.email_draft.trim().to_owned();
        let password = self.password_draft.trim().to_owned();
        if self.sign_in_in_progress || email.is_empty() || password.is_empty() {
            return;
        }
        self.sign_in_in_progress = true;
        self.sign_in_error = None;
        cx.emit(SettingsChanged);

        let svc = self.login_service.clone();

        cx.spawn(async move |this, async_cx| {
              // Step 1: exchange email + password for application key.
              let credential_result = async_cx.background_executor()
                                              .spawn({
                                                  let email = email.clone();
                                                  let password = password.clone();
                                                  async move {
                                                      svc.login_with_credentials(&email, &password)
                                                         .map(|key| (svc, key))
                                                  }
                                              })
                                              .await;

              let (svc, api_key) = match credential_result {
                  Ok(pair) => pair,
                  Err(e) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.sign_in_in_progress = false;
                              ctrl.sign_in_error = Some(e.0);
                              cx.emit(SettingsChanged);
                          })
                          .ok();
                      return;
                  }
              };

              // Step 2: exchange application key for session tokens.
              let auth_result = async_cx.background_executor()
                                        .spawn({
                                            let key = api_key.clone();
                                            async move { svc.authenticate(&key).map(|t| (key, t)) }
                                        })
                                        .await;

              this.update(async_cx, |ctrl, cx| {
                      ctrl.sign_in_in_progress = false;
                      match auth_result {
                          Ok((key, tokens)) => {
                              let store =
                                  KeyringCredentialStore::new(KEYRING_SERVICE, KEYRING_API_KEY);
                              if let Err(e) =
                                  store.store(&Credential { service: KEYRING_SERVICE.into(),
                                                            account: KEYRING_API_KEY.into(),
                                                            secret:  key.clone(),
                                                            email:   Some(email.clone()), })
                              {
                                  tracing::warn!("failed to save credential to keyring: {e}");
                              }
                              ProfileConfig::save(Some(&email));
                              ctrl.password_draft.clear();
                              ctrl.set_logged_in(Some(email), Some(key.as_str()), cx);
                              cx.emit(SignInSucceeded(tokens));
                          }
                          Err(e) => {
                              ctrl.sign_in_error =
                                  Some(format!("{}: {}", t!("activity.session_setup_failed"), e.0));
                              cx.emit(SettingsChanged);
                          }
                      }
                  })
                  .ok();
          })
          .detach();
    }
}
