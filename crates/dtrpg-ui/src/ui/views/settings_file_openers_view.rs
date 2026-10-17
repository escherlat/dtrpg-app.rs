//! File Openers settings section: CRUD for extension → application overrides.

use std::path::PathBuf;

use dtrpg_core::data::file_openers::FileOpenerEntry;
use gpui::prelude::*;
use gpui::{AnyElement, App, Entity, IntoElement, ParentElement, Styled, Window, div, px};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::tooltip::Tooltip;
use rust_i18n::t;

use crate::controllers::settings::SettingsController;
use crate::data::theme::ColorTokens;
use crate::ui::widgets::selectable_text;

/// Renders the File Openers settings section.
///
/// `pending_file_opener`, when `Some`, is the application path picked via the
/// native file dialog and awaiting an extension; a pending row renders in its
/// place in the list with `extension_input` focused for inline entry.
pub fn render_file_openers_section(
    file_openers: &[FileOpenerEntry], entity: Entity<SettingsController>, colors: &ColorTokens,
    extension_input: Entity<InputState>, pending_file_opener: Option<PathBuf>,
) -> impl IntoElement + 'static + use<> {
    let text_primary = colors.text_primary;
    let text_tertiary = colors.text_tertiary;
    let border = colors.border;
    let accent = colors.accent;
    let accent_on = colors.accent_on;

    // Collect extensions whose app path no longer exists on disk.
    let stale_extensions: Vec<String> = file_openers
        .iter()
        .filter(|e| !e.app_path.exists())
        .map(|e| e.extension.clone())
        .collect();

    let is_pending = pending_file_opener.is_some();

    let mut col = div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .p(px(24.0))
        // ── Header row ────────────────────────────────────────────────────
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(text_primary)
                                .child(t!("settings.file_openers_title")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(text_tertiary)
                                .child(t!("settings.file_openers_description")),
                        ),
                )
                .child(render_add_button(
                    entity.clone(),
                    extension_input.clone(),
                    accent,
                    accent_on,
                    is_pending,
                )),
        )
        // ── Divider ───────────────────────────────────────────────────────
        .child(div().h(px(1.0)).bg(border));

    if let Some(app_path) = pending_file_opener {
        col = col.child(render_pending_row(
            &app_path,
            extension_input,
            entity.clone(),
            colors,
        ));
    }

    if file_openers.is_empty() && !is_pending {
        col = col.child(render_empty_state(text_tertiary));
    } else {
        for entry in file_openers {
            let is_stale = stale_extensions.contains(&entry.extension);
            col = col.child(render_entry_row(entry, is_stale, entity.clone(), colors));
        }
    }

    col
}

// ── Empty state
// ───────────────────────────────────────────────────────────────

fn render_empty_state(text_tertiary: gpui::Hsla) -> impl IntoElement + 'static {
    div()
        .flex()
        .items_center()
        .justify_center()
        .py(px(32.0))
        .child(
            div()
                .text_sm()
                .text_color(text_tertiary)
                .child(t!("settings.file_openers_empty")),
        )
}

// ── Pending row (inline add)
// ───────────────────────────────────────────────────

