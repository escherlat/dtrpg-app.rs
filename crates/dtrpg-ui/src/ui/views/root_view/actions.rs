//! Window-opening and dialog actions dispatched from the root view's menu
//! bindings and app-level action fallbacks.

use super::*;

impl LibraryRootView {
    /// The library controller, for app-level menu fallbacks that act on the
    /// catalog without going through this view.
    pub(crate) fn controller(&self) -> Entity<LibraryController> {
        self.controller.clone()
    }

    /// Opens the Settings window, or brings an already-open one to front.
    ///
    /// Extracted so both the `ShowSettings` action bound on this view's root
    /// element and the app-level fallback in `ui::app::setup` (which fires
    /// when no element in the window's dispatch path currently owns focus)
    /// share one implementation.
    pub(crate) fn show_settings(&mut self, cx: &mut Context<Self>) {
        let already_open = self.settings_window
                               .map(|handle| {
                                   handle.update(cx, |_, window, _| window.activate_window())
                                         .is_ok()
                               })
                               .unwrap_or(false);
        if !already_open {
            self.settings.update(cx, |ctrl, cx| ctrl.open(cx));
            let handle = open_settings_window(self.settings.clone(),
                                              self.controller.clone(),
                                              self.file_opener_extension_input.clone(),
                                              cx);
            self.settings_window = Some(handle);
        }
    }

    /// Opens the "Report a Feature/Bug" window, or brings an already-open
    /// one to front.
    pub(crate) fn show_report_feature_bug(&mut self, cx: &mut Context<Self>) {
        let already_open = self.report_feature_bug_window
                               .map(|handle| {
                                   handle.update(cx, |_, window, _| window.activate_window())
                                         .is_ok()
                               })
                               .unwrap_or(false);
        if !already_open {
            let handle = open_report_feature_bug_window(cx);
            self.report_feature_bug_window = Some(handle);
        }
    }

    /// Opens the About window, or brings an already-open one to front.
    pub(crate) fn show_about(&mut self, cx: &mut Context<Self>) {
        let already_open = self.about_window
                               .map(|handle| {
                                   handle.update(cx, |_, window, _| window.activate_window())
                                         .is_ok()
                               })
                               .unwrap_or(false);
        if !already_open {
            let handle = open_about_window(cx);
            self.about_window = Some(handle);
        }
    }

    /// Opens Settings on the Account tab with the email input focused.
    ///
    /// Used by the "Login to DriveThruRPG" notification banner action — see
    /// `SettingsController::request_email_focus_on_account_tab`.
    pub(crate) fn show_settings_focused_on_email(&mut self, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |ctrl, cx| ctrl.request_email_focus_on_account_tab(cx));
        self.show_settings(cx);
    }

    /// Toggles the Activity panel. See [`Self::show_settings`] for why this
    /// is a method rather than inline closure logic.
    pub(crate) fn show_activity(&mut self, cx: &mut Context<Self>) {
        self.activity.update(cx, |a, cx| a.toggle_panel(cx));
    }

    /// Toggles the Alert History panel. See [`Self::show_settings`] for why
    /// this is a method rather than inline closure logic.
    pub(crate) fn show_alert_history(&mut self, cx: &mut Context<Self>) {
        self.activity.update(cx, |a, cx| a.toggle_alert_panel(cx));
    }

    /// Opens the "Add Collection" dialog. See [`Self::show_settings`] for why
    /// this is a method rather than inline closure logic.
    pub(crate) fn show_add_collection_dialog(&mut self, window: &mut gpui::Window,
                                             cx: &mut Context<Self>) {
        let ctrl = self.controller.clone();
        let input = self.collection_name_input.clone();
        window.open_dialog(cx, move |dialog, _window, _cx| {
                  let ctrl = ctrl.clone();
                  let input = input.clone();
                  dialog
                .close_button(false)
                .overlay_closable(true)
                .w(px(320.))
                // Visible Cancel/Create buttons are rendered via `.footer(...)`
                // below (wrapped in `DialogClose`/`DialogAction`, which dispatch
                // the same `CancelDialog`/`ConfirmDialog` actions Escape/Enter
                // use), so the callbacks registered here run regardless of
                // whether the user clicks a button or uses the keyboard.
                .on_ok({
                    let input = input.clone();
                    let ctrl = ctrl.clone();
                    move |_, _, cx| {
                        let name = input.read(cx).value().trim().to_string();
                        if name.is_empty() {
                            return false;
                        }
                        ctrl.update(cx, |c, cx| c.create_collection(name, cx));
                        true
                    }
                })
                .on_cancel(|_, _, _| true)
                .content({
                    let input = input.clone();
                    move |content, _, _| {
                        content
                            .child(DialogHeader::new().px_4().pt_4().child(
                                DialogTitle::new().child(t!("collections.add_dialog_title")),
                            ))
                            .child(div().px_4().py_2().child(Input::new(&input)))
                    }
                })
                .footer(
                    DialogFooter::new()
                        .px_4()
                        .pb_4()
                        .child(
                            DialogClose::new().child(
                                Button::new("cancel-collection")
                                    .label(t!("collections.add_dialog_cancel")),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("confirm-collection")
                                    .primary()
                                    .label(t!("collections.add_dialog_confirm")),
                            ),
                        ),
                )
              });
    }
}
