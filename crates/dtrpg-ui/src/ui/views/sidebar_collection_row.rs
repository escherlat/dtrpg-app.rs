//! Collection row rendering.

use super::*;

/// Renders a single collection nav row: click to filter, right-click to
/// download all/reload/delete, and drop target for [`DraggedLibraryItem`]
/// (adds the dragged catalog item as a member of this collection).
pub(super) fn render_collection_row(id: impl Into<ElementId>, row: CollectionRow,
                         entity: Entity<LibraryController>, tabs: Entity<TabsController>,
                         cx: &mut App)
                         -> AnyElement {
    let is_active = row.is_active;
    let filter = SidebarFilter::Collection(row.id, Arc::clone(&row.name));
    let click_entity = entity.clone();
    let click_tabs = tabs;
    let download_entity = entity.clone();
    let reload_entity = entity.clone();
    let delete_entity = entity.clone();
    let drop_entity = entity;
    let col_id = row.id;
    let row_name_for_delete = row.name.to_string();

    div().id(id.into())
         .w_full()
         .flex()
         .items_center()
         .overflow_x_hidden()
         .flex_shrink_0()
         .p_2()
         .gap_x_2()
         .rounded(cx.theme().radius)
         .text_sm()
         .h_7()
         .when(!is_active, |this| {
             this.hover(|this| {
                     this.bg(cx.theme().sidebar_accent.opacity(0.8))
                         .text_color(cx.theme().sidebar_accent_foreground)
                 })
         })
         .when(is_active, |this| {
             this.font_medium()
                 .bg(cx.theme().tokens.sidebar_accent)
                 .text_color(cx.theme().sidebar_accent_foreground)
         })
         .on_click(move |_, _, cx| {
             click_entity.update(cx, |ctrl, cx| ctrl.set_filter(filter.clone(), cx));
             click_tabs.update(cx, |ctrl, cx| ctrl.activate(TabTarget::Catalog, cx));
         })
         .child(div().flex_1()
                     .flex()
                     .items_center()
                     .justify_between()
                     .gap_x_2()
                     .overflow_x_hidden()
                     .child(div().flex_1()
                                 .overflow_x_hidden()
                                 .child(row.name.to_string()))
                     .child(div().text_xs().child(row.count.to_string())))
         .drag_over::<DraggedLibraryItem>(|this, _, _, cx| this.bg(cx.theme().tokens.drop_target))
         .on_drop(move |drag: &DraggedLibraryItem, _, cx| {
             drop_entity.update(cx, |ctrl, cx| {
                            ctrl.add_item_to_collection(col_id,
                                                        drag.member_id,
                                                        drag.product_id,
                                                        cx);
                        });
         })
         .context_menu(move |menu, _, _| {
             menu.item(PopupMenuItem::new(t!("collections.download_all")).on_click({
                           let entity = download_entity.clone();
                           move |_, _, cx| {
                               entity.update(cx, |ctrl, cx| {
                                         ctrl.request_download_all_for_collection(col_id, cx);
                                     });
                           }
                       }))
                 .item(PopupMenuItem::new(t!("collections.reload")).on_click({
                           let entity = reload_entity.clone();
                           move |_, _, cx| {
                               entity.update(cx, |ctrl, cx| ctrl.load_collections(cx));
                           }
                       }))
                 .item(PopupMenuItem::new(t!("collections.delete")).on_click({
                           let entity = delete_entity.clone();
                           let collection_name = row_name_for_delete.clone();
                           move |_, window, cx| {
                               let entity = entity.clone();
                               let title = t!("collections.delete_confirm_title",
                                              name = collection_name).to_string();
                               window.open_alert_dialog(cx, move |alert, _, _| {
                                         let entity = entity.clone();
                                         alert
                            .confirm()
                            .title(title.clone())
                            .description(t!("collections.delete_confirm_description").to_string())
                            .on_ok(move |_, _, cx| {
                                entity.update(cx, |ctrl, cx| {
                                    ctrl.delete_collection(col_id, cx);
                                });
                                true
                            })
                                     });
                           }
                       }))
         })
         .into_any_element()
}

/// Renders the full sidebar column using gpui-kit `Sidebar`.
///
/// Library totals and the activity indicator, previously shown in the
/// sidebar footer, now live in the status bar (see `status_bar_view.rs`),
/// per `main-window-status-bar`.
#[allow(clippy::too_many_arguments)]
