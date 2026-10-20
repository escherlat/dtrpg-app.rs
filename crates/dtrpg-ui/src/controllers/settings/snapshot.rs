//! Builds the per-render-pass [`SettingsSnapshot`] from controller state.

use super::*;

impl SettingsController {
    /// Returns all data needed by the views for one render pass.
    pub fn snapshot(&self) -> SettingsSnapshot {
        let auth = match &self.auth_state {
            AuthState::LoggedOut => AuthStateSnapshot { is_logged_in:    false,
                                                        email:           None,
                                                        display_initial: None,
                                                        avatar_bytes:    None,
                                                        api_key_hint:    None,
                                                        token_status:    None, },
            AuthState::LoggedIn { email,
                                  avatar_bytes, } => {
                AuthStateSnapshot { is_logged_in:    true,
                                    display_initial: email.as_deref()
                                                          .and_then(|s| s.trim().chars().next())
                                                          .map(|c| c.to_ascii_uppercase()),
                                    email:           email.clone(),
                                    avatar_bytes:    avatar_bytes.clone(),
                                    api_key_hint:    self.api_key_hint.clone(),
                                    token_status:    self.token_status.get(), }
            }
        };

        SettingsSnapshot { is_open: self.is_open,
                           file_openers: self.file_openers.entries().to_vec(),
                           is_authenticated: self.is_authenticated,
                           storage_root_path: self.storage.root_path(),
                           storage_roots: self.storage.roots(),
                           storage_rules: self.storage.rules().to_vec(),
                           publisher_rule_input: self.publisher_rule_input.clone(),
                           collection_rule_input: self.collection_rule_input.clone(),
                           size_rule_input: self.size_rule_input.clone(),
                           rule_target_root_id: self.rule_target_root_id,
                           rule_editor_type: self.rule_editor_type,
                           storage_unavailable: self.storage_unavailable,
                           storage_path_exists: self.storage_path_exists,
                           auth,
                           email_draft: self.email_draft.clone(),
                           password_draft: self.password_draft.clone(),
                           sign_in_in_progress: self.sign_in_in_progress,
                           sign_in_error: self.sign_in_error.clone(),
                           email_input: self.email_input.clone(),
                           password_input: self.password_input.clone(),
                           storage_path_draft: self.storage_path_draft.clone(),
                           storage_path_input: self.storage_path_input.clone(),
                           pending_file_opener: self.pending_file_opener.clone(),
                           active_page_ix: self.active_page_ix,
                           cache_counts: self.cache_counts,
                           max_concurrent_downloads: self.storage.max_concurrent_downloads(),
                           max_concurrent_downloads_input: self.max_concurrent_downloads_input
                                                               .clone(),
                           create_collections: self.storage.create_collections(),
                           native: self.native.snapshot(),
                           recently_updated_window_days: self.storage
                                                             .recently_updated_window_days(),
                           recently_updated_window_input: self.recently_updated_window_input
                                                              .clone(),
                           body_font_select: self.body_font_select.clone(),
                           value_font_select: self.value_font_select.clone(),
                           label_font_select: self.label_font_select.clone(),
                           mono_font_select: self.mono_font_select.clone() }
    }
}
