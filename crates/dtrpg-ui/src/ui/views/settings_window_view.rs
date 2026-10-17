//! Root view for the settings window: a separate, non-modal window (see
//! `implement-rust-settings-window-gpui`) rather than an overlay inside the
//! main library window.

use gpui::{
    Context, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render, Styled, Window,
    div,
};
use gpui_kit::component::input::InputState;

use crate::controllers::library::LibraryController;
use crate::controllers::settings::SettingsController;
use crate::data::theme::LibriTheme;
use crate::ui::views::settings_appearance_view::AppearanceFontSelects;
use crate::ui::views::settings_view::render_settings_panel;

/// Root view for the settings window.
///
/// Shares the same [`SettingsController`] entity as
/// [`super::root_view::LibraryRootView`] so drafts, active tab, and other
/// settings state persist across close/reopen within the app session — the
/// entity isn't recreated per window open. Also shares the same
/// [`LibraryController`] entity so the Appearance page's font/theme setters
/// mutate the same controller the main window reads from.
pub struct SettingsWindowView {
    settings:                    Entity<SettingsController>,
    library:                     Entity<LibraryController>,
    /// Draft input for the in-progress "add file opener" row. Owned by the
    /// main window's `LibraryRootView` (not the settings controller, unlike
    /// the email/password/storage-path inputs), so it's passed in explicitly.
    file_opener_extension_input: Entity<InputState>,
    focus_handle:                FocusHandle,
}

impl SettingsWindowView {
    /// Constructs the settings window's root view and focuses it immediately
    /// so Escape closes the window without an extra click first.
    pub fn new(window: &mut Window, cx: &mut Context<Self>,
               settings: Entity<SettingsController>, library: Entity<LibraryController>,
               file_opener_extension_input: Entity<InputState>)
               -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self { settings,
               library,
               file_opener_extension_input,
               focus_handle }
    }
}

impl Focusable for SettingsWindowView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SettingsWindowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<LibriTheme>().clone();
        let colors = &theme.colors;
        // Adjusting the shared rem size scales every `rems(...)`-based size
        // utility (`.text_sm()`, `.text_xs()`, etc.) in this window, like
        // zooming a page — see `LibraryController::set_ui_text_size`.
        window.set_rem_size(theme.fonts.ui_text_size);
        let snap = self.settings.read(cx).snapshot();

        // One-shot focus request from the "Login to DriveThruRPG" banner
        // action (`LibraryRootView::show_settings_focused_on_email`) — must
        // run here rather than at the click site, since that site only has
        // the main window's `Window`, not this settings window's.
        let focus_email_pending = self.settings
                                      .update(cx, |ctrl, _cx| ctrl.take_focus_email_pending());
        if focus_email_pending
           && !snap.auth.is_logged_in
           && let Some(email_state) = snap.email_input.clone()
        {
            email_state.update(cx, |input, cx| input.focus(window, cx));
        }

        let sign_in_enabled = !snap.sign_in_in_progress
                              && !snap.email_draft.trim().is_empty()
                              && !snap.password_draft.trim().is_empty();
        let font_selects = AppearanceFontSelects { body:  snap.body_font_select,
                                                   value: snap.value_font_select,
                                                   label: snap.label_font_select,
                                                   mono:  snap.mono_font_select, };

        let panel = render_settings_panel(&snap.file_openers,
                                          snap.auth,
                                          snap.storage_root_path,
                                          snap.storage_path_exists,
                                          snap.storage_roots,
                                          snap.storage_rules,
                                          self.settings.clone(),
                                          self.library.clone(),
                                          &self.focus_handle,
                                          colors,
                                          &theme,
                                          snap.email_input,
                                          snap.password_input,
                                          snap.sign_in_in_progress,
                                          sign_in_enabled,
                                          snap.sign_in_error,
                                          snap.storage_path_input,
                                          self.file_opener_extension_input.clone(),
                                          snap.pending_file_opener,
                                          snap.active_page_ix,
                                          snap.cache_counts,
                                          snap.max_concurrent_downloads,
                                          snap.max_concurrent_downloads_input,
                                          snap.create_collections,
                                          snap.recently_updated_window_days,
                                          snap.recently_updated_window_input,
                                          font_selects);

        div().size_full().child(panel)
    }
}
