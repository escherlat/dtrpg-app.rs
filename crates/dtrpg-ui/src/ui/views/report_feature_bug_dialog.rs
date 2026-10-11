//! "Report a Feature/Bug" dialog: a kind selector (Bug/Feature Request), a
//! subject, and a description field, with read-only diagnostic context
//! (version, commit, build date, target) appended automatically. Submission
//! opens the user's browser to a pre-filled GitHub "new issue" page — no
//! background network call, no stored token.
//!
//! Implements `shared-report-feature-bug` (see
//! `openspec/changes/define-shared-report-feature-bug` in the `dtrpg-app`
//! meta-repo for the parent, language-agnostic spec). Matches Fernrohr-App's
//! browser-handoff submission mechanism rather than Knot's `gh`-CLI path, per
//! that spec's design decision: no local tool dependency, no stored
//! credentials. Modeled on `manage_collections_dialog.rs`'s
//! `Rc<RefCell<_>>`-plus-`window.refresh()` pattern for state that isn't
//! itself a GPUI entity.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{App, Entity, Window, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dialog::{DialogContent, DialogHeader, DialogTitle};
use gpui_component::input::{Input, InputState};
use gpui_component::radio::Radio;
use gpui_component::{Disableable as _, WindowExt as _};
use rust_i18n::t;

use crate::build_info;
use crate::data::theme::LibriTheme;

const ISSUE_URL_BASE: &str = "https://github.com/pilgrimagesoftware/dtrpg-app.rs/issues/new";

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

/// Opens the "Report a Feature/Bug" dialog, unless one is already open — per
/// `shared-report-feature-bug`'s no-stacking requirement, a repeated trigger
/// leaves the existing dialog as-is rather than opening a second instance.
pub fn open_report_feature_bug_dialog(window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }

    let kind = Rc::new(RefCell::new(ReportKind::Bug));
    let subject =
        cx.new(|cx| {
              InputState::new(window, cx).placeholder(t!("report.subject_placeholder").to_string())
          });
    let description = cx.new(|cx| {
                            InputState::new(window, cx).multi_line(true)
                                   .rows(6)
                                   .placeholder(t!("report.description_placeholder").to_string())
                        });

    window.open_dialog(cx, move |dialog, _window, _cx| {
              let kind = kind.clone();
              let subject = subject.clone();
              let description = description.clone();
              dialog.w(px(440.))
                    .overlay_closable(true)
                    .content(move |content, window, cx| {
                        render_content(content, window, cx, &kind, &subject, &description)
                    })
          });
}

/// `shared-report-feature-bug`'s submission-validation rule: Report is
/// reachable only once both fields carry more than whitespace.
fn fields_filled(subject: &str, description: &str) -> bool {
    !subject.trim().is_empty() && !description.trim().is_empty()
}

fn render_content(content: DialogContent, _window: &mut Window, cx: &mut App,
                  kind: &Rc<RefCell<ReportKind>>, subject: &Entity<InputState>,
                  description: &Entity<InputState>)
                  -> DialogContent {
    let colors = cx.global::<LibriTheme>().colors.clone();
    let current_kind = *kind.borrow();
    let enabled = fields_filled(&subject.read(cx).value(), &description.read(cx).value());

    let label = |text: String| {
        div().text_xs()
             .text_color(colors.text_secondary)
             .child(text)
    };

    let kind_row = {
        let kind_bug = kind.clone();
        let kind_feature = kind.clone();
        div().flex()
             .gap(px(16.0))
             .child(Radio::new("report-kind-bug").label(t!("report.kind_bug").to_string())
                                                 .checked(current_kind == ReportKind::Bug)
                                                 .on_click(move |_, window, _cx| {
                                                     *kind_bug.borrow_mut() = ReportKind::Bug;
                                                     window.refresh();
                                                 }))
             .child(Radio::new("report-kind-feature").label(t!("report.kind_feature").to_string())
                                                     .checked(current_kind == ReportKind::Feature)
                                                     .on_click(move |_, window, _cx| {
                                                         *kind_feature.borrow_mut() =
                                                             ReportKind::Feature;
                                                         window.refresh();
                                                     }))
    };

    let diagnostics = div().flex()
                           .flex_col()
                           .gap_1()
                           .p_2()
                           .rounded(px(6.0))
                           .bg(colors.surface_alt)
                           .child(label(t!("report.diagnostics_version",
                                           version = env!("CARGO_PKG_VERSION")).to_string()))
                           .child(label(t!("report.diagnostics_build",
                                           date = build_info::BUILD_DATE,
                                           commit = build_info::GIT_HASH).to_string()))
                           .child(label(t!("report.diagnostics_platform",
                                           target = build_info::TARGET).to_string()));

    let buttons = {
        let subject = subject.clone();
        let description = description.clone();
        let kind = kind.clone();
        div().flex()
             .justify_end()
             .gap_2()
             .child(Button::new("report-cancel").label(t!("report.cancel").to_string())
                                                .ghost()
                                                .on_click(|_, window, cx| window.close_dialog(cx)))
             .child(Button::new("report-submit").label(t!("report.submit").to_string())
                                                .primary()
                                                .disabled(!enabled)
                                                .on_click(move |_, window, cx| {
                                                    let subject_value =
                                                        subject.read(cx).value().to_string();
                                                    let description_value =
                                                        description.read(cx).value().to_string();
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
                                                    window.close_dialog(cx);
                                                }))
    };

    content.child(DialogHeader::new().px_4()
                                     .pt_4()
                                     .child(DialogTitle::new().child(t!("report.title"))))
           .child(div().flex()
                       .flex_col()
                       .gap_2()
                       .px_4()
                       .py_2()
                       .child(kind_row)
                       .child(label(t!("report.subject_label").to_string()))
                       .child(Input::new(subject).w_full())
                       .child(label(t!("report.description_label").to_string()))
                       .child(Input::new(description).w_full().h(px(120.0)))
                       .child(diagnostics))
           .child(div().px_4().pb_4().child(buttons))
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
