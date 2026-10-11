//! Status bar view: library totals, active-tab summary, theme picker,
//! activity indicator, and notification indicator.
//!
//! Consolidates indicators previously scattered in the sidebar footer
//! (library totals, activity) into a single bottom row, per
//! `main-window-status-bar`. The notification indicator maps onto this
//! crate's existing alert history panel — there is no separate
//! notification-inbox capability in this app yet, so this relocates and
//! reuses that entry point rather than introducing a new one.
//!
//! The activity and alert history panels are rendered as `Popover`s anchored
//! to their trigger buttons here (rather than as root-level, hand-positioned
//! overlays) so they always appear next to the button that opens them,
//! regardless of window width or future status bar layout changes.

use gpui::prelude::*;
use gpui::{Anchor, Entity, IntoElement, Styled, div, px};
use gpui_component::IconName;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::popover::Popover;
use gpui_component::progress::ProgressCircle;
use gpui_component::separator::Separator;
use gpui_component::status_bar::StatusBar;
use gpui_component::{Sizable as _, Size};
use rust_i18n::t;

use crate::controllers::activity::ActivityController;
use crate::controllers::library::LibraryController;
use crate::data::activity::{ActivitySnapshot, AlertHistorySnapshot};
use crate::data::theme::{ColorTokens, ThemeKey};
use crate::i18n::Locale;
use crate::ui::views::activity_panel_view::render_activity_panel;
use crate::ui::views::alert_history_view::render_alert_history_panel;
use crate::util::pluralize::pluralize;
use crate::util::size::size_format;

/// Data the status bar needs to render, decoupled from where it's sourced.
pub struct StatusBarSnapshot {
    /// Total number of items in the library.
    pub total_count:      usize,
    /// Total size of all library items, in megabytes.
    pub total_mb:         f64,
    /// Title of the currently active tab (catalog section name, or an open
    /// detail tab's item title).
    pub active_tab_label: String,
    /// Item count for the active tab's content (catalog: matched item count;
    /// detail tab: always 1).
    pub active_tab_count: usize,
    /// Currently active theme.
    pub theme_key:        ThemeKey,
    /// Currently active locale.
    pub current_locale:   Locale,
}

/// Activity and alert history state needed to render the status bar's
/// activity/notification buttons and their anchored panels.
pub struct ActivityBarData<'a> {
    /// Controller entity, used to toggle the panels and act on their contents.
    pub entity:     Entity<ActivityController>,
    /// Activity panel snapshot for the current render pass.
    pub snap:       &'a ActivitySnapshot,
    /// Alert history panel snapshot for the current render pass.
    pub alert_snap: &'a AlertHistorySnapshot,
}

fn theme_label(key: ThemeKey) -> String {
    match key {
        ThemeKey::Parchment => t!("theme.parchment").to_string(),
        ThemeKey::Slate => t!("theme.slate").to_string(),
        ThemeKey::Sage => t!("theme.sage").to_string(),
        ThemeKey::Ink => t!("theme.ink").to_string(),
        ThemeKey::Moss => t!("theme.moss").to_string(),
        ThemeKey::Rosewood => t!("theme.rosewood").to_string(),
    }
}

/// Builds a `Button` tooltip with a title line above a descriptive body line.
///
/// `Button`'s tooltip only accepts plain text, so the title and body are
/// joined with a newline rather than rendered as separate styled elements.
fn status_bar_tooltip(title: &str, body: &str) -> String {
    format!("{title}\n{body}")
}

