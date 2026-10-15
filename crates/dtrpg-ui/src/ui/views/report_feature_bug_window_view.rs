//! "Report a Feature/Bug" window: a kind selector (Bug/Feature Request), a
//! subject, and a description field, with read-only diagnostic context
//! (version, commit, build date, target) appended automatically. Submission
//! opens the user's browser to a pre-filled GitHub "new issue" page — no
//! background network call, no stored token.
//!
//! Its own window rather than a modal dialog, matching Knot's "Report an
//! Issue…" window (see `openspec/specs/bug-reporting` in the Knot repo), but
//! keeping `shared-report-feature-bug`'s browser-handoff submission
//! mechanism rather than Knot's `gh`-CLI path — see
//! `openspec/changes/define-shared-report-feature-bug` in the `dtrpg-app`
//! meta-repo for that design decision, which this window does not revisit.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, MouseButton, Render, Size, TitlebarOptions,
    Window, WindowBounds, WindowHandle, WindowOptions, div, point, px,
};
use gpui_kit::component::Disableable as _;
use gpui_kit::component::Root;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
use gpui_kit::component::radio::Radio;
use gpui_kit::component::scroll::ScrollableElement;
use rust_i18n::t;

use crate::build_info;
use crate::data::theme::LibriTheme;
use crate::ui::widgets::selectable_text;

const ISSUE_URL_BASE: &str = "https://github.com/pilgrimagesoftware/dtrpg-app.rs/issues/new";
const WINDOW_WIDTH: f32 = 440.0;
const WINDOW_HEIGHT: f32 = 480.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportKind {
    Bug,
    Feature,
}

impl ReportKind {
    /// Prefix applied to the composed issue title — there is no authenticated
    /// API call to apply a GitHub label with, so the kind is encoded as plain
    /// text instead (per `shared-report-feature-bug`'s design decision).
    fn title_prefix(self) -> &'static str {
        match self {
            ReportKind::Bug => "[Bug]",
            ReportKind::Feature => "[Feature Request]",
        }
    }
}

/// Root view for the "Report a Feature/Bug" window.
pub struct ReportFeatureBugWindowView {
    kind:         Rc<RefCell<ReportKind>>,
    subject:      Entity<InputState>,
    description:  Entity<TextareaState>,
    focus_handle: FocusHandle,
}

impl ReportFeatureBugWindowView {
    /// Constructs the window's root view and focuses it immediately so
    /// Escape closes the window without an extra click first.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let subject = cx.new(|cx| {
                            InputState::new(window, cx).placeholder(t!("report.subject_placeholder")
                                                                     .to_string())
                        });
        let description = cx.new(|cx| {
                                TextareaState::new(window, cx).rows(6)
                         .placeholder(t!("report.description_placeholder").to_string())
                            });
        Self { kind: Rc::new(RefCell::new(ReportKind::Bug)),
               subject,
               description,
               focus_handle }
    }
}

impl Focusable for ReportFeatureBugWindowView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Opens the "Report a Feature/Bug" window. Callers are responsible for
/// tracking the returned handle and bringing an already-open window forward
/// instead of calling this again — see
/// `LibraryRootView::show_report_feature_bug`.
#[allow(clippy::expect_used)]
pub fn open_report_feature_bug_window(cx: &mut App) -> WindowHandle<Root> {
    let window_size = Size { width:  px(WINDOW_WIDTH),
                             height: px(WINDOW_HEIGHT), };
    let (window, _view) =
        gpui_kit::open_window(WindowOptions {
                                  // Same chrome pattern as `open_settings_window`: hide the
                                  // native title text but keep the macOS traffic lights, and
                                  // disable minimize/resize since the content scrolls its own
                                  // overflow.
                                  titlebar: Some(TitlebarOptions {
                                      appears_transparent: true,
                                      traffic_light_position: Some(point(px(12.0), px(6.0))),
                                      ..Default::default()
                                  }),
                                  window_bounds: Some(WindowBounds::centered(window_size, cx)),
                                  is_resizable: false,
                                  is_minimizable: false,
                                  ..Default::default()
                              },
                              cx,
                              move |window, cx| {
                                  cx.new(|cx| ReportFeatureBugWindowView::new(window, cx))
                              })
                  .expect("failed to open report feature/bug window");
    // `gpui_kit::open_window` wraps the view in a `Root`, so the window's
    // root entity type is always `Root` — see `open_settings_window`'s
    // matching comment.
    window.downcast::<Root>()
          .expect("open_window's root entity is always Root")
}

