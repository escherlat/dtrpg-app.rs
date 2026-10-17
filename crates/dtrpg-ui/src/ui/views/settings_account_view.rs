//! Account settings section: identity display, log-out, and email+password
//! sign-in form.

use std::sync::Arc;

use gpui::{
    AnyElement, Entity, Image, ImageFormat, ImageSource, InteractiveElement, IntoElement,
    ParentElement, StatefulInteractiveElement, Styled, div, px,
};
use gpui_kit::component::Sizable;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::description_list::{DescriptionItem, DescriptionList};
use gpui_kit::component::input::{Input, InputState};
use rust_i18n::t;

use crate::controllers::settings::{AuthStateSnapshot, SettingsController};
use crate::data::theme::ColorTokens;
use crate::ui::widgets::{selectable_text, small_caps_text};
use crate::util::datetime::format_absolute;

/// Width of the Email/API Key/token info rows' shared label column, sized to
/// fit the longest label ("Acquired", "API Key", "Expires") at `text_sm` plus a
/// small margin.
const ACCOUNT_INFO_LABEL_WIDTH: gpui::Pixels = px(80.0);

/// Renders the Account settings section.
#[allow(clippy::too_many_arguments)]
pub fn render_account_section(
    auth: &AuthStateSnapshot, entity: Entity<SettingsController>, colors: &ColorTokens,
    email_input: Option<Entity<InputState>>, password_input: Option<Entity<InputState>>,
    sign_in_in_progress: bool, sign_in_enabled: bool, sign_in_error: Option<String>,
    label_font_family: &str, mono_font_family: &str,
) -> AnyElement {
    if auth.is_logged_in {
        render_authenticated(auth, entity, colors, label_font_family, mono_font_family)
            .into_any_element()
    } else {
        render_unauthenticated(
            entity,
            colors,
            email_input,
            password_input,
            sign_in_in_progress,
            sign_in_enabled,
            sign_in_error,
        )
        .into_any_element()
    }
}

// ── Authenticated state
// ───────────────────────────────────────────────────────

fn render_authenticated(
    auth: &AuthStateSnapshot, entity: Entity<SettingsController>, colors: &ColorTokens,
    label_font_family: &str, mono_font_family: &str,
) -> impl IntoElement + 'static {
    let text_primary = colors.text_primary;
    let border = colors.border;

    let avatar = render_avatar_circle(auth, colors);
    let email_text = auth
        .email
        .clone()
        .or_else(|| auth.api_key_hint.clone())
        .unwrap_or_else(|| t!("settings.default_account_name").to_string());

    let mut info_list = DescriptionList::horizontal()
        .bordered(false)
        .columns(1)
        .label_width(ACCOUNT_INFO_LABEL_WIDTH)
        .child(account_info_item(
            "settings-account-email",
            t!("settings.email_label").to_string(),
            email_text.clone(),
            colors,
            label_font_family,
            mono_font_family,
        ));

    if let Some(hint) = &auth.api_key_hint {
        info_list = info_list.child(account_info_item(
            "settings-account-api-key",
            t!("settings.api_key_label").to_string(),
            hint.clone(),
            colors,
            label_font_family,
            mono_font_family,
        ));
    }

    if let Some(status) = auth.token_status {
        info_list = info_list.child(account_info_item(
            "settings-account-token-acquired",
            t!("settings.token_acquired_label").to_string(),
            format_absolute(status.acquired_at),
            colors,
            label_font_family,
            mono_font_family,
        ));
        if let Some(expires_at) = status.expires_at {
            info_list = info_list.child(account_info_item(
                "settings-account-token-expires",
                t!("settings.token_expires_label").to_string(),
                format_absolute(expires_at),
                colors,
                label_font_family,
                mono_font_family,
            ));
        }
    }

    div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .p(px(24.0))
        // ── Identity row ──────────────────────────────────────────────────
        .child(
            div().flex().items_center().child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .flex_1()
                    .min_w_0()
                    .child(avatar)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(text_primary)
                                    .child(t!("settings.account_title")),
                            )
                            .child(info_list),
                    ),
            ),
        )
        // ── Divider ───────────────────────────────────────────────────────
        .child(div().h(px(1.0)).bg(border))
        // ── Actions ───────────────────────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(render_logout_button(entity)),
        )
}

/// Gap between the label column and the value column in the Email/API Key
/// `DescriptionList` rows. The list's own column padding is zeroed out by
/// `bordered(false)`, so without this the label and value text sit flush
/// against each other.
const ACCOUNT_INFO_COLUMN_GAP: gpui::Pixels = px(8.0);

