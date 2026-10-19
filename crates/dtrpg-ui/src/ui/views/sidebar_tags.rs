//! The Tags section of the sidebar: each tag with its color and how many items
//! have it. Choosing one shows only those items; a right click sets its color.

use dtrpg_core::data::tag_library::TagEntry;
use dtrpg_core::data::ui_state::UiState;
use dtrpg_core::util::filter::SidebarFilter;
use gpui::prelude::*;
use gpui::{App, Entity, SharedString, div, px};
use gpui_kit::component::sidebar::{SidebarMenu, SidebarMenuItem};
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::controllers::tabs::{TabTarget, TabsController};
use crate::data::theme::LibriTheme;
use crate::ui::library::tag::{color_menu, tag_dot};

/// Builds the Tags menu: a collapsible header over a row for each tag in
/// `tags`. `active` is the filter now applied, so its tag is marked.
pub fn tags_menu(tags: Vec<TagEntry>, active: &SidebarFilter, entity: Entity<LibraryController>,
                 tabs: Entity<TabsController>)
                 -> SidebarMenu {
    let open = UiState::load().tags_open();
    let count = tags.len();
    let rows: Vec<SidebarMenuItem> = tags.into_iter()
                                         .map(|tag| tag_row(tag, active, &entity, &tabs))
                                         .collect();
    let toggle_entity = entity;
    SidebarMenu::new().child(SidebarMenuItem::new(t!("sidebar.tags").to_string())
        .default_open(open)
        .click_to_toggle(true)
        .on_click(move |_, _, cx| {
            UiState::load().save_tags_open(!open);
            toggle_entity.update(cx, |ctrl, cx| ctrl.notify_ui_change(cx));
        })
        .suffix(move |_, _| div().text_xs().child(count.to_string()))
        .children(rows))
}

/// One tag's row.
fn tag_row(tag: TagEntry, active: &SidebarFilter, entity: &Entity<LibraryController>,
           tabs: &Entity<TabsController>)
           -> SidebarMenuItem {
    let is_active = matches!(active, SidebarFilter::Tag { name, .. }
                             if name.to_lowercase() == tag.name.to_lowercase());
    let (click_entity, color_entity) = (entity.clone(), entity.clone());
    let tabs = tabs.clone();
    let (name, color, count) = (tag.name.clone(), tag.color, tag.count);
    let menu_name = tag.name.clone();
    SidebarMenuItem::new(SharedString::from(tag.name.to_string()))
        .active(is_active)
        .suffix(move |_, cx: &mut App| {
            let no_color = cx.global::<LibriTheme>().colors.text_tertiary;
            div().flex()
                 .items_center()
                 .gap(px(6.0))
                 .text_xs()
                 .child(tag_dot(color, no_color))
                 .child(count.to_string())
                 .into_any_element()
        })
        .on_click(move |_, _, cx| {
            click_entity.update(cx, |ctrl, cx| ctrl.filter_by_tag(&name, cx));
            tabs.update(cx, |ctrl, cx| ctrl.activate(TabTarget::Catalog, cx));
        })
        .context_menu(move |menu, _, _| color_menu(menu, &color_entity, &menu_name, color))
}