/// `shared-report-feature-bug`'s submission-validation rule: Report is
/// reachable only once both fields carry more than whitespace.
fn fields_filled(subject: &str, description: &str) -> bool {
    !subject.trim().is_empty() && !description.trim().is_empty()
}

impl Render for ReportFeatureBugWindowView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.global::<LibriTheme>().colors.clone();
        let current_kind = *self.kind.borrow();
        let enabled = fields_filled(&self.subject.read(cx).value(),
                                    &self.description.read(cx).value());

        let label = |text: String| {
            div().text_xs()
                 .text_color(colors.text_secondary)
                 .child(text)
        };

        let kind_row = {
            let kind_bug = self.kind.clone();
            let kind_feature = self.kind.clone();
            div().flex()
                 .gap(px(16.0))
                 .child(Radio::new("report-kind-bug").label(t!("report.kind_bug").to_string())
                                                     .checked(current_kind == ReportKind::Bug)
                                                     .on_click(move |_, window, _cx| {
                                                         *kind_bug.borrow_mut() = ReportKind::Bug;
                                                         window.refresh();
                                                     }))
                 .child(Radio::new("report-kind-feature")
                         .label(t!("report.kind_feature").to_string())
                         .checked(current_kind == ReportKind::Feature)
                         .on_click(move |_, window, _cx| {
                             *kind_feature.borrow_mut() = ReportKind::Feature;
                             window.refresh();
                         }))
        };

        let diagnostics =
            div().flex()
                 .flex_col()
                 .gap_1()
                 .p_2()
                 .rounded(px(6.0))
                 .bg(colors.surface_alt)
                 .child(selectable_text("report-diagnostics-version",
                                        t!("report.diagnostics_version",
                                           version = env!("CARGO_PKG_VERSION")).to_string()))
                 .child(selectable_text("report-diagnostics-build",
                                        t!("report.diagnostics_build",
                                           date = build_info::BUILD_DATE,
                                           commit = build_info::GIT_HASH).to_string()))
                 .child(selectable_text("report-diagnostics-platform",
                                        t!("report.diagnostics_platform",
                                           target = build_info::TARGET).to_string()));

        let buttons = {
            let subject = self.subject.clone();
            let description = self.description.clone();
            let kind = self.kind.clone();
            div().flex()
                 .justify_end()
                 .gap_2()
                 .child(Button::new("report-cancel").label(t!("report.cancel").to_string())
                                                    .ghost()
                                                    .on_click(|_, window, _cx| {
                                                        window.remove_window();
                                                    }))
                 .child(Button::new("report-submit").label(t!("report.submit").to_string())
                                                    .primary()
                                                    .disabled(!enabled)
                                                    .on_click(move |_, window, cx| {
                                                        let subject_value =
                                                            subject.read(cx).value().to_string();
                                                        let description_value = description.read(cx)
                                                                                           .value()
                                                                                           .to_string();
                                                        if !fields_filled(&subject_value,
                                                                          &description_value)
                                                        {
                                                            return;
                                                        }
                                                        let url =
                                                            build_github_issue_url(*kind.borrow(),
                                                                                   &subject_value,
                                                                                   &description_value);
                                                        cx.open_url(&url);
                                                        window.remove_window();
                                                    }))
        };

        // Reserves vertical clearance for the macOS traffic light buttons
        // (see `open_report_feature_bug_window`'s `appears_transparent`) and
        // renders the window title — same pattern as the settings window's
        // `settings-drag-region`.
        let drag_region = div().id("report-drag-region")
                               .h(px(28.0))
                               .flex_none()
                               .flex()
                               .items_center()
                               .justify_center()
                               .text_base()
                               .line_height(px(28.0))
                               .font_weight(gpui::FontWeight::BOLD)
                               .text_color(colors.text_primary)
                               .child(t!("report.title").to_string())
                               .on_mouse_down(MouseButton::Left, |_, window, _| {
                                   window.start_window_move();
                               });

        div().id("report-window-root")
             .track_focus(&self.focus_handle)
             .on_key_down(move |event, window, _cx| {
                 if event.keystroke.key == "escape" {
                     window.remove_window();
                 }
             })
             .size_full()
             .bg(colors.surface)
             .flex()
             .flex_col()
             .child(drag_region)
             // `overflow_y_scrollbar()` wraps its element in `Scrollable`,
             // whose `render` only copies the `size` style field onto its
             // own outer wrapper — sizing utilities set here would be
             // silently dropped. The flex/scroll container lives on the
             // plain outer div instead; the inner div just needs
             // `size_full()` to fill it, which `Scrollable` already applies
             // on its own.
             .child(div().flex_1()
                         .min_h_0()
                         .child(div().flex()
                                     .flex_col()
                                     .size_full()
                                     .overflow_y_scrollbar()
                                     .gap_2()
                                     .px_4()
                                     .py_2()
                                     .child(kind_row)
                                     .child(label(t!("report.subject_label").to_string()))
                                     .child(Input::new(&self.subject).w_full())
                                     .child(label(t!("report.description_label").to_string()))
                                     .child(Textarea::new(&self.description).w_full().h(px(120.0)))
                                     .child(diagnostics)))
             .child(div().px_4().pb_4().child(buttons))
    }
}

