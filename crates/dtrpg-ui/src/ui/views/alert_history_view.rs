//! Alert history panel overlay: lists the durable log of past error activity
//! items.
//!
//! Unlike the activity panel's `recent` list (which expires entries on a
//! timer), this panel reads
//! [`crate::controllers::activity::ActivityController::alert_snapshot`],
//! a capped but non-expiring log, so users can review failures from earlier in
//! the session after the activity panel has already dismissed them.

use std::time::{SystemTime, UNIX_EPOCH};

use dtrpg_core::data::activity::{AlertEntry, AlertHistorySnapshot};
use gpui::prelude::*;
use gpui::{AnyElement, Entity, IntoElement, ParentElement, SharedString, Styled, div, px};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tooltip::Tooltip;
use rust_i18n::t;

use crate::controllers::activity::ActivityController;
use crate::data::theme::ColorTokens;
use crate::ui::widgets::selectable_text;
use crate::util::datetime::{format_absolute, format_relative};

/// Renders the alert history panel's content. The caller (the status bar's
/// `Popover` wrapping the notification button) is responsible for anchoring and
/// positioning this content relative to the trigger button.
pub fn render_alert_history_panel(snap: &AlertHistorySnapshot,
                                  entity: Entity<ActivityController>, colors: &ColorTokens)
                                  -> AnyElement {
    let border = colors.border;
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;
    let text_tertiary = colors.text_tertiary;

    let close_entity = entity.clone();
    let clear_entity = entity.clone();
    let has_entries = !snap.entries.is_empty();

    // No bg/border/shadow/rounding here — the enclosing `Popover` content
    // wrapper already supplies that framing (see `popover_style` in
    // `gpui-kit`); adding our own on top produced a nested double frame.
    div().occlude()
         .w(px(340.0))
         .flex()
         .flex_col()
         // ── Header ────────────────────────────────────────────────────────
         .child(div().flex()
                     .items_center()
                     .justify_between()
                     .gap(px(8.0))
                     .px(px(14.0))
                     .py(px(8.0))
                     .border_b_1()
                     .border_color(border)
                     .child(div().text_sm()
                                 .font_weight(gpui::FontWeight::SEMIBOLD)
                                 .text_color(text_primary)
                                 .child(t!("alert_history.title")))
                     .child(div().flex()
                                 .items_center()
                                 .gap(px(10.0))
                                 .children(has_entries.then(|| {
                                                          div()
                                .id("alert-history-clear")
                                .text_xs()
                                .text_color(text_tertiary)
                                .cursor_pointer()
                                .child(t!("alert_history.clear"))
                                .on_click(move |_, _, cx| {
                                    clear_entity.update(cx, |a, cx| a.clear_alert_log(cx));
                                })
                                                      }))
                                 .child(div().id("alert-history-close")
                                             .text_xs()
                                             .text_color(text_tertiary)
                                             .cursor_pointer()
                                             .child("x")
                                             .on_click(move |_, _, cx| {
                                                 close_entity.update(cx, |a, cx| {
                                                                 a.toggle_alert_panel(cx)
                                                             });
                                             }))))
         // ── Entry list ────────────────────────────────────────────────────
         .child(if snap.entries.is_empty() {
                    render_empty(text_primary, text_tertiary)
                }
                else {
                    div().flex()
                         .flex_col()
                         .max_h(px(400.0))
                         .overflow_y_scrollbar()
                         .children(snap.entries.iter().map(|entry| {
                                                          render_entry_row(entry,
                                                                           text_secondary,
                                                                           text_tertiary,
                                                                           border)
                                                      }))
                         .into_any_element()
                })
         .into_any_element()
}

fn render_empty(text_primary: gpui::Hsla, text_tertiary: gpui::Hsla) -> AnyElement {
    div().flex()
         .flex_col()
         .items_center()
         .justify_center()
         .py(px(24.0))
         .gap(px(6.0))
         .child(div().text_2xl().text_color(text_tertiary).child("!"))
         .child(div().text_sm()
                     .text_color(text_primary)
                     .child(t!("alert_history.empty")))
         .child(div().text_xs()
                     .text_color(text_tertiary)
                     .child(t!("alert_history.empty_hint")))
         .into_any_element()
}

/// Converts a `SystemTime` to a Unix timestamp in seconds, saturating to 0 for
/// timestamps before the epoch (should not occur in practice).
fn unix_secs(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
     .map(|d| d.as_secs() as i64)
     .unwrap_or(0)
}

fn render_entry_row(entry: &AlertEntry, text_color: gpui::Hsla, text_tertiary: gpui::Hsla,
                    border: gpui::Hsla)
                    -> impl IntoElement + 'static + use<> {
    let ts = unix_secs(entry.occurred_at);
    let relative_label = format_relative(ts);
    let absolute_label = format_absolute(ts);
    let label = entry.label.to_string();
    let message = entry.message.clone();

    div()
        .id(("alert-history-row", entry.id))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .px(px(14.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(border)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div().min_w_0().flex_1().child(
                        selectable_text(("alert-history-label", entry.id), label)
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(text_color)
                            .truncate(),
                    ),
                )
                .child(
                    div()
                        .id(("alert-history-ts", entry.id))
                        .flex_none()
                        .text_xs()
                        .text_color(text_tertiary)
                        .child(relative_label)
                        .tooltip(move |window, cx| {
                            Tooltip::new(absolute_label.clone()).build(window, cx)
                        }),
                ),
        )
        .child(render_copyable_message(entry.id, message, text_tertiary))
}

/// Renders an alert entry's full (word-wrapped, not truncated) error message
/// alongside an always-visible copy button.
///
/// `min_w_0()` on the message text is required for it to wrap within the
/// panel's fixed width instead of overflowing past it — a flex item's default
/// min-width is its content's intrinsic width, which for a long unwrapped
/// error string is wider than the 340px panel.
fn render_copyable_message(entry_id: u64, message: String, text_tertiary: gpui::Hsla)
                           -> impl IntoElement + 'static {
    div()
        .id(("alert-history-message-row", entry_id))
        .flex()
        .items_start()
        .gap(px(6.0))
        .child(
            div().min_w_0().flex_1().child(
                selectable_text(("alert-history-message", entry_id), message.clone())
                    .text_xs()
                    .text_color(text_tertiary),
            ),
        )
        .child(
            div().flex_none().child(
                Clipboard::new(SharedString::from(format!("alert-msg-{entry_id}-copy")))
                    .value(message)
                    .tooltip(t!("alert_history.copy_tooltip").to_string()),
            ),
        )
}
