//! Drag payload for catalog-item-to-collection drag-and-drop.

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, SharedString, Styled, Window,
    div, px,
};
use gpui_component::ActiveTheme;

/// Carried by a catalog item (grid card or thumb row) while it is being
/// dragged onto a sidebar collection.
///
/// `member_id` is the item's `order_product_id` (falling back to
/// `product_id`), matching the id space `CollectionEntry::member_ids` uses —
/// see `LibraryController::add_item_to_collection`. `product_id` is the
/// item's catalog `product_id`, sent as the network add call's product
/// identifier.
#[derive(Clone)]
pub struct DraggedLibraryItem {
    /// Title shown in the drag preview.
    pub title: SharedString,
    /// The id to add to a collection's `member_ids` on drop.
    pub member_id: u64,
    /// The catalog `product_id` sent on the network add call.
    pub product_id: u64,
}

impl Render for DraggedLibraryItem {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("drag-library-item")
            .cursor_grab()
            .py_1()
            .px_2()
            .max_w(px(220.))
            .truncate()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .text_sm()
            .text_color(cx.theme().foreground)
            .child(self.title.clone())
    }
}
