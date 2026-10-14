//! Advanced settings section: cache visibility and destructive/maintenance
//! actions (cache details, opening the cache folder, cache clearing).
//!
//! Also renders the About section, which is purely informational (app name,
//! version, description) and shares no state with Advanced beyond both living
//! in the Settings panel.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Entity, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::description_list::{DescriptionItem, DescriptionList};
use gpui_component::input::{InputState, NumberInput};
use gpui_component::tooltip::Tooltip;
use rust_i18n::t;

use crate::build_info;
use crate::controllers::settings::{CacheCounts, SettingsController};
use crate::data::constants::{
    FORCE_RELOAD_COOLDOWN_SECS, ITEM_CHECK_BATCH_COOLDOWN_SECS, ITEM_CHECK_BATCH_TIMER_SECS,
    ITEM_CHECK_COOLDOWN_SECS, STALE_SECS, THUMBNAIL_COOLDOWN_SECS,
};
use crate::data::theme::ColorTokens;
use crate::ui::widgets::small_caps_text;
use crate::util::datetime::{format_absolute, format_relative};
use crate::util::pluralize::pluralize;

/// Formats a fixed duration in seconds as a human-readable, localized string
/// ("60 seconds", "5 minutes", "7 days"), routing the noun form through
/// [`pluralize`] rather than hand-formatting English plural suffixes.
///
/// Distinct from `activity_panel_view::format_duration`, which renders
/// *elapsed* time as "Xm Ys" — a style that reads oddly for a static
/// configuration value like a 7-day staleness window ("10080m 0s"). Chooses
/// the coarsest unit that divides evenly; falls back to seconds otherwise.
fn format_static_duration(secs: u64) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;

    if secs >= DAY && secs.is_multiple_of(DAY) {
        pluralize((secs / DAY) as usize, "count.day", "count.days")
    } else if secs >= HOUR && secs.is_multiple_of(HOUR) {
        pluralize((secs / HOUR) as usize, "count.hour", "count.hours")
    } else if secs >= MINUTE && secs.is_multiple_of(MINUTE) {
        pluralize((secs / MINUTE) as usize, "count.minute", "count.minutes")
    } else {
        pluralize(secs as usize, "count.second", "count.seconds")
    }
}

/// Renders the shared "Cache details" row frame: a label + value cell on one
/// line, with a short explanatory description beneath in tertiary text.
/// `value_el` is prebuilt so callers can supply either plain text
/// ([`stat_row`]) or a tooltip-carrying element ([`timestamp_row`]). The
/// label styling (font, weight, size, color) matches every other settings
/// section's row labels — see `settings_appearance_view::row_label`.
fn row_frame(
    label: impl Into<SharedString>, value_el: impl IntoElement,
    description: impl Into<SharedString>, colors: &ColorTokens, label_font_family: &str,
) -> impl IntoElement + 'static {
    let label = label.into();
    let description = description.into();
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .flex()
                .justify_between()
                .items_baseline()
                .gap(px(12.0))
                .child(
                    div()
                        .text_sm()
                        .font_family(label_font_family.to_string())
                        .text_color(colors.text_primary)
                        .child(small_caps_text(label)),
                )
                .child(value_el),
        )
        .child(
            div()
                .text_xs()
                .text_color(colors.text_tertiary)
                .child(description),
        )
}

/// Renders one "Cache details" data point with a plain-text value: counts
/// and the fixed timing/cooldown constants.
fn stat_row(
    label: impl Into<SharedString>, value: impl Into<SharedString>,
    description: impl Into<SharedString>, colors: &ColorTokens, label_font_family: &str,
    value_font_family: &str,
) -> impl IntoElement + 'static {
    let value_el = div()
        .text_sm()
        .text_right()
        .font_family(value_font_family.to_string())
        .text_color(colors.text_secondary)
        .child(value.into());
    row_frame(label, value_el, description, colors, label_font_family)
}

/// Renders a "Cache details" row for an actual recorded timestamp: the
/// visible value is the relative time ("2 hours ago"), with the absolute
/// timestamp available as a tooltip — matching this app's existing
/// relative-value/absolute-tooltip convention (see
/// `detail_panel_view::render_relative_date_value`) rather than showing
/// both inline. `id` must be unique per row — it identifies the value
/// element so it can carry a tooltip.
fn timestamp_row(
    id: &'static str, label: impl Into<SharedString>, ts: i64,
    description: impl Into<SharedString>, colors: &ColorTokens, label_font_family: &str,
    value_font_family: &str,
) -> impl IntoElement + 'static {
    let absolute = format_absolute(ts);
    let value_el = div()
        .id(id)
        .text_sm()
        .text_right()
        .font_family(value_font_family.to_string())
        .text_color(colors.text_secondary)
        .child(format_relative(ts))
        .tooltip(move |window, cx| Tooltip::new(absolute.clone()).build(window, cx));
    row_frame(label, value_el, description, colors, label_font_family)
}

