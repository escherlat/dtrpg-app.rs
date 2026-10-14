//! Activity panel overlay: lists in-progress and recently-completed background
//! operations.

use std::time::Instant;

use gpui::prelude::*;
use gpui::{AnyElement, Entity, IntoElement, ParentElement, Styled, div, px, relative};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tooltip::Tooltip;
use rust_i18n::t;

use crate::controllers::activity::ActivityController;
use crate::data::activity::{ActivitySnapshot, ActivityStatus};
use crate::data::theme::ColorTokens;

/// Renders the activity panel's content. The caller (the status bar's `Popover`
/// wrapping the activity button) is responsible for anchoring and positioning
/// this content relative to the trigger button.
pub fn render_activity_panel(
    snap: &ActivitySnapshot, entity: Entity<ActivityController>, colors: &ColorTokens,
) -> AnyElement {
    let border = colors.border;
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;
    let text_tertiary = colors.text_tertiary;
    let accent = colors.accent;

    let close_entity = entity.clone();
    let selected_id = snap.selected_id;

    // No bg/border/shadow/rounding here — the enclosing `Popover` content
    // wrapper already supplies that framing (see `popover_style` in
    // `gpui-kit`); adding our own on top produced a nested double frame.
    div()
        .w(px(340.0))
        .flex()
        .flex_col()
        // ── Header ────────────────────────────────────────────────────────
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(14.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(border)
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(text_primary)
                        .child(t!("activity.title")),
                )
                .child(
                    div()
                        .id("activity-close")
                        .text_xs()
                        .text_color(text_tertiary)
                        .cursor_pointer()
                        .child("x")
                        .on_click(move |_, _, cx| {
                            close_entity.update(cx, |a, cx| a.toggle_panel(cx));
                        }),
                ),
        )
        // ── Item list ─────────────────────────────────────────────────────
        .child(if snap.items.is_empty() {
            render_empty(text_primary, text_tertiary)
        } else {
            div()
                .flex()
                .flex_col()
                .max_h(px(400.0))
                .overflow_y_scrollbar()
                .children(snap.items.iter().map(|item| {
                    let is_in_progress = matches!(item.status, ActivityStatus::InProgress);
                    render_item_row(
                        item.id,
                        item.label.as_ref(),
                        &item.status,
                        selected_id,
                        entity.clone(),
                        item.started_at,
                        item.elapsed_secs,
                        item.progress,
                        is_in_progress && item.cancel_fn.is_some(),
                        text_secondary,
                        text_tertiary,
                        accent,
                        border,
                    )
                }))
                .into_any_element()
        })
        .into_any_element()
}

fn render_empty(text_primary: gpui::Hsla, text_tertiary: gpui::Hsla) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .py(px(24.0))
        .gap(px(6.0))
        .child(div().text_2xl().text_color(text_tertiary).child("o"))
        .child(
            div()
                .text_sm()
                .text_color(text_primary)
                .child(t!("activity.empty")),
        )
        .child(
            div()
                .text_xs()
                .text_color(text_tertiary)
                .child(t!("activity.empty_hint")),
        )
        .into_any_element()
}

/// Formats a duration as "Xs" for under 60 s or "Xm Ys" for 60 s or more.
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m {s}s")
    }
}

#[allow(clippy::too_many_arguments)]
fn render_item_row(
    item_id: u64, label: &str, status: &ActivityStatus, selected_id: Option<u64>,
    activity_entity: Entity<ActivityController>, started_at: Instant, elapsed_secs: Option<u64>,
    progress: Option<f32>, has_cancel: bool, text_color: gpui::Hsla, text_tertiary: gpui::Hsla,
    accent: gpui::Hsla, border: gpui::Hsla,
) -> impl IntoElement + 'static + use<> {
    let is_in_progress = matches!(status, ActivityStatus::InProgress);

    let (icon, icon_color) = match status {
        ActivityStatus::InProgress => ("~", accent),
        ActivityStatus::Complete => ("+", text_color),
        ActivityStatus::Error(_) => ("!", text_tertiary),
    };
    let error_msg = if let ActivityStatus::Error(msg) = status {
        Some(msg.clone())
    } else {
        None
    };
    let label = label.to_string();
    let is_expanded = selected_id == Some(item_id);

    let elapsed_label = if is_in_progress {
        format_duration(started_at.elapsed().as_secs())
    } else {
        elapsed_secs.map(format_duration).unwrap_or_default()
    };

    // Tooltip shows the label as a bold title, plus the error message as a
    // secondary line underneath for error items.
    let tooltip_title = label.clone();
    let tooltip_error = error_msg.clone();

    let select_entity = activity_entity.clone();
    let cancel_entity = activity_entity;

    div()
        .id(format!("activity-row-{item_id}"))
        .flex()
        .flex_col()
        .px(px(14.0))
        .py(px(6.0))
        .cursor_pointer()
        .tooltip(move |window, cx| {
            let title = tooltip_title.clone();
            let error = tooltip_error.clone();
            Tooltip::element(move |_, _| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(title.clone()),
                    )
                    .children(error.clone().map(|err| div().child(err)))
            })
            .build(window, cx)
        })
        .on_click(move |_, _, cx| {
            select_entity.update(cx, |a, cx| a.select_activity(item_id, cx));
        })
        // ── Header line: icon + label + elapsed + optional cancel ─────────
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(icon_color)
                        .child(icon),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .child(if is_expanded {
                            div()
                                .text_xs()
                                .text_color(text_color)
                                .child(label)
                                .into_any_element()
                        } else {
                            div()
                                .text_xs()
                                .text_color(text_color)
                                .truncate()
                                .child(label)
                                .into_any_element()
                        })
                        .children(error_msg.map(|msg| {
                            if is_expanded {
                                div().text_xs().text_color(text_tertiary).child(msg)
                            } else {
                                div()
                                    .text_xs()
                                    .text_color(text_tertiary)
                                    .truncate()
                                    .child(msg)
                            }
                        })),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(text_tertiary)
                        .child(elapsed_label),
                )
                .children(has_cancel.then(|| {
                    div()
                        .id(format!("activity-cancel-{item_id}"))
                        .flex_none()
                        .px(px(4.0))
                        .text_xs()
                        .text_color(text_color)
                        .cursor_pointer()
                        .child("x")
                        // Stop propagation so clicking cancel doesn't
                        // also fire the row's own on_click (which
                        // selects/expands the item) — same class of
                        // bug as the sidebar's suffix buttons.
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            cancel_entity.update(cx, |a, cx| a.cancel_activity(item_id, cx));
                        })
                })),
        )
        // ── Progress bar (in-progress items only) ─────────────────────────
        //
        // `progress: None` means the total is genuinely unknown (e.g. the DriveThruRPG
        // API never reports a total item count) — render the empty track only, rather
        // than a fixed partial fill that looks identical whether the operation just
        // started or has been stalled for minutes. Real progress (label text showing a
        // growing item count, or a percentage once a total is known) is what actually
        // signals liveness in that case; see the callers of `update_label`.
        .children(is_in_progress.then(|| {
            div()
                .w_full()
                .h(px(3.0))
                .mt(px(4.0))
                .bg(border)
                .rounded(px(1.5))
                .children(
                    progress
                        .map(|fill| div().h_full().w(relative(fill)).bg(accent).rounded(px(1.5))),
                )
        }))
}
