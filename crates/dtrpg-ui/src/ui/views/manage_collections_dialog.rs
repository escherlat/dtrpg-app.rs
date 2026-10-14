//! "Manage Collections" dialog: view and change which collections a single
//! catalog item belongs to, and create new collections inline.
//!
//! Replaces the context menu's former "Add to…" submenu and "Remove from
//! this collection" item (see `collection-membership-editing`), which had a
//! `PopupMenu` dismiss-cascade bug at two of its four call sites.

use std::cell::RefCell;
use std::rc::Rc;
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
use crate::data::events::{
    CollectionCreateFailed, CollectionMemberAddFailed, CollectionMemberRemoveFailed,
};
use crate::data::theme::LibriTheme;
use crate::util::matching::member_ids_contain;

/// Transient state shared by the dialog's closures for one open session.
struct ManageCollectionsState {
    error: Option<String>,
}

/// Opens the Manage Collections dialog for a single catalog item.
///
/// `member_id` is the item's `collection_member_id` (see `util::matching`) —
/// the same id space `CollectionEntry::member_ids` uses; it drives the
/// checked state and removal. `product_id` is the item's catalog
/// `product_id`, sent as the network add call's product identifier.
pub fn open_manage_collections_dialog(
    window: &mut Window, cx: &mut App, controller: Entity<LibraryController>, item_title: Arc<str>,
    member_id: u64, product_id: u64,
) {
    let state = Rc::new(RefCell::new(ManageCollectionsState { error: None }));
    let new_collection_input = cx.new(|cx| {
        InputState::new(window, cx).placeholder(t!("collections.name_placeholder").to_string())
    });

    subscribe_failure_events(window, cx, &controller, &state);

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let controller = controller.clone();
        let state = state.clone();
        let new_collection_input = new_collection_input.clone();
        let item_title = item_title.clone();
        dialog
            .w(px(360.))
            .overlay_closable(true)
            .content(move |content, window, cx| {
                render_content(
                    content,
                    window,
                    cx,
                    &controller,
                    &state,
                    &new_collection_input,
                    &item_title,
                    member_id,
                    product_id,
                )
            })
    });
}

/// Subscribes to the add/remove/create failure events for the lifetime of the
/// shared `state`. Uses a weak reference so a stale subscription (the dialog's
/// own `Rc` clones are dropped once the dialog closes) becomes a harmless no-op
/// rather than keeping the state alive forever.
fn subscribe_failure_events(
    window: &mut Window, cx: &mut App, controller: &Entity<LibraryController>,
    state: &Rc<RefCell<ManageCollectionsState>>,
) {
    let weak = Rc::downgrade(state);
    window
        .subscribe(controller, cx, {
            let weak = weak.clone();
            move |_ctrl, event: &CollectionMemberAddFailed, window, _cx| {
                if let Some(state) = weak.upgrade() {
                    state.borrow_mut().error = Some(event.message.clone());
                    window.refresh();
                }
            }
        })
        .detach();
    window
        .subscribe(controller, cx, {
            let weak = weak.clone();
            move |_ctrl, event: &CollectionMemberRemoveFailed, window, _cx| {
                if let Some(state) = weak.upgrade() {
                    state.borrow_mut().error = Some(event.message.clone());
                    window.refresh();
                }
            }
        })
        .detach();
    window
        .subscribe(
            controller,
            cx,
            move |_ctrl, event: &CollectionCreateFailed, window, _cx| {
                if let Some(state) = weak.upgrade() {
                    state.borrow_mut().error = Some(event.message.clone());
                    window.refresh();
                }
            },
        )
        .detach();
}

#[allow(clippy::too_many_arguments)]
fn render_content(
    content: DialogContent, _window: &mut Window, cx: &mut App,
    controller: &Entity<LibraryController>, state: &Rc<RefCell<ManageCollectionsState>>,
    new_collection_input: &Entity<InputState>, item_title: &Arc<str>, member_id: u64,
    product_id: u64,
) -> DialogContent {
    let colors = cx.global::<LibriTheme>().colors.clone();
    let collections = controller.read(cx).collections.clone();
    let error = state.borrow().error.clone();

    let mut rows = v_flex().gap_2();
    for collection in &collections {
        let checked = member_ids_contain(&collection.member_ids, member_id, product_id);
        let collection_id = collection.id;
        let controller = controller.clone();
        let state = state.clone();
        rows = rows.child(
            Checkbox::new(("manage-collections-row", collection_id))
                .label(collection.name.to_string())
                .checked(checked)
                .on_click(move |new_checked, _window, cx| {
                    state.borrow_mut().error = None;
                    controller.update(cx, |ctrl, cx| {
                        if *new_checked {
                            ctrl.add_item_to_collection(collection_id, member_id, product_id, cx);
                        } else {
                            ctrl.remove_item_from_collection(
                                collection_id,
                                member_id,
                                product_id,
                                cx,
                            );
                        }
                    });
                }),
        );
    }
    if collections.is_empty() {
        rows = rows.child(
            div()
                .text_sm()
                .text_color(colors.text_secondary)
                .child(t!("collections.manage_empty")),
        );
    }

    let new_collection_row = {
        let controller = controller.clone();
        let state = state.clone();
        let input = new_collection_input.clone();
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(div().flex_1().child(Input::new(new_collection_input)))
            .child(
                Button::new("manage-collections-new")
                    .label(t!("collections.manage_new_confirm"))
                    .on_click(move |_, window, cx| {
                        let name = input.read(cx).value().trim().to_string();
                        if name.is_empty() {
                            return;
                        }
                        state.borrow_mut().error = None;
                        controller.update(cx, |ctrl, cx| {
                            ctrl.create_collection_and_add_member(name, member_id, product_id, cx);
                        });
                        input.update(cx, |st, cx| st.set_value("", window, cx));
                    }),
            )
    };

    content
        .child(
            DialogHeader::new()
                .px_4()
                .pt_4()
                .child(DialogTitle::new().child(t!(
                    "collections.manage_dialog_title",
                    title = item_title.to_string()
                ))),
        )
        .child(div().px_4().py_2().child(rows))
        .when_some(error, |c, msg| {
            c.child(
                div()
                    .px_4()
                    .pb_2()
                    .text_sm()
                    .text_color(colors.error)
                    .child(msg),
            )
        })
        .child(div().px_4().pb_4().child(new_collection_row))
}