/// The GitHub new-issue URL: the kind-prefixed subject as the issue title,
/// and the description with this build's version/commit/build-date/target
/// appended to the body so a report always carries the exact build it came
/// from.
fn build_github_issue_url(kind: ReportKind, subject: &str, description: &str) -> String {
    let title = format!("{} {subject}", kind.title_prefix());
    let body = format!(
                       "{description}\n\n\
        ---\n\
        **Version:** {}\n\
        **Commit:** {}\n\
        **Build Date:** {}\n\
        **Target:** {}\n",
                       env!("CARGO_PKG_VERSION"),
                       build_info::GIT_HASH,
                       build_info::BUILD_DATE,
                       build_info::TARGET
    );
    format!("{ISSUE_URL_BASE}?title={}&body={}",
            encode(&title),
            encode(&body))
}

/// Minimal percent-encoder for a URL query parameter value: keeps ASCII
/// alphanumerics, percent-encodes everything else (including `-_.~`, unlike
/// the standard "unreserved" set) — simple and unambiguous rather than
/// pulling in the `percent-encoding` crate for one call site.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() {
            out.push(*byte as char);
        }
        else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_disabled_until_both_fields_are_non_whitespace() {
        assert!(!fields_filled("", ""));
        assert!(!fields_filled("   ", "a description"));
        assert!(!fields_filled("a subject", "   "));
        assert!(!fields_filled("", "a description"));
        assert!(fields_filled("a subject", "a description"));
    }

    #[test]
    fn the_url_targets_the_app_repos_new_issue_page() {
        let url = build_github_issue_url(ReportKind::Bug,
                                         "Catalog shows duplicates",
                                         "Steps: open the app, wait for sync.");
        assert!(url.starts_with(ISSUE_URL_BASE));
    }

    #[test]
    fn the_title_carries_the_kind_prefix_and_subject() {
        let url = build_github_issue_url(ReportKind::Bug,
                                         "Catalog shows duplicates",
                                         "Steps: open the app, wait for sync.");
        assert!(url.contains("title=%5BBug%5D%20Catalog"),
                "missing kind-prefixed title: {url}");

        let url = build_github_issue_url(ReportKind::Feature, "Dark mode", "Please add it.");
        assert!(url.contains("title=%5BFeature%20Request%5D%20Dark"),
                "missing feature-kind-prefixed title: {url}");
    }

    #[test]
    fn the_body_carries_the_description_and_diagnostics() {
        let url =
            build_github_issue_url(ReportKind::Bug, "Subject", "Steps: open a cluster, retry.");
        assert!(url.contains("Steps%3A%20open"),
                "the description is missing: {url}");
        assert!(url.contains(&encode(env!("CARGO_PKG_VERSION"))),
                "the version is missing: {url}");
        assert!(url.contains(&encode(build_info::GIT_HASH)),
                "the commit is missing: {url}");
        assert!(url.contains(&encode(build_info::TARGET)),
                "the target is missing: {url}");
    }

    #[test]
    fn the_url_never_contains_a_raw_space_or_newline() {
        let url = build_github_issue_url(ReportKind::Bug,
                                         "A subject with spaces",
                                         "A description\nwith a newline.");
        assert!(!url.contains(' '),
                "a raw space breaks the query string: {url}");
        assert!(!url.contains('\n'),
                "a raw newline breaks the query string: {url}");
    }

    #[test]
    fn encode_keeps_alphanumerics_and_escapes_everything_else() {
        assert_eq!(encode("abc123"), "abc123");
        assert_eq!(encode("a b"), "a%20b");
        assert_eq!(encode("1.0.0"), "1%2E0%2E0");
    }
}
