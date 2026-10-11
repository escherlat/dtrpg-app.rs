//! Title bar view: window title, drag region, and account button.
//!
//! Sits above the sidebar/content split, separated from it by a horizontal
//! rule. Hosts the account button and its menu (user info, settings,
//! sign out), relocated here from the toolbar per
//! `shared-main-window-structure`.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Entity, Image, ImageFormat, ImageSource, MouseButton, ObjectFit,
    ParentElement, Styled, div, img, px,
};
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use rust_i18n::t;

use crate::controllers::settings::{AuthStateSnapshot, SettingsController};
use crate::data::theme::ColorTokens;
use crate::ui::actions::{ReportFeatureBug, ShowSettings};

/// Renders the title bar: app title on the left, a drag region, and the
/// account button on the right, separated from the content below by a
/// horizontal rule.
pub fn render_title_bar(auth: &AuthStateSnapshot, settings: Entity<SettingsController>,
                        colors: &ColorTokens, cx: &App)
                        -> impl IntoElement + 'static + use<> {
    let surface = colors.surface;
    let border = colors.border;
    let text_primary = colors.text_primary;

    // Reserve clearance for the macOS traffic light buttons, which overlay the
    // top-left of the window when the titlebar renders with
    // `appears_transparent: true`. The wordmark sits immediately after this
    // clearance — it is the app's only wordmark; `sidebar_view::build_header`
    // no longer repeats it.
    let leading_inset = if cfg!(target_os = "macos") {
        px(78.0)
    }
    else {
        px(12.0)
    };

    div().h(px(44.0))
         .flex_none()
         .flex()
         .items_center()
         .gap(px(8.0))
         .pl(leading_inset)
         .pr(px(12.0))
         .border_b_1()
         .border_color(border)
         .bg(surface)
         .child(div().text_color(text_primary)
                     .font_weight(gpui::FontWeight::SEMIBOLD)
                     .child(t!("sidebar.app_name").to_string()))
         .child(div().id("title-bar-drag-region")
                     .flex_1()
                     .h_full()
                     .on_mouse_down(MouseButton::Left, |_, window, _| {
                         window.start_window_move();
                     }))
         .child(render_account_button(auth, settings, colors, cx))
}

fn detect_image_format(bytes: &[u8]) -> ImageFormat {
    if bytes.starts_with(b"\x89PNG") {
        ImageFormat::Png
    }
    else {
        ImageFormat::Jpeg
    }
}

/// Renders the account button; its dropdown menu exposes user info, a
/// settings action, and a sign-out action when authenticated, or a sign-in
/// action when not.
fn render_account_button(auth: &AuthStateSnapshot, settings: Entity<SettingsController>,
                         colors: &ColorTokens, cx: &App)
                         -> AnyElement {
    if !auth.is_logged_in {
        let surface_alt = colors.surface_alt;
        let border_strong = colors.border_strong;
        let text_tertiary = colors.text_tertiary;
        let unauthenticated_variant =
            ButtonCustomVariant::new(cx).color(surface_alt)
                                        .foreground(text_tertiary)
                                        .hover(gpui::Hsla { l: (surface_alt.l * 0.9).min(1.0),
                                                            ..surface_alt })
                                        .active(gpui::Hsla { l: (surface_alt.l * 0.8).min(1.0),
                                                             ..surface_alt });
        let inner = div().flex()
                         .items_center()
                         .justify_center()
                         .size_full()
                         .text_xs()
                         .text_color(text_tertiary)
                         .child("👤")
                         .into_any_element();
        return Button::new("title-bar-account-btn")
            .custom(unauthenticated_variant)
            .tooltip(t!("toolbar.tooltip_not_signed_in").to_string())
            .rounded_full()
            .w(px(28.0))
            .h(px(28.0))
            .border_1()
            .border_color(border_strong)
            .child(inner)
            .dropdown_menu(move |menu, _, _| {
                let s = settings.clone();
                menu.item(
                    PopupMenuItem::new(t!("toolbar.sign_in")).on_click(move |_, _, cx| {
                        s.update(cx, |ctrl, cx| ctrl.open(cx));
                    }),
                )
            })
            .into_any_element();
    }

    let initial_text = auth.display_initial
                           .map(|c| c.to_string())
                           .unwrap_or_else(|| "D".to_string());

    let accent = colors.accent;
    let avatar_variant =
        ButtonCustomVariant::new(cx).color(accent)
                                    .foreground(gpui::white())
                                    .hover(gpui::Hsla { l: (accent.l * 0.85).min(1.0),
                                                        ..accent })
                                    .active(gpui::Hsla { l: (accent.l * 0.75).min(1.0),
                                                         ..accent });

    let inner: AnyElement = if let Some(bytes) = &auth.avatar_bytes {
        let format = detect_image_format(bytes);
        let image = Arc::new(Image::from_bytes(format, bytes.as_ref().clone()));
        img(ImageSource::Image(image)).w(px(28.0))
                                      .h(px(28.0))
                                      .rounded_full()
                                      .object_fit(ObjectFit::Cover)
                                      .into_any_element()
    }
    else {
        div().flex()
             .items_center()
             .justify_center()
             .size_full()
             .text_xs()
             .text_color(gpui::white())
             .child(initial_text)
             .into_any_element()
    };

    let menu_email = auth.email
                         .clone()
                         .or_else(|| auth.api_key_hint.clone())
                         .unwrap_or_else(|| t!("settings.default_account_name").to_string());

    Button::new("title-bar-account-btn").custom(avatar_variant)
                                        .tooltip(t!("toolbar.tooltip_account").to_string())
                                        .rounded_full()
                                        .w(px(28.0))
                                        .h(px(28.0))
                                        .child(inner)
                                        .dropdown_menu(move |menu, _, _| {
                                            let s_logout = settings.clone();
                                            menu.item(PopupMenuItem::label(menu_email.clone()))
                .item(PopupMenuItem::separator())
                .item(PopupMenuItem::new(t!("toolbar.tooltip_settings")).on_click(
                    move |_, window, cx| {
                        // Dispatch the same `ShowSettings` action the `cmd-,`
                        // keybinding and native menu item use, rather than
                        // toggling `SettingsController::is_open` — settings
                        // renders in its own OS window (see
                        // `open_settings_window`/`RootView::show_settings`),
                        // not an in-app panel gated by that flag, so toggling
                        // it alone left this menu item doing nothing.
                        window.dispatch_action(Box::new(ShowSettings), cx);
                    },
                ))
                .item(
                    PopupMenuItem::new(t!("title_bar.report")).on_click(move |_, window, cx| {
                        // Same action the Help menu item dispatches, so both
                        // entry points open the identical dialog (and share
                        // its no-stacking guard) — see
                        // `shared-report-feature-bug`.
                        window.dispatch_action(Box::new(ReportFeatureBug), cx);
                    }),
                )
                .item(PopupMenuItem::separator())
                .item(
                    PopupMenuItem::new(t!("title_bar.sign_out")).on_click(move |_, _, cx| {
                        s_logout.update(cx, |ctrl, cx| ctrl.logout(cx));
                    }),
                )
                                        })
                                        .into_any_element()
}