/// Renders the status bar row below the main content area.
pub fn render_status_bar(snap: StatusBarSnapshot, entity: Entity<LibraryController>,
                         activity_data: ActivityBarData<'_>, colors: &ColorTokens)
                         -> impl IntoElement + 'static {
    let total_size_str = size_format(snap.total_mb);

    let library_summary =
        div().text_xs()
             .text_color(colors.text_secondary)
             .child(format!("{} \u{2022} {total_size_str}",
                            pluralize(snap.total_count, "count.total_item", "count.total_items")));

    let active_tab_summary =
        div().text_xs()
             .text_color(colors.text_secondary)
             .child(format!("{} \u{2022} {}",
                            snap.active_tab_label,
                            pluralize(snap.active_tab_count, "count.item", "count.items")));

    let language_entity = entity.clone();
    let language_picker =
        Button::new("status-bar-language").ghost()
                                          .compact()
                                          .label(snap.current_locale.label())
                                          .tooltip(t!("status_bar.language_tooltip",
                                                      language = snap.current_locale.label())
                                                       .to_string())
                                          .dropdown_menu(move |menu, _, _| {
                                              let mut m = menu;
                                              for locale in Locale::ALL {
                                                  let e = language_entity.clone();
                                                  m = m.item(
                    PopupMenuItem::new(locale.label())
                        .checked(locale == snap.current_locale)
                        .on_click(move |_, _, cx| {
                            e.update(cx, |ctrl, cx| ctrl.set_locale(locale, cx));
                        }),
                );
                                              }
                                              m
                                          });

    let theme_picker =
        Button::new("status-bar-theme").ghost()
                                       .compact()
                                       .label(theme_label(snap.theme_key))
                                       .tooltip(t!("status_bar.theme_tooltip",
                                                   theme = theme_label(snap.theme_key)).to_string())
                                       .dropdown_menu(move |menu, _, _| {
                                           let mut m = menu;
                                           for key in [ThemeKey::Parchment,
                                                       ThemeKey::Slate,
                                                       ThemeKey::Sage,
                                                       ThemeKey::Ink,
                                                       ThemeKey::Moss,
                                                       ThemeKey::Rosewood]
                                           {
                                               let e = entity.clone();
                                               m = m.item(
                    PopupMenuItem::new(theme_label(key))
                        .checked(key == snap.theme_key)
                        .on_click(move |_, _, cx| {
                            e.update(cx, |ctrl, cx| ctrl.set_theme(key, cx));
                        }),
                );
                                           }
                                           m
                                       });

    let ActivityBarData { entity: activity,
                          snap: activity_snap,
                          alert_snap, } = activity_data;

    // Only mention completed items in the tooltip when there are any this
    // session — otherwise it always reads "0 completed", which is noise.
    let activity_tooltip_body = if activity_snap.recent_count > 0 {
        t!("status_bar.activity_tooltip",
           in_progress = activity_snap.in_progress_count,
           completed = activity_snap.recent_count)
    }
    else {
        t!("status_bar.activity_tooltip_in_progress_only",
           in_progress = activity_snap.in_progress_count)
    };
    // Idle state (no in-progress loaders) renders at value 0, which draws
    // only the faint background ring — an inactive circle rather than the
    // former "○" glyph. `gpui-component`'s default `progress_bar` theme
    // token is near-black, invisible against a dark status bar, so the
    // circle uses the app's accent color (already used for in-progress
    // activity elsewhere, e.g. `activity_panel_view`) instead.
    let progress_circle = if activity_snap.in_progress_count > 0 {
        match activity_snap.aggregate_progress {
            Some(fraction) => {
                ProgressCircle::new("status-bar-activity-progress").with_size(Size::Small)
                                                                   .color(colors.accent)
                                                                   .value(fraction * 100.0)
            }
            None => ProgressCircle::new("status-bar-activity-progress").with_size(Size::Small)
                                                                       .color(colors.accent)
                                                                       .loading(true),
        }
    }
    else {
        ProgressCircle::new("status-bar-activity-progress").with_size(Size::Small)
                                                           .color(colors.accent)
                                                           .value(0.0)
    };
    let activity_indicator =
        Button::new("status-bar-activity").ghost()
                                          .compact()
                                          .child(progress_circle)
                                          .tooltip(status_bar_tooltip(&t!("activity.title"),
                                                                      &activity_tooltip_body));

    let activity_for_open_change = activity.clone();
    let activity_panel = Popover::new("status-bar-activity-popover")
        .anchor(Anchor::BottomLeft)
        .trigger(activity_indicator)
        .open(activity_snap.panel_open)
        .on_open_change(move |open, _window, cx| {
            activity_for_open_change.update(cx, |a, cx| a.set_panel_open(*open, cx));
        })
        .child(render_activity_panel(
            activity_snap,
            activity.clone(),
            colors,
        ));

    let has_unread_alert = alert_snap.has_unread;
    let notification_button = Button::new("status-bar-notifications").ghost()
                                                                     .compact()
                                                                     .icon(IconName::Bell)
                                                                     .tooltip(status_bar_tooltip(
        &t!("status_bar.notifications_tooltip_title"),
        &t!(
            "status_bar.notifications_tooltip",
            n = activity_snap.alert_count
        ),
    ));

    let alert_for_open_change = activity.clone();
    // `Popover::trigger` requires `Selectable`, which `Badge` doesn't
    // implement, so the unread-alert dot is layered on as a sibling of the
    // popover rather than wrapping the trigger in a `Badge`.
    let notification_panel = div()
        .relative()
        .child(
            Popover::new("status-bar-notifications-popover")
                .anchor(Anchor::BottomRight)
                .trigger(notification_button)
                .open(alert_snap.open)
                .on_open_change(move |open, _window, cx| {
                    alert_for_open_change.update(cx, |a, cx| a.set_alert_panel_open(*open, cx));
                })
                .child(render_alert_history_panel(alert_snap, activity, colors)),
        )
        .when(has_unread_alert, |this| {
            this.child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .size(px(6.0))
                    .rounded_full()
                    .bg(gpui::red()),
            )
        });

    StatusBar::new().left(library_summary)
                    .left(Separator::vertical().h_5())
                    .left(active_tab_summary)
                    .right(language_picker)
                    .right(Separator::vertical().h_5())
                    .right(theme_picker)
                    .right(Separator::vertical().h_5())
                    .right(activity_panel)
                    .right(Separator::vertical().h_5())
                    .right(notification_panel)
}
