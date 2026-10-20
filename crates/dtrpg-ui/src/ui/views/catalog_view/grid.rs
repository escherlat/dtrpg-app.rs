use super::*;

// ── Grid layout
// ───────────────────────────────────────────────────────────────

// `window` is required to measure title-truncation width for the tooltip;
// grouping the remaining rendering context into a struct is left for a
// follow-up since these call sites are shared with the grouped/ungrouped list
// delegates' own parameter lists.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_grid(items: Vec<LibraryItem>, cover_cache: HashMap<Arc<str>, Arc<Image>>,
                          checking_items: HashSet<Arc<str>>,
                          item_activity: HashMap<Arc<str>, ItemActivity>, colors: ColorTokens,
                          density: DensityConstants, entity: Entity<LibraryController>,
                          tabs: Entity<TabsController>, storage_root_path: PathBuf,
                          window: &Window, selection_mode: bool, selected_ids: HashSet<Arc<str>>)
                          -> impl IntoElement + 'static {
    let gap_x = density.card_gap_x;
    let gap_y = density.card_gap_y;
    let min_w = density.card_min_width;

    div().flex()
         .flex_wrap()
         .gap(gap_x)
         .mb(gap_y)
         .children(items.into_iter().map(|item| {
                                        let cover = cover_cache.get(&item.id).cloned();
                                        let is_checking = checking_items.contains(&item.id);
                                        let activity = item_activity.get(&item.id)
                                                                    .copied()
                                                                    .unwrap_or_default();
                                        let is_selected = selected_ids.contains(&item.id);
                                        render_grid_card(&item,
                                                         cover,
                                                         &colors,
                                                         min_w,
                                                         entity.clone(),
                                                         tabs.clone(),
                                                         storage_root_path.clone(),
                                                         window,
                                                         is_checking,
                                                         activity,
                                                         selection_mode,
                                                         is_selected)
                                    }))
}