/// Builds a right-aligned label / left-aligned monospace-value row for the
/// Email/API Key `DescriptionList`. The label styling (font, weight, size,
/// color) matches every other settings section's row labels — see
/// `settings_appearance_view::row_label` — the value keeps its prior
/// monospace treatment.
fn account_info_item(
    value_id: &'static str, label: String, value: String, colors: &ColorTokens,
    label_font_family: &str, mono_font_family: &str,
) -> DescriptionItem {
    DescriptionItem::new(
        div()
            .w_full()
            .text_right()
            .text_sm()
            .font_family(label_font_family.to_string())
            .text_color(colors.text_primary)
            .child(small_caps_text(label))
            .into_any_element(),
    )
    .value(
        div()
            .pl(ACCOUNT_INFO_COLUMN_GAP)
            .child(
                selectable_text(value_id, value)
                    .text_xs()
                    .font_family(mono_font_family.to_string())
                    .text_color(colors.text_tertiary),
            )
            .into_any_element(),
    )
}

/// Renders a 56×56 avatar circle: Gravatar image if available, initial letter
/// otherwise.
fn render_avatar_circle(auth: &AuthStateSnapshot, _colors: &ColorTokens) -> AnyElement {
    const AVATAR_SIZE: gpui::Pixels = px(56.);

    if let Some(bytes) = &auth.avatar_bytes {
        let format = if bytes.starts_with(b"\x89PNG") {
            ImageFormat::Png
        } else {
            ImageFormat::Jpeg
        };
        let image = Arc::new(Image::from_bytes(format, bytes.as_ref().clone()));
        // flex_none() keeps this from being resized by an ancestor flex
        // layout's basis/shrink computation - without it, Avatar's own
        // explicit pixel sizing can still get squished in a tight flex
        // context (observed in the title-bar account button).
        return div()
            .flex_none()
            .child(
                Avatar::new()
                    .with_size(gpui_kit::component::Size::Size(AVATAR_SIZE))
                    .rounded_full()
                    .src(ImageSource::Image(image)),
            )
            .into_any_element();
    }

    let name = auth
        .email
        .clone()
        .or_else(|| auth.display_initial.map(|c| c.to_string()))
        .unwrap_or_else(|| "D".to_string());

    div()
        .flex_none()
        .child(
            Avatar::new()
                .with_size(gpui_kit::component::Size::Size(AVATAR_SIZE))
                .rounded_full()
                .name(name),
        )
        .into_any_element()
}

// ── Unauthenticated state
// ─────────────────────────────────────────────────────

fn render_unauthenticated(
    entity: Entity<SettingsController>, colors: &ColorTokens,
    email_input: Option<Entity<InputState>>, password_input: Option<Entity<InputState>>,
    sign_in_in_progress: bool, sign_in_enabled: bool, sign_in_error: Option<String>,
) -> impl IntoElement + 'static {
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;
    let text_tertiary = colors.text_tertiary;
    let error_color = colors.error;
    let border = colors.border;
    let accent = colors.accent;
    let accent_on = colors.accent_on;
    let disabled_bg = colors.hover;

    let mut form = div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .p(px(24.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(text_primary)
                        .child(t!("settings.account_title")),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(text_secondary)
                        .child(t!("settings.not_signed_in")),
                ),
        )
        .child(div().h(px(1.0)).bg(border))
        .child(
            div().flex().flex_col().gap(px(12.0)).child(
                div()
                    .text_xs()
                    .text_color(text_tertiary)
                    .child(t!("settings.sign_in_prompt")),
            ),
        );

    if let (Some(email_state), Some(pw_state)) = (email_input, password_input) {
        let entity_for_btn = entity.clone();
        let can_click = sign_in_enabled && !sign_in_in_progress;
        let btn_bg = if can_click { accent } else { disabled_bg };
        let btn_text = if can_click { accent_on } else { text_secondary };
        let btn_label = if sign_in_in_progress {
            t!("settings.sign_in_in_progress")
        } else {
            t!("settings.sign_in_button")
        };

        let mut form_section = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                Input::new(&email_state)
                    .appearance(true)
                    .disabled(sign_in_in_progress)
                    .into_element(),
            )
            .child(
                Input::new(&pw_state)
                    .appearance(true)
                    .disabled(sign_in_in_progress)
                    .into_element(),
            );

        if let Some(err) = sign_in_error {
            form_section = form_section.child(
                selectable_text("settings-sign-in-error", err)
                    .text_xs()
                    .text_color(error_color),
            );
        }

        form_section = form_section.child(
            div()
                .id("sign-in-btn")
                .h(px(34.0))
                .px(px(16.0))
                .rounded(px(8.0))
                .bg(btn_bg)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .on_click(move |_event, _window, cx| {
                    if can_click {
                        entity_for_btn.update(cx, |ctrl, cx| ctrl.sign_in(cx));
                    }
                })
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(btn_text)
                        .child(btn_label),
                ),
        );

        form = form.child(form_section);
    }

    form
}

fn render_logout_button(entity: Entity<SettingsController>) -> impl IntoElement + 'static {
    Button::new("logout-btn")
        .danger()
        .label(t!("settings.log_out_button"))
        .on_click(move |_, _, cx| {
            entity.update(cx, |ctrl, cx| ctrl.logout(cx));
        })
}