/// Renders the "Cache details" subsection: per-type cache counts, the
/// cache-related timeout/cooldown constants, and — where a real timestamp
/// exists — a companion row showing exactly when that data was last
/// recorded, each with a label and description.
fn render_cache_details(
    cache_counts: CacheCounts, colors: &ColorTokens, label_font_family: &str,
    value_font_family: &str,
) -> impl IntoElement + 'static + use<> {
    let avatar_value = if cache_counts.avatar_cached {
        t!("settings.cache_avatar_value_cached").to_string()
    } else {
        t!("settings.cache_avatar_value_not_cached").to_string()
    };

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(colors.text_primary)
                .child(t!("settings.cache_details_title")),
        )
        .child(stat_row(
            t!("settings.cache_metadata_label"),
            cache_counts.metadata_items.to_string(),
            t!("settings.cache_metadata_description"),
            colors,
            label_font_family,
            value_font_family,
        ))
        .when_some(cache_counts.metadata_saved_at_secs, |this, ts| {
            this.child(timestamp_row(
                "cache-stat-metadata-saved",
                t!("settings.cache_metadata_saved_label"),
                ts,
                t!("settings.cache_metadata_saved_description"),
                colors,
                label_font_family,
                value_font_family,
            ))
        })
        .child(stat_row(
            t!("settings.cache_covers_label"),
            cache_counts.cover_thumbnails.to_string(),
            t!("settings.cache_covers_description"),
            colors,
            label_font_family,
            value_font_family,
        ))
        .child(stat_row(
            t!("settings.cache_avatar_label"),
            avatar_value,
            t!("settings.cache_avatar_description"),
            colors,
            label_font_family,
            value_font_family,
        ))
        .child(stat_row(
            t!("settings.cache_staleness_label"),
            format_static_duration(STALE_SECS),
            t!("settings.cache_staleness_description"),
            colors,
            label_font_family,
            value_font_family,
        ))
        .child(stat_row(
            t!("settings.cache_reload_cooldown_label"),
            format_static_duration(FORCE_RELOAD_COOLDOWN_SECS),
            t!("settings.cache_reload_cooldown_description"),
            colors,
            label_font_family,
            value_font_family,
        ))
        .child(stat_row(
            t!("settings.cache_item_check_cooldown_label"),
            format_static_duration(ITEM_CHECK_COOLDOWN_SECS),
            t!("settings.cache_item_check_cooldown_description"),
            colors,
            label_font_family,
            value_font_family,
        ))
        .child(stat_row(
            t!("settings.cache_batch_cooldown_label"),
            format_static_duration(ITEM_CHECK_BATCH_COOLDOWN_SECS),
            t!("settings.cache_batch_cooldown_description"),
            colors,
            label_font_family,
            value_font_family,
        ))
        .when_some(cache_counts.last_item_check_batch_secs, |this, ts| {
            this.child(timestamp_row(
                "cache-stat-last-batch-check",
                t!("settings.cache_last_batch_check_label"),
                ts,
                t!("settings.cache_last_batch_check_description"),
                colors,
                label_font_family,
                value_font_family,
            ))
        })
        .child(stat_row(
            t!("settings.cache_batch_timer_label"),
            format_static_duration(ITEM_CHECK_BATCH_TIMER_SECS),
            t!("settings.cache_batch_timer_description"),
            colors,
            label_font_family,
            value_font_family,
        ))
        .child(stat_row(
            t!("settings.cache_thumbnail_cooldown_label"),
            format_static_duration(THUMBNAIL_COOLDOWN_SECS),
            t!("settings.cache_thumbnail_cooldown_description"),
            colors,
            label_font_family,
            value_font_family,
        ))
}

/// Renders the "Recently Updated window" row: a label/note pair and an
/// editable [`NumberInput`] (bounded 7-90 via `InputState::min`/`max`, set
/// when the field was created) with built-in +/- stepper buttons.
///
/// Falls back to a plain, non-editable value when `input` is `None` (before
/// the root view attaches the shared input state), matching the pattern used
/// for `storage_path_input` in `settings_storage_view`.
fn render_recently_updated_window_row(
    recently_updated_window_days: u32, input: Option<Entity<InputState>>, colors: &ColorTokens,
) -> impl IntoElement + 'static + use<> {
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;

    let field: AnyElement = if let Some(input_state) = input {
        NumberInput::new(&input_state).into_any_element()
    } else {
        div()
            .text_sm()
            .text_color(text_primary)
            .child(recently_updated_window_days.to_string())
            .into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(text_primary)
                .child(t!("settings.recently_updated_window_title")),
        )
        .child(div().w(px(140.0)).child(field))
        .child(
            div()
                .text_xs()
                .text_color(text_secondary)
                .child(t!("settings.recently_updated_window_note")),
        )
}