// See the `render_grid` allow above — `window` is needed for title-truncation
// measurement.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_grid_card(item: &LibraryItem, cover_image: Option<Arc<Image>>,
                               colors: &ColorTokens, card_w: f32,
                               entity: Entity<LibraryController>, tabs: Entity<TabsController>,
                               storage_root_path: PathBuf, window: &Window, is_checking: bool,
                               activity: ItemActivity, selection_mode: bool, selected: bool)
                               -> impl IntoElement + 'static + use<> {
    let id = Arc::clone(&item.id);
    let title = item.title.to_string();
    let publisher = item.publisher.to_string();
    let status = item.status;
    let cover_h = card_w * 10.0 / 7.0;
    // Card has 4px horizontal padding on each side around the title text.
    let title_available_w = px(card_w) - px(8.0);
    let title_truncated = is_title_truncated(window,
                                             &title,
                                             text_xs_size(window),
                                             FontWeight::MEDIUM,
                                             title_available_w);

    let cover: AnyElement = if let Some(image) = cover_image {
        img(image).w(px(card_w))
                  .h(px(cover_h))
                  .object_fit(ObjectFit::Cover)
                  .into_any_element()
    }
    else {
        render_generative_cover(item, card_w, cover_h, true).into_any_element()
    };
    let colors = colors.clone();
    let ctx_id = Arc::clone(&id);
    let ctx_entity = entity.clone();
    let ctx_item = item.clone();
    let ctx_tabs = tabs.clone();
    let bounds_entity = entity.clone();
    let bounds_id = Arc::clone(&id);
    let ctx_path = dtrpg_core::data::storage::publisher_dir(&storage_root_path, &publisher);
    let drag_payload = DraggedLibraryItem { title:      title.clone().into(),
                                            member_id:  collection_member_id(item),
                                            product_id: item.product_id, };

    let checkbox_id = Arc::clone(&id);
    let checkbox_entity = entity.clone();

    let cover_cell: AnyElement = {
        let mut inner = div().relative().child(cover);
        if let Some(badge) = render_item_count_badge(item, &colors) {
            inner = inner.child(div().absolute().bottom(px(2.0)).right(px(2.0)).child(badge));
        }
        if let Some(badge) = render_unavailable_badge(item, &colors) {
            inner = inner.child(div().absolute().top(px(2.0)).left(px(2.0)).child(badge));
        }
        if let Some(spinner) = render_checking_indicator(is_checking) {
            inner = inner.child(div().absolute().top(px(2.0)).right(px(2.0)).child(spinner));
        }
        if selection_mode {
            let checkbox_elem_id: Arc<str> = Arc::from(format!("grid-select-{}", &*checkbox_id));
            inner = inner.child(div().absolute().bottom(px(2.0)).left(px(2.0)).child(
                Checkbox::new(checkbox_elem_id).checked(selected).on_click(
                    move |new_checked, _window, cx| {
                        checkbox_entity.update(cx, |ctrl, cx| {
                            if *new_checked {
                                ctrl.bulk_select_item(Arc::clone(&checkbox_id), cx);
                            } else {
                                ctrl.bulk_deselect_item(&checkbox_id, cx);
                            }
                        });
                    },
                ),
            ));
        }
        if status == ItemStatus::Downloaded {
            Badge::new().dot()
                        .color(if item.is_offloaded() {
                                   colors.warning_text
                               }
                               else {
                                   gpui::green()
                               })
                        .child(inner)
                        .into_any_element()
        }
        else {
            inner.into_any_element()
        }
    };

    let title_tooltip = title.clone();
    let title_el_id: Arc<str> = Arc::from(format!("title-grid-{}", &*id));
    let mut title_el = div().id(title_el_id)
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.text_primary)
                            .truncate()
                            .child(title.clone());
    if title_truncated {
        title_el = title_el.tooltip(move |window, cx| {
                               Tooltip::new(title_tooltip.clone()).build(window, cx)
                           });
    }

    div().id(Arc::clone(&id))
         .w(px(card_w))
         .flex()
         .flex_col()
         .rounded(px(6.0))
         .overflow_hidden()
         .cursor_pointer()
         // Reported continuously (not just while selected) so that if this
         // card is the one the user clicks, its bounds are already known —
         // it was necessarily visible, and therefore already painted, before
         // the click could happen. See `LibraryController::entry_bounds`.
         .on_prepaint(move |bounds, _window, cx| {
             bounds_entity.update(cx, |ctrl, cx| {
                              ctrl.set_entry_bounds(bounds_id.clone(), bounds, cx)
                          });
         })
         .on_click(item_click_handler(Arc::clone(&id), title.clone(), entity, tabs))
         .on_drag(drag_payload, |drag, _, _, cx| {
             cx.stop_propagation();
             cx.new(|_| drag.clone())
         })
         .child(cover_cell)
         .child(div().px(px(4.0))
                     .pt(px(4.0))
                     .pb(px(6.0))
                     .flex()
                     .flex_col()
                     .gap(px(1.0))
                     .child(title_el)
                     .child(div().flex()
                                 .items_center()
                                 .justify_between()
                                 .child(div().text_xs()
                                             .text_color(colors.text_tertiary)
                                             .truncate()
                                             .child(publisher))
                                 .child(render_status(&id,
                                                      status,
                                                      item.is_offloaded(),
                                                      activity,
                                                      &colors))))
         .context_menu(move |menu, window, cx| {
             let menu = match status {
                 ItemStatus::Downloaded => {
                     let open_path = ctx_path.clone();
                     let reveal_path = ctx_path.clone();
                     let remove_id = Arc::clone(&ctx_id);
                     let entity_remove = ctx_entity.clone();
                     let item_for_open = ctx_item.clone();
                     let tabs_for_open = ctx_tabs.clone();
                     let controller_for_open = ctx_entity.clone();
                     menu.item(PopupMenuItem::new(t!("catalog.action_open")).on_click(
                        move |_, _, cx| {
                            open_item_or_focus_detail_tab(
                                &open_path,
                                &item_for_open,
                                &tabs_for_open,
                                &controller_for_open,
                                cx,
                            );
                        },
                    ))
                    .item(
                        PopupMenuItem::new(platform_reveal_label()).on_click(move |_, _, _| {
                            if !reveal_path.exists() {
                                tracing::warn!(
                                    path = %reveal_path.display(),
                                    "reveal: file not found"
                                );
                                return;
                            }
                            if let Err(e) = reveal_in_file_manager(&reveal_path) {
                                tracing::warn!("reveal_in_file_manager failed: {e}");
                            }
                        }),
                    )
                    .item({
                        let remove_title = ctx_item.title.to_string();
                        PopupMenuItem::new(t!("catalog.action_remove_download")).on_click(
                            move |_, window, cx| {
                                let entity_remove = entity_remove.clone();
                                let remove_id = Arc::clone(&remove_id);
                                let remove_title = remove_title.clone();
                                window.open_alert_dialog(cx, move |alert, _, _| {
                                    let entity_remove = entity_remove.clone();
                                    let remove_id = Arc::clone(&remove_id);
                                    alert
                                        .confirm()
                                        .title(
                                            t!(
                                                "catalog.remove_download_confirm_title",
                                                title = remove_title.clone()
                                            )
                                            .to_string(),
                                        )
                                        .description(
                                            t!("catalog.remove_download_confirm_description")
                                                .to_string(),
                                        )
                                        .on_ok(move |_, _window, cx| {
                                            entity_remove.update(cx, |ctrl, cx| {
                                                ctrl.remove_download(&remove_id, cx)
                                            });
                                            true
                                        })
                                });
                            },
                        )
                    })
                 }
                 ItemStatus::Cloud => {
                     let dl_id = Arc::clone(&ctx_id);
                     let dl_title = ctx_item.title.to_string();
                     let entity_dl = ctx_entity.clone();
                     menu.item(PopupMenuItem::new(t!("catalog.action_download")).on_click(
                        move |_, _, cx| {
                            entity_dl.update(cx, |ctrl, cx| {
                                ctrl.request_download(&dl_id, dl_title.clone(), cx)
                            });
                        },
                    ))
                 }
             };
             append_common_menu_items(menu, &ctx_item, &ctx_entity, window, cx)
         })
}