/// Renders the in-progress "add file opener" row: app name plus an editable,
/// focused extension input, in place of the static extension badge used by
/// committed entries.
///
/// Escape cancels the pending add without persisting anything, and stops the
/// keystroke from bubbling to the settings panel's own Escape-to-close handler.
fn render_pending_row(
    app_path: &std::path::Path, extension_input: Entity<InputState>,
    entity: Entity<SettingsController>, colors: &ColorTokens,
) -> AnyElement {
    let text_secondary = colors.text_secondary;
    let text_tertiary = colors.text_tertiary;
    let border = colors.border;
    let app_name = app_name_from_path(app_path);

    let entity_cancel = entity.clone();

    div()
        .id("file-opener-pending-row")
        .flex()
        .items_center()
        .gap(px(12.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(border)
        .on_key_down(move |event, _window, cx| {
            if event.keystroke.key == "escape" {
                cx.stop_propagation();
                entity_cancel.update(cx, |ctrl, cx| ctrl.cancel_pending_file_opener(cx));
            }
        })
        // ── Extension input ───────────────────────────────────────────────
        .child(div().w(px(90.0)).child(Input::new(&extension_input)))
        // ── App name ──────────────────────────────────────────────────────
        .child(
            div().flex_1().min_w_0().child(
                selectable_text("file-opener-pending-app-name", app_name)
                    .text_sm()
                    .text_color(text_secondary)
                    .truncate(),
            ),
        )
        // ── Cancel button ─────────────────────────────────────────────────
        .child(
            div()
                .id("cancel-pending-file-opener")
                .size(px(28.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(border)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .tooltip(|window, cx| {
                    Tooltip::new(t!("settings.file_opener_add_cancel").to_string())
                        .build(window, cx)
                })
                .on_click(move |_, _, cx| {
                    entity.update(cx, |ctrl, cx| ctrl.cancel_pending_file_opener(cx));
                })
                .child(div().text_xs().text_color(text_tertiary).child("\u{00d7}")),
        )
        .into_any_element()
}

// ── Entry row
// ─────────────────────────────────────────────────────────────────

fn render_entry_row(
    entry: &FileOpenerEntry, is_stale: bool, entity: Entity<SettingsController>,
    colors: &ColorTokens,
) -> AnyElement {
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;
    let text_tertiary = colors.text_tertiary;
    let border = colors.border;

    let extension = entry.extension.clone();
    let app_name = app_name_from_path(&entry.app_path);
    let ext_label = format!(".{extension}");

    let entity_remove = entity.clone();
    let extension_for_remove = extension.clone();

    div()
        .id(format!("file-opener-row-{extension}"))
        .flex()
        .items_center()
        .gap(px(12.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(border)
        // ── Extension badge ───────────────────────────────────────────────
        .child(
            div()
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(border)
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(text_primary)
                .child(ext_label),
        )
        // ── App name ──────────────────────────────────────────────────────
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    selectable_text(format!("file-opener-app-name-{extension}"), app_name)
                        .text_sm()
                        .text_color(text_secondary)
                        .truncate(),
                )
                .when(is_stale, |el| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(colors.warning_text)
                            .child(format!("⚠ {}", t!("settings.file_opener_app_not_found"))),
                    )
                }),
        )
        // ── Remove button ─────────────────────────────────────────────────
        .child(
            div()
                .id(format!("remove-opener-{extension}"))
                .size(px(28.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(border)
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .tooltip(|window, cx| {
                    Tooltip::new(t!("settings.file_opener_remove_tooltip").to_string())
                        .build(window, cx)
                })
                .on_click(move |_, window, cx| {
                    let ext = extension_for_remove.clone();
                    let entity = entity_remove.clone();
                    window.open_alert_dialog(cx, move |alert, _, _| {
                        let ext2 = ext.clone();
                        let entity2 = entity.clone();
                        alert
                            .confirm()
                            .title(
                                t!("settings.file_opener_remove_confirm_title", ext = ext)
                                    .to_string(),
                            )
                            .description(
                                t!("settings.file_opener_remove_confirm_description").to_string(),
                            )
                            .on_ok(move |_, _window, cx| {
                                entity2.update(cx, |ctrl, cx| {
                                    ctrl.remove_file_opener(&ext2, cx);
                                });
                                true
                            })
                    });
                })
                .child(div().text_xs().text_color(text_tertiary).child("\u{00d7}")),
        )
        .into_any_element()
}

// ── Add button
// ────────────────────────────────────────────────────────────────

fn render_add_button(
    entity: Entity<SettingsController>, extension_input: Entity<InputState>, accent: gpui::Hsla,
    accent_on: gpui::Hsla, is_pending: bool,
) -> impl IntoElement + 'static {
    // Disabled (dimmed, inert) while an add is already in progress, so a second
    // click can't start a new pending row and orphan the first one.
    let bg = if is_pending {
        accent.opacity(0.4)
    } else {
        accent
    };

    let mut btn = div()
        .id("add-file-opener")
        .size(px(30.0))
        .rounded(px(8.0))
        .bg(bg)
        .flex()
        .items_center()
        .justify_center()
        .tooltip(|window, cx| {
            Tooltip::new(t!("settings.file_opener_add_tooltip").to_string()).build(window, cx)
        })
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(accent_on)
                .child("+"),
        );

    if !is_pending {
        btn = btn.cursor_pointer().on_click(move |_, window, cx| {
            pick_app_and_begin_add(&entity, &extension_input, window, cx);
        });
    }

    btn
}

/// Runs the native application picker and, if an app was chosen, starts the
/// inline "add file opener" flow: resets and focuses `extension_input`,
/// reclaims OS key-window status (the native panel can steal it), and tells
/// the controller to render a pending row for the picked app.
///
/// The picker is presented asynchronously (`AsyncFileDialog`) instead of
/// blocking this handler: a blocking `rfd::FileDialog` runs AppKit's modal loop
/// while the handler still holds gpui's `App` borrow, and a foreground task
/// resuming mid-modal aborts in `AppCell::borrow_mut` (see issue #257). The
/// post-pick UI work therefore goes through the window context's `update`
/// rather than the borrowed `window`/`cx` handed to this function.
fn pick_app_and_begin_add(
    entity: &Entity<SettingsController>, extension_input: &Entity<InputState>, window: &mut Window,
    cx: &mut App,
) {
    let entity = entity.clone();
    let extension_input = extension_input.clone();
    window
        .to_async(cx)
        .spawn(async move |cx| {
            let picked = rfd::AsyncFileDialog::new()
                .add_filter(
                    t!("settings.file_opener_app_filter_name").to_string(),
                    &["app"],
                )
                .set_directory("/Applications")
                .pick_file()
                .await;
            let Some(handle) = picked else {
                return;
            };
            let app_path = handle.path().to_path_buf();

            cx.update(|window, cx| {
                extension_input.update(cx, |state, cx| state.set_value("", window, cx));
                // The native panel can leave the GPUI window without OS
                // key-window status. Without reclaiming it here, gpui's
                // focus tracking still thinks the handle is focused but no
                // keyboard events arrive, so the extension input below
                // silently refuses focus.
                window.activate_window();
            })
            .ok();

            entity.update(cx, |ctrl, cx| ctrl.begin_add_file_opener(app_path, cx));

            cx.update(|window, cx| {
                extension_input.update(cx, |state, cx| state.focus(window, cx));
            })
            .ok();
        })
        .detach();
}

// ── Helpers
// ───────────────────────────────────────────────────────────────────

/// Derives a display-friendly application name from the path.
fn app_name_from_path(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}
