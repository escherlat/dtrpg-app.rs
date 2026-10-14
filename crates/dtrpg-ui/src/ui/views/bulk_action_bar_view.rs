//! Bulk-action bar: shown below the toolbar while bulk-selection mode is
//! active and at least one item is selected (see `catalog-bulk-actions`).

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, Entity, IntoElement, ParentElement, SharedString, Styled, Window, div, px};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::data::collection::CollectionEntry;
use crate::data::constants::BULK_ACTION_CONFIRM_THRESHOLD;
use crate::data::enums::SelectionField;
use crate::data::library::LibraryItem;
use crate::data::theme::ColorTokens;
use crate::util::matching::member_ids_contain;

/// Runs `dispatch` immediately for a selection at or below
/// [`BULK_ACTION_CONFIRM_THRESHOLD`]; for a larger selection, asks for
/// confirmation first. `Open` in particular can launch one external-app
/// process per item, which has been observed to lock up the default viewer
/// app for a selection in the thousands.
fn confirm_and_dispatch(
    window: &mut Window, cx: &mut App, count: usize, action_label: SharedString,
    dispatch: impl Fn(&mut App) + Clone + 'static,
) {
    if count <= BULK_ACTION_CONFIRM_THRESHOLD {
        dispatch(cx);
        return;
    }
    window.open_alert_dialog(cx, move |alert, _, _| {
        let dispatch = dispatch.clone();
        alert
            .confirm()
            .title(t!("toolbar.bulk_confirm_title", n = count).to_string())
            .description(
                t!(
                    "toolbar.bulk_confirm_description",
                    action = action_label.clone()
                )
                .to_string(),
            )
            .on_ok(move |_, _window, cx| {
                dispatch(cx);
                true
            })
    });
}

/// Renders the bulk-action bar row. Caller is responsible for only
/// including this when selection mode is active and `bulk_selection_count >
/// 0`.
#[allow(clippy::too_many_arguments)]
pub fn render_bulk_action_bar(
    bulk_selection_count: usize, bulk_selected_ids: &HashSet<Arc<str>>, items: &[LibraryItem],
    collections: &[CollectionEntry], entity: Entity<LibraryController>,
    pattern_input: Entity<InputState>, pattern_field: Rc<Cell<SelectionField>>,
    colors: &ColorTokens,
) -> impl IntoElement + 'static {
    let surface = colors.surface;
    let border = colors.border;
    let text_secondary = colors.text_secondary;

    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(10.0))
        .px(px(18.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(border)
        .bg(surface)
        .child(
            div()
                .text_sm()
                .text_color(text_secondary)
                .child(t!("toolbar.selection_count", n = bulk_selection_count).to_string()),
        )
        .child(render_select_all_controls(entity.clone()))
        .child(render_pattern_select(
            entity.clone(),
            pattern_input,
            pattern_field,
        ))
        .child(div().flex_1())
        .child(render_bulk_actions(
            bulk_selection_count,
            bulk_selected_ids,
            items,
            collections,
            entity,
        ))
}

fn render_select_all_controls(entity: Entity<LibraryController>) -> impl IntoElement + 'static {
    let e1 = entity.clone();
    div()
        .flex()
        .gap(px(6.0))
        .child(
            Button::new("bulk-select-all")
                .ghost()
                .label(t!("toolbar.select_all"))
                .on_click(move |_, _, cx| {
                    e1.update(cx, |ctrl, cx| ctrl.bulk_select_all(cx));
                }),
        )
        .child(
            Button::new("bulk-deselect-all")
                .ghost()
                .label(t!("toolbar.deselect_all"))
                .on_click(move |_, _, cx| {
                    entity.update(cx, |ctrl, cx| ctrl.bulk_deselect_all(cx));
                }),
        )
}

fn field_label(field: SelectionField) -> std::borrow::Cow<'static, str> {
    match field {
        SelectionField::Title => t!("toolbar.select_matches_field_title"),
        SelectionField::Publisher => t!("toolbar.select_matches_field_publisher"),
        SelectionField::System => t!("toolbar.select_matches_field_system"),
    }
}

fn render_pattern_select(
    entity: Entity<LibraryController>, pattern_input: Entity<InputState>,
    pattern_field: Rc<Cell<SelectionField>>,
) -> impl IntoElement + 'static {
    let current_field = pattern_field.get();
    let field_for_menu = pattern_field.clone();
    let input_for_click = pattern_input.clone();

    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(Input::new(&pattern_input).w(px(160.0)))
        .child(
            Button::new("bulk-pattern-field")
                .ghost()
                .label(field_label(current_field))
                .dropdown_caret(true)
                .dropdown_menu(move |menu, _, _| {
                    let f1 = field_for_menu.clone();
                    let f2 = field_for_menu.clone();
                    let f3 = field_for_menu.clone();
                    menu.item(
                        PopupMenuItem::new(t!("toolbar.select_matches_field_title"))
                            .checked(current_field == SelectionField::Title)
                            .on_click(move |_, _, _| f1.set(SelectionField::Title)),
                    )
                    .item(
                        PopupMenuItem::new(t!("toolbar.select_matches_field_publisher"))
                            .checked(current_field == SelectionField::Publisher)
                            .on_click(move |_, _, _| f2.set(SelectionField::Publisher)),
                    )
                    .item(
                        PopupMenuItem::new(t!("toolbar.select_matches_field_system"))
                            .checked(current_field == SelectionField::System)
                            .on_click(move |_, _, _| f3.set(SelectionField::System)),
                    )
                }),
        )
        .child(
            Button::new("bulk-select-matches")
                .ghost()
                .label(t!("toolbar.select_matches"))
                .on_click(move |_, _, cx| {
                    let pattern = input_for_click.read(cx).value().to_string();
                    let field = pattern_field.get();
                    entity.update(cx, |ctrl, cx| {
                        ctrl.bulk_select_by_pattern(&pattern, field, cx);
                    });
                }),
        )
}

