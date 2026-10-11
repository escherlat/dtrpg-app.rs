//! Notification banner view: renders persistent auth-related notices below the
//! toolbar.

use gpui::{AnyElement, Entity, IntoElement, ParentElement, Styled, div, px};
use gpui_component::alert::Alert;
use gpui_component::button::{Button, ButtonVariants};
use rust_i18n::t;

use crate::controllers::auth_state::AuthStateController;
use crate::data::notification::{Notice, NoticeKind};
use crate::data::theme::ColorTokens;
use crate::ui::views::root_view::LibraryRootView;

/// Renders the notification banner column.
///
/// Returns an invisible empty element when `notices` is empty so the caller
/// does not need to conditionalize the insertion point.
pub fn render_notification_banner(notices: Vec<Notice>,
                                  auth_entity: Entity<AuthStateController>,
                                  root_entity: Entity<LibraryRootView>, colors: &ColorTokens)
                                  -> AnyElement {
    if notices.is_empty() {
        return div().into_any_element();
    }

    let border = colors.border;

    div().flex_none()
         .flex()
         .flex_col()
         .border_b_1()
         .border_color(border)
         .children(notices.into_iter().map(|notice| {
             let (message, action_label) = notice_strings(&notice);
             let kind = notice.kind;

             let auth_entity_action = auth_entity.clone();
             let auth_entity_dismiss = auth_entity.clone();
             let root_entity = root_entity.clone();

             // Border and background live on this outer container, not the
             // inner `Alert`, so the emphasis encloses both the
             // message and the action button as one banner rather
             // than boxing the message alone. `Alert`'s own
             // banner-mode border/background use a warning color
             // mixed with white, which reads as barely-there; its own border is
             // muted to `warning_bg` here so it doesn't compete with the outer
             // one.
             div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(8.0))
                .py(px(4.0))
                .bg(colors.warning_bg)
                .border_2()
                .border_color(colors.warning_text)
                .child(
                    Alert::warning(format!("notice-alert-{kind:?}"), message.to_string())
                        .banner()
                        .flex_1()
                        .border_color(colors.warning_bg)
                        .on_close(move |_, _, cx| {
                            auth_entity_dismiss.update(cx, |ctrl, cx| {
                                ctrl.dismiss_notice(kind, cx);
                            });
                        }),
                )
                .child(
                    Button::new(format!("notice-action-{kind:?}"))
                        .ghost()
                        .label(action_label)
                        .mr(px(8.0))
                        .on_click(move |_, _, cx| {
                            // `SettingsController::open` alone only flips an internal
                            // flag — the window itself is created/tracked by
                            // `LibraryRootView::show_settings`, so that's what actually
                            // needs calling for a click here to do anything visible.
                            // The `_focused_on_email` variant also switches to the
                            // Account tab and focuses the email input, since the whole
                            // point of this action is to get the user signing in as
                            // fast as possible.
                            root_entity.update(cx, |view, cx| {
                                view.show_settings_focused_on_email(cx);
                            });
                            auth_entity_action.update(cx, |ctrl, cx| {
                                ctrl.dismiss_notice(kind, cx);
                            });
                        }),
                )
         }))
         .into_any_element()
}

fn notice_strings(notice: &Notice)
                  -> (std::borrow::Cow<'static, str>, std::borrow::Cow<'static, str>) {
    match notice.kind {
        NoticeKind::NotSignedIn => {
            (t!("settings.not_signed_in"), t!("notification.set_up_account"))
        }
        NoticeKind::SessionExpired => {
            (t!("notification.session_expired"), t!("notification.sign_in_again"))
        }
    }
}
