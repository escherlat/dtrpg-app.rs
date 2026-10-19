//! Native Integration settings section: how Libri works with the operating
//! system. For now, whether tags move between the library and files' tags.
//!
//! Libri keeps its own index as the source of truth and projects what it can
//! onto the operating system. What the platform cannot do is left out, not
//! faked: where files keep no tags, the switches are replaced by a note.

use dtrpg_core::data::native_preferences::NativePreferencesFile;
use dtrpg_core::platform::{Platform, PlatformIntegration};
use gpui::prelude::*;
use gpui::{AnyElement, Entity, SharedString, div, px};
use gpui_kit::component::switch::Switch;
use rust_i18n::t;

use crate::controllers::settings::SettingsController;
use crate::data::theme::ColorTokens;
use crate::ui::widgets::small_caps_text;

/// Renders the Native Integration page.
pub fn render_native_section(entity: Entity<SettingsController>, native: NativePreferencesFile,
                             colors: &ColorTokens, label_font_family: &str)
                             -> impl IntoElement + 'static + use<> {
    let tags: AnyElement = if Platform.capabilities().filesystem_tags {
        tag_switches(entity, native, colors).into_any_element()
    }
    else {
        div().text_xs()
             .text_color(colors.text_secondary)
             .child(t!("settings.native_tags_unavailable").to_string())
             .into_any_element()
    };

    div().flex()
         .flex_col()
         .gap(px(24.0))
         .p(px(24.0))
         .child(div().text_sm()
                     .font_weight(gpui::FontWeight::SEMIBOLD)
                     .text_color(colors.text_primary)
                     .child(t!("settings.native_title").to_string()))
         .child(div().h(px(1.0)).bg(colors.border))
         .child(div().text_xs()
                     .text_color(colors.text_secondary)
                     .child(t!("settings.native_intro").to_string()))
         .child(div().flex()
                     .flex_col()
                     .gap(px(12.0))
                     .child(div().font_family(label_font_family.to_string())
                                 .text_sm()
                                 .text_color(colors.text_primary)
                                 .child(small_caps_text(t!("settings.native_tags_heading").to_string())))
                     .child(tags))
}

/// The three tag switches, each with a title and what it does.
fn tag_switches(entity: Entity<SettingsController>, native: NativePreferencesFile,
                colors: &ColorTokens)
                -> impl IntoElement + 'static + use<> {
    let write = entity.clone();
    let read = entity.clone();
    let launch = entity;
    div().flex()
         .flex_col()
         .gap(px(16.0))
         .child(switch_row("native-write-tags",
                           t!("settings.native_write_title").to_string(),
                           t!("settings.native_write_note").to_string(),
                           native.write_tags_to_files,
                           colors,
                           move |checked, cx| {
                               write.update(cx, |ctrl, cx| {
                                        ctrl.set_write_tags_to_files(checked, cx)
                                    });
                           }))
         .child(switch_row("native-read-tags",
                           t!("settings.native_read_title").to_string(),
                           t!("settings.native_read_note").to_string(),
                           native.read_tags_from_files,
                           colors,
                           move |checked, cx| {
                               read.update(cx, |ctrl, cx| {
                                       ctrl.set_read_tags_from_files(checked, cx)
                                   });
                           }))
         .child(switch_row("native-reconcile-on-launch",
                           t!("settings.native_launch_title").to_string(),
                           t!("settings.native_launch_note").to_string(),
                           native.reconcile_on_launch,
                           colors,
                           move |checked, cx| {
                               launch.update(cx, |ctrl, cx| {
                                         ctrl.set_reconcile_on_launch(checked, cx)
                                     });
                           }))
}

/// One switch with its title and a note beneath, in the layout of the Storage
/// page's "Create collections" row.
fn switch_row(id: &'static str, title: String, note: String, checked: bool,
              colors: &ColorTokens, on_change: impl Fn(bool, &mut gpui::App) + 'static)
              -> AnyElement {
    div().flex()
         .flex_col()
         .gap(px(4.0))
         .child(div().flex()
                     .items_center()
                     .justify_between()
                     .child(div().text_sm()
                                 .font_weight(gpui::FontWeight::SEMIBOLD)
                                 .text_color(colors.text_primary)
                                 .child(SharedString::from(title)))
                     .child(Switch::new(id).checked(checked)
                                           .on_click(move |checked, _window, cx| {
                                               on_change(*checked, cx)
                                           })))
         .child(div().text_xs()
                     .text_color(colors.text_secondary)
                     .child(SharedString::from(note)))
         .into_any_element()
}
