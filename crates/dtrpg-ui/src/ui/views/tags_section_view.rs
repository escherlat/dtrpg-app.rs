//! The Tags row of the detail panel: the item's tags as chips, with a button to
//! change them.
//!
//! Chips show a tag's color and have a button that takes the tag off the item.
//! A right click on a chip sets the tag's color. The note under the chips says
//! where the tags are written, and only where that is somewhere.

use std::sync::Arc;

use dtrpg_core::data::library::LibraryItem;
use dtrpg_core::platform::{Platform, PlatformIntegration, PlatformKind};
use gpui::prelude::*;
use gpui::{App, Entity, SharedString, div, px};
use gpui_kit::component::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::ContextMenuExt;
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::data::theme::ColorTokens;
use crate::ui::library::tag::{color_menu, tag_dot};
use crate::ui::views::manage_tags_dialog::open_manage_tags_dialog;

/// Renders the item's tags and the button that changes them.
pub fn render_tags_section(item: &LibraryItem, entity: Entity<LibraryController>,
                           colors: &ColorTokens, cx: &App)
                           -> impl IntoElement + 'static {
    let tags = entity.read(cx).tags_of(&item.id);
    let item_id = Arc::clone(&item.id);
    let item_title = Arc::clone(&item.title);

    let chips: Vec<_> = tags.into_iter().enumerate().map(|(ix, tag)| {
        let name = tag.name.clone();
        let remove = {
            let entity = entity.clone();
            let item_id = item_id.clone();
            let name = name.clone();
            Button::new(("detail-tag-remove", ix)).ghost()
                                                  .compact()
                                                  .icon(IconName::Close)
                                                  .tooltip(t!("detail.tags_remove_tooltip",
                                                              name = name.to_string()).to_string())
                                                  .on_click(move |_, _, cx| {
                                                      entity.update(cx, |ctrl, cx| {
                                                                ctrl.remove_tag(&item_id, &name, cx);
                                                            });
                                                  })
        };
        let entity = entity.clone();
        div().id(("detail-tag", ix))
             .flex()
             .items_center()
             .gap(px(6.0))
             .pl(px(10.0))
             .pr(px(2.0))
             .py(px(1.0))
             .rounded_full()
             .border_1()
             .border_color(colors.border)
             .bg(colors.surface_alt)
             .child(tag_dot(tag.color, colors.text_tertiary))
             .child(div().text_sm()
                         .text_color(colors.text_primary)
                         .child(SharedString::from(tag.name.to_string())))
             .child(remove)
             .context_menu(move |menu, _, _| color_menu(menu, &entity, &tag.name, tag.color))
    }).collect();

    let add = Button::new("detail-tags-add").ghost()
                                            .outline()
                                            .compact()
                                            .icon(IconName::Plus)
                                            .label(t!("detail.tags_add").to_string())
                                            .on_click(move |_, window, cx| {
                                                open_manage_tags_dialog(window,
                                                                        cx,
                                                                        entity.clone(),
                                                                        Arc::clone(&item_id),
                                                                        Arc::clone(&item_title));
                                            });

    let note = div().text_xs()
                    .text_color(colors.text_tertiary)
                    .child(tags_note());

    div().flex()
         .flex_col()
         .gap(px(6.0))
         .child(div().text_sm()
                     .font_weight(gpui::FontWeight::SEMIBOLD)
                     .text_color(colors.text_primary)
                     .child(t!("detail.tags_heading").to_string()))
         .child(div().flex()
                     .flex_wrap()
                     .items_center()
                     .gap(px(6.0))
                     .children(chips)
                     .child(add))
         .child(note)
}

/// What to say about where tags are written, for the platform this runs on.
fn tags_note() -> String {
    if !Platform.capabilities().filesystem_tags {
        return t!("detail.tags_note_index_only").to_string();
    }
    match Platform.kind() {
        PlatformKind::MacOs => t!("detail.tags_note_macos"),
        PlatformKind::Linux => t!("detail.tags_note_linux"),
        PlatformKind::Windows => t!("detail.tags_note_index_only"),
    }.to_string()
}