#[allow(clippy::too_many_arguments)]
fn render_bulk_actions(
    bulk_selection_count: usize, bulk_selected_ids: &HashSet<Arc<str>>, items: &[LibraryItem],
    collections: &[CollectionEntry], entity: Entity<LibraryController>,
) -> impl IntoElement + 'static {
    let add_collections = collections.to_vec();
    let remove_collections: Vec<_> = collections
        .iter()
        .filter(|c| {
            items
                .iter()
                .filter(|i| bulk_selected_ids.contains(&i.id))
                .any(|i| {
                    member_ids_contain(
                        &c.member_ids,
                        crate::util::matching::collection_member_id(i),
                        i.product_id,
                    )
                })
        })
        .cloned()
        .collect();

    let e_download = entity.clone();
    let e_remove_download = entity.clone();
    let e_thumbnail = entity.clone();
    let e_add = entity.clone();
    let e_remove = entity.clone();
    let e_open = entity;

    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(
            Button::new("bulk-download")
                .ghost()
                .label(t!("toolbar.bulk_download"))
                .on_click(move |_, window, cx| {
                    let e = e_download.clone();
                    confirm_and_dispatch(
                        window,
                        cx,
                        bulk_selection_count,
                        t!("toolbar.bulk_download").to_string().into(),
                        move |cx| {
                            e.update(cx, |ctrl, cx| ctrl.bulk_download(cx));
                        },
                    );
                }),
        )
        .child(
            Button::new("bulk-remove-download")
                .ghost()
                .label(t!("toolbar.bulk_remove_download"))
                .on_click(move |_, window, cx| {
                    let e = e_remove_download.clone();
                    confirm_and_dispatch(
                        window,
                        cx,
                        bulk_selection_count,
                        t!("toolbar.bulk_remove_download").to_string().into(),
                        move |cx| {
                            e.update(cx, |ctrl, cx| ctrl.bulk_remove_download(cx));
                        },
                    );
                }),
        )
        .child(
            Button::new("bulk-fetch-thumbnail")
                .ghost()
                .label(t!("toolbar.bulk_fetch_thumbnail"))
                .on_click(move |_, window, cx| {
                    let e = e_thumbnail.clone();
                    confirm_and_dispatch(
                        window,
                        cx,
                        bulk_selection_count,
                        t!("toolbar.bulk_fetch_thumbnail").to_string().into(),
                        move |cx| {
                            e.update(cx, |ctrl, cx| ctrl.bulk_fetch_thumbnail(cx));
                        },
                    );
                }),
        )
        .child(
            Button::new("bulk-add-to-collection")
                .ghost()
                .label(t!("toolbar.bulk_add_to_collection"))
                .dropdown_caret(true)
                .dropdown_menu(move |mut menu, _, _| {
                    for collection in &add_collections {
                        let id = collection.id;
                        let e = e_add.clone();
                        menu = menu.item(PopupMenuItem::new(collection.name.to_string()).on_click(
                            move |_, window, cx| {
                                let e = e.clone();
                                confirm_and_dispatch(
                                    window,
                                    cx,
                                    bulk_selection_count,
                                    t!("toolbar.bulk_add_to_collection").to_string().into(),
                                    move |cx| {
                                        e.update(cx, |ctrl, cx| {
                                            ctrl.bulk_add_to_collection(id, cx)
                                        });
                                    },
                                );
                            },
                        ));
                    }
                    menu
                }),
        )
        .child(
            Button::new("bulk-remove-from-collection")
                .ghost()
                .label(t!("toolbar.bulk_remove_from_collection"))
                .dropdown_caret(true)
                .dropdown_menu(move |mut menu, _, _| {
                    for collection in &remove_collections {
                        let id = collection.id;
                        let e = e_remove.clone();
                        menu = menu.item(PopupMenuItem::new(collection.name.to_string()).on_click(
                            move |_, window, cx| {
                                let e = e.clone();
                                confirm_and_dispatch(
                                    window,
                                    cx,
                                    bulk_selection_count,
                                    t!("toolbar.bulk_remove_from_collection").to_string().into(),
                                    move |cx| {
                                        e.update(cx, |ctrl, cx| {
                                            ctrl.bulk_remove_from_collection(id, cx)
                                        });
                                    },
                                );
                            },
                        ));
                    }
                    menu
                }),
        )
        .child(
            Button::new("bulk-open")
                .ghost()
                .label(t!("toolbar.bulk_open"))
                .on_click(move |_, window, cx| {
                    let e = e_open.clone();
                    confirm_and_dispatch(
                        window,
                        cx,
                        bulk_selection_count,
                        t!("toolbar.bulk_open").to_string().into(),
                        move |cx| {
                            e.update(cx, |ctrl, cx| ctrl.bulk_open(cx));
                        },
                    );
                }),
        )
}
