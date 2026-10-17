//! Root view for the About window: a separate, non-modal window (matching
//! Knot's and Fernrohr's About window) rather than a dialog overlaid on the
//! main library window.

use gpui::{
    Context, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, Window, div, px,
};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use rust_i18n::t;

use crate::build_info;
use crate::data::theme::LibriTheme;

/// Root view for the About window.
pub struct AboutWindowView {
    focus_handle: FocusHandle,
}

impl AboutWindowView {
    /// Constructs the About window's root view and focuses it immediately so
    /// Escape closes the window without an extra click first.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self { focus_handle }
    }
}

impl Focusable for AboutWindowView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AboutWindowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.global::<LibriTheme>().clone();
        let colors = &theme.colors;
        window.set_rem_size(theme.fonts.ui_text_size);

        let label = |text: String| {
            div()
                .text_xs()
                .text_color(colors.text_secondary)
                .child(text)
        };

        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(colors.surface)
            .text_color(colors.text_primary)
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .px_6()
            .py_8()
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t!("sidebar.app_name")),
            )
            .child(
                div()
                    .text_sm()
                    .child(t!("about.version", version = env!("CARGO_PKG_VERSION"))),
            )
            .child(
                div()
                    .text_xs()
                    .text_center()
                    .max_w(px(260.))
                    .child(t!("about.description")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .mt_2()
                    .pt_2()
                    .child(label(
                        t!(
                            "report.diagnostics_build",
                            date = build_info::BUILD_DATE,
                            commit = build_info::GIT_HASH
                        )
                        .to_string(),
                    ))
                    .child(label(
                        t!("report.diagnostics_platform", target = build_info::TARGET).to_string(),
                    )),
            )
            .child(
                div().mt_4().child(
                    Button::new("about-close")
                        .primary()
                        .label(t!("about.ok"))
                        .on_click(|_, window, _cx| {
                            window.remove_window();
                        }),
                ),
            )
    }
}