/// Renders the Advanced settings section: the "Recently Updated window"
/// number field, cache visibility (counts and timing constants), an "Open
/// cache folder" action, and a "Clear cache" action with a confirmation
/// dialog.
#[allow(clippy::too_many_arguments)]
pub fn render_advanced_section(
    entity: Entity<SettingsController>, cache_counts: CacheCounts, colors: &ColorTokens,
    label_font_family: &str, value_font_family: &str, recently_updated_window_days: u32,
    recently_updated_window_input: Option<Entity<InputState>>,
) -> impl IntoElement + 'static + use<> {
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;
    let border = colors.border;
    let entity_open_folder = entity.clone();

    div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .p(px(24.0))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(text_primary)
                .child(t!("settings.advanced_title")),
        )
        .child(div().h(px(1.0)).bg(border))
        .child(render_recently_updated_window_row(
            recently_updated_window_days,
            recently_updated_window_input,
            colors,
        ))
        .child(div().h(px(1.0)).bg(border))
        .child(render_cache_details(
            cache_counts,
            colors,
            label_font_family,
            value_font_family,
        ))
        .child(
            Button::new("open-cache-folder-btn")
                .outline()
                .label(t!("settings.open_cache_folder_button"))
                .on_click(move |_, _, cx| {
                    entity_open_folder.read(cx).open_cache_folder();
                }),
        )
        .child(div().h(px(1.0)).bg(border))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_sm()
                        .text_color(text_secondary)
                        .child(t!("settings.clear_cache_description")),
                )
                .child(
                    Button::new("clear-cache-btn")
                        .danger()
                        .label(t!("settings.clear_cache_button"))
                        .on_click(move |_, window, cx| {
                            let entity = entity.clone();
                            window.open_alert_dialog(cx, move |alert, _, _| {
                                let entity = entity.clone();
                                alert
                                    .confirm()
                                    .title(t!("settings.clear_cache_confirm_title").to_string())
                                    .description(
                                        t!("settings.clear_cache_confirm_description").to_string(),
                                    )
                                    .on_ok(move |_, _window, cx| {
                                        entity.update(cx, |ctrl, cx| ctrl.clear_cache(cx));
                                        true
                                    })
                            });
                        }),
                ),
        )
}

/// Renders the About settings section: app name, version, and a short
/// description.
pub fn render_about_section(colors: &ColorTokens) -> impl IntoElement + 'static + use<> {
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;

    let build_info_list = DescriptionList::horizontal()
        .columns(1)
        .bordered(false)
        .child(about_info_item(
            t!("about.version_label").to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        ))
        .child(about_info_item(
            t!("about.commit_label").to_string(),
            build_info::GIT_HASH.to_string(),
        ))
        .child(about_info_item(
            t!("about.build_date_label").to_string(),
            build_info::BUILD_DATE.to_string(),
        ))
        .child(about_info_item(
            t!("about.target_label").to_string(),
            build_info::TARGET.to_string(),
        ));

    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(24.0))
        .child(
            div()
                .text_lg()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(text_primary)
                .child(t!("sidebar.app_name")),
        )
        .child(build_info_list)
        .child(
            div()
                .text_xs()
                .text_color(text_secondary)
                .child(t!("about.description")),
        )
}

/// Builds a right-aligned label/value row for the About section's build-info
/// `DescriptionList`. `DescriptionList` doesn't right-align its value cells on
/// its own (the value cell is a plain `flex_1` div), so the value is wrapped
/// in a full-width, right-aligned `div()` here.
fn about_info_item(label: String, value: String) -> DescriptionItem {
    DescriptionItem::new(label).value(div().w_full().text_right().child(value).into_any_element())
}

#[cfg(test)]
mod tests {
    use super::format_static_duration;

    #[test]
    fn formats_seconds_under_a_minute() {
        assert_eq!(format_static_duration(60), "1 minute");
        assert_eq!(format_static_duration(45), "45 seconds");
        assert_eq!(format_static_duration(1), "1 second");
    }

    #[test]
    fn formats_minutes() {
        assert_eq!(format_static_duration(300), "5 minutes");
        assert_eq!(format_static_duration(900), "15 minutes");
    }

    #[test]
    fn formats_days() {
        assert_eq!(format_static_duration(7 * 24 * 60 * 60), "7 days");
        assert_eq!(format_static_duration(24 * 60 * 60), "1 day");
    }
}
