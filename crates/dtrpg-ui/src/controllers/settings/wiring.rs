//! Attaches UI widget state entities (text inputs, font pickers) created by
//! the root view onto the controller, so later renders can read them back.

use super::*;

impl SettingsController {
    /// Attaches the email input state entity created by the root view.
    pub fn set_email_input(&mut self, input: Entity<InputState>) {
        self.email_input = Some(input);
    }

    /// Attaches the password input state entity created by the root view.
    pub fn set_password_input(&mut self, input: Entity<InputState>) {
        self.password_input = Some(input);
    }

    /// Attaches the storage path input state entity created by the root view.
    pub fn set_storage_path_input(&mut self, input: Entity<InputState>) {
        self.storage_path_input = Some(input);
    }

    /// Attaches the "Recently Updated window" number input state entity
    /// created by the root view.
    pub fn set_recently_updated_window_input(&mut self, input: Entity<InputState>) {
        self.recently_updated_window_input = Some(input);
    }

    /// Attaches the "Max concurrent downloads" number input state entity
    /// created by the root view.
    pub fn set_max_concurrent_downloads_input(&mut self, input: Entity<InputState>) {
        self.max_concurrent_downloads_input = Some(input);
    }

    /// Attaches the publisher rule input state created by the root view.
    pub fn set_publisher_rule_input(&mut self, input: Entity<InputState>) {
        self.publisher_rule_input = Some(input);
    }

    /// Attaches the collection and size rule inputs created by the root view.
    pub fn set_collection_rule_input(&mut self, input: Entity<InputState>) {
        self.collection_rule_input = Some(input);
    }

    pub fn set_size_rule_input(&mut self, input: Entity<InputState>) {
        self.size_rule_input = Some(input);
    }

    /// Attaches the body-font picker state entity created by the root view.
    pub fn set_body_font_select(&mut self, state: FontSelectState) {
        self.body_font_select = Some(state);
    }

    /// Attaches the value-font picker state entity created by the root view.
    pub fn set_value_font_select(&mut self, state: FontSelectState) {
        self.value_font_select = Some(state);
    }

    /// Attaches the label-font picker state entity created by the root view.
    pub fn set_label_font_select(&mut self, state: FontSelectState) {
        self.label_font_select = Some(state);
    }

    /// Attaches the monospace-font picker state entity created by the root
    /// view.
    pub fn set_mono_font_select(&mut self, state: FontSelectState) {
        self.mono_font_select = Some(state);
    }
}
