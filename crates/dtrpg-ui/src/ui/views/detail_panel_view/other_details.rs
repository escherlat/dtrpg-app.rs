use super::*;

/// Renders the "Other details" disclosure section: a clickable header that
/// toggles a collapsed-by-default panel of fields not already shown in the
/// primary metadata table or item tier (stable id, numeric id, order product
/// id, product id, added-order value, and the generative cover color).
///
/// See `catalog-entry-detail-advanced-disclosure`.
pub(super) fn render_other_details(item: &LibraryItem, entity: Entity<LibraryController>,
                                   colors: &ColorTokens, cx: &App)
                                   -> impl IntoElement + 'static {
    let entry_id = Arc::clone(&item.id);
    let open = entity.read(cx).is_other_details_open(&entry_id);
    let label_font_family = cx.global::<LibriTheme>().fonts.label_font.clone();
    let value_font_family = cx.global::<LibriTheme>().fonts.value_font.clone();

    let toggle_entity = entity;
    let toggle_entry_id = Arc::clone(&entry_id);

    let header = div().id("other-details-header")
                      .flex()
                      .items_center()
                      .gap(px(6.0))
                      .cursor_pointer()
                      .text_sm()
                      .font_weight(gpui::FontWeight::SEMIBOLD)
                      .text_color(colors.text_primary)
                      .on_click(move |_, _, cx| {
                          toggle_entity.update(cx, |ctrl, cx| {
                                           ctrl.toggle_other_details(Arc::clone(&toggle_entry_id),
                                                                     cx);
                                       });
                      })
                      .child(Icon::new(if open {
                                           IconName::ChevronUp
                                       }
                                       else {
                                           IconName::ChevronDown
                                       }).text_color(colors.text_secondary))
                      .child(t!("detail.other_details_heading").to_string());

    let swatch_color = cover_style(item).background;
    let content = DescriptionList::vertical()
        .columns(2)
        .bordered(false)
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_stable_id").to_string(),
                &label_font_family,
            ))
            .value(copyable_value(
                SharedString::from("other-details-stable-id"),
                item.id.to_string(),
                &value_font_family,
            )),
        )
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_numeric_id").to_string(),
                &label_font_family,
            ))
            .value(copyable_value(
                SharedString::from("other-details-numeric-id"),
                item.numeric_id.to_string(),
                &value_font_family,
            )),
        )
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_order_product_id").to_string(),
                &label_font_family,
            ))
            .value(copyable_value(
                SharedString::from("other-details-order-product-id"),
                item.order_product_id.to_string(),
                &value_font_family,
            )),
        )
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_product_id").to_string(),
                &label_font_family,
            ))
            .value(copyable_value(
                SharedString::from("other-details-product-id"),
                item.product_id.to_string(),
                &value_font_family,
            )),
        )
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_added_order").to_string(),
                &label_font_family,
            ))
            .value(styled_value(
                item.added_order.to_string(),
                &value_font_family,
            )),
        )
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_cover_color").to_string(),
                &label_font_family,
            ))
            .value(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().size(px(12.0)).rounded_full().bg(swatch_color))
                    .child(copyable_value(
                        SharedString::from("other-details-cover-color"),
                        item.color.to_string(),
                        &value_font_family,
                    ))
                    .into_any_element(),
            )
            .span(2),
        );

    Collapsible::new().gap(px(8.0))
                      .open(open)
                      .child(header)
                      .content(content)
}
/// Renders a summary of the collections this entry currently belongs to, with a
/// "Manage…" button opening the Manage Collections dialog scoped to this entry.
///
/// See `detail-view-collection-membership`.
pub(super) fn render_collections_section(item: &LibraryItem, entity: Entity<LibraryController>,
                                         colors: &ColorTokens, cx: &App)
                                         -> impl IntoElement + 'static {
    let member_id = collection_member_id(item);
    let product_id = item.product_id;
    let member_names: Vec<Arc<str>> =
        entity.read(cx)
              .collections
              .iter()
              .filter(|c| member_ids_contain(&c.member_ids, member_id, product_id))
              .map(|c| Arc::clone(&c.name))
              .collect();

    let summary: AnyElement = if member_names.is_empty() {
        div().text_sm()
             .text_color(colors.text_tertiary)
             .child(t!("detail.collections_empty").to_string())
             .into_any_element()
    }
    else {
        selectable_text("detail-tab-collections-summary",
                        member_names.iter()
                                    .map(|n| n.to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ")).text_sm()
                                                .text_color(colors.text_secondary)
                                                .into_any_element()
    };

    let item_title = Arc::clone(&item.title);

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(colors.text_primary)
                .child(t!("detail.collections_heading").to_string()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(summary)
                .child(
                    Button::new("detail-tab-manage-collections")
                        .ghost()
                        .outline()
                        .icon(IconName::Settings)
                        .tooltip(t!("detail.collections_manage_button"))
                        .on_click(move |_, window, cx| {
                            open_manage_collections_dialog(
                                window,
                                cx,
                                entity.clone(),
                                Arc::clone(&item_title),
                                member_id,
                                product_id,
                            );
                        }),
                ),
        )
}
