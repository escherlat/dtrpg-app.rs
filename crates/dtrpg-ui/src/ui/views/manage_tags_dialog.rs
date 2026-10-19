//! "Manage tags" dialog: tick the tags an item has, and make a new one inline.
//!
//! Follows `manage_collections_dialog`. A change is made through the library
//! controller, which saves it and shows the result; a failure is reported in
//! the activity panel.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{App, Entity, Window, div, px};
use gpui_kit::component::button::Button;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::{DialogContent, DialogHeader, DialogTitle};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{WindowExt as _, v_flex};
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::data::theme::LibriTheme;
use crate::ui::library::tag::tag_dot;

/// Opens the Manage tags dialog for the item `item_id`.
pub fn open_manage_tags_dialog(window: &mut Window, cx: &mut App,
                               controller: Entity<LibraryController>, item_id: Arc<str>,
                               item_title: Arc<str>) {
    let new_tag_input =
        cx.new(|cx| {
              InputState::new(window, cx).placeholder(t!("tags.name_placeholder").to_string())
          });

    window.open_dialog(cx, move |dialog, _window, _cx| {
              let controller = controller.clone();
              let new_tag_input = new_tag_input.clone();
              let item_id = item_id.clone();
              let item_title = item_title.clone();
              dialog.w(px(360.))
                    .overlay_closable(true)
                    .content(move |content, _window, cx| {
                        render_content(content,
                                       cx,
                                       &controller,
                                       &new_tag_input,
                                       &item_id,
                                       &item_title)
                    })
          });
}

fn render_content(content: DialogContent, cx: &mut App, controller: &Entity<LibraryController>,
                  new_tag_input: &Entity<InputState>, item_id: &Arc<str>, item_title: &Arc<str>)
                  -> DialogContent {
    let colors = cx.global::<LibriTheme>().colors.clone();
    let (entries, has) = {
        let ctrl = controller.read(cx);
        (ctrl.tag_entries(), ctrl.tags_of(item_id))
    };

    let mut rows = v_flex().gap_2();
    for (ix, entry) in entries.iter().enumerate() {
        let checked = has.iter().any(|tag| tag.name == entry.name);
        let controller = controller.clone();
        let item_id = item_id.clone();
        let name = entry.name.clone();
        rows = rows.child(div().flex()
                               .items_center()
                               .gap(px(8.0))
                               .child(Checkbox::new(("manage-tags-row", ix)).label(entry.name
                                                                                    .to_string())
                                                                           .checked(checked)
                                                                           .on_click(move |new_checked,
                                                                                           _window,
                                                                                           cx| {
                                                                               controller.update(cx, |ctrl, cx| {
                                    if *new_checked {
                                        ctrl.add_tag(&item_id, &name, cx);
                                    } else {
                                        ctrl.remove_tag(&item_id, &name, cx);
                                    }
                                });
                                                                           }))
                               .child(tag_dot(entry.color, colors.text_tertiary)));
    }
    if entries.is_empty() {
        rows = rows.child(div().text_sm()
                               .text_color(colors.text_secondary)
                               .child(t!("tags.manage_empty").to_string()));
    }

    let new_tag_row = {
        let controller = controller.clone();
        let input = new_tag_input.clone();
        let item_id = item_id.clone();
        div().flex()
             .items_center()
             .gap(px(8.0))
             .child(div().flex_1().child(Input::new(new_tag_input)))
             .child(Button::new("manage-tags-new").label(t!("tags.new_confirm").to_string())
                                                  .on_click(move |_, window, cx| {
                                                      let name =
                                                          input.read(cx).value().trim().to_string();
                                                      if name.is_empty() {
                                                          return;
                                                      }
                                                      controller.update(cx, |ctrl, cx| {
                                                                    ctrl.add_tag(&item_id, &name,
                                                                                 cx);
                                                                });
                                                      input.update(cx, |st, cx| {
                                                               st.set_value("", window, cx)
                                                           });
                                                  }))
    };

    content.child(DialogHeader::new().px_4()
                                     .pt_4()
                                     .child(DialogTitle::new().child(t!("tags.manage_dialog_title",
                                                                        title = item_title.to_string())
                                                                      .to_string())))
           .child(div().px_4().py_2().child(rows))
           .child(div().px_4().pb_4().child(new_tag_row))
}
