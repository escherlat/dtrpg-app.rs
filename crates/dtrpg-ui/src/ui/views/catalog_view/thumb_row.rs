use super::*;

// ── Thumbnail context menu item
// ─────────────────────────────────────────────────

/// Builds the "Load Thumbnail" right-click menu item. It is disabled when no
/// `cover_url` is present or when the cooldown guard is active.
pub(super) fn thumbnail_menu_item(item: &LibraryItem, entity: &Entity<LibraryController>)
                                  -> PopupMenuItem {
    let can_load = item.cover_url.is_some() && thumbnail_cooldown_elapsed(item);
    let cover_url = item.cover_url.clone();
    let entity = entity.clone();

    PopupMenuItem::new(t!("catalog.action_load_thumbnail")).disabled(!can_load)
                                                           .on_click(move |_, _, cx| {
                                                               if let Some(ref url) = cover_url {
                                                                   entity.update(cx, |ctrl, cx| {
                                                                             ctrl.load_thumbnail(Arc::clone(url), cx)
                                                                         });
                                                               }
                                                           })
}

// ── Reveal action
// ─────────────────────────────────────────────────────────────

pub(super) fn platform_reveal_label() -> std::borrow::Cow<'static, str> {
    match Platform.kind() {
        PlatformKind::MacOs => t!("catalog.action_show_in_finder"),
        PlatformKind::Windows => t!("catalog.action_show_in_explorer"),
        PlatformKind::Linux => t!("catalog.action_show_in_files"),
    }
}

// ── Thumbs layout
// ─────────────────────────────────────────────────────────────

// See the `render_grid` allow above — `window` is needed for title-truncation
// measurement.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_thumb_row(item: &LibraryItem, cover_image: Option<Arc<Image>>,
                               colors: &ColorTokens, density: &DensityConstants,
                               entity: Entity<LibraryController>, tabs: Entity<TabsController>,
                               storage_root_path: PathBuf, window: &Window, is_checking: bool,
                               activity: ItemActivity, selection_mode: bool, selected: bool)
                               -> impl IntoElement + 'static + use<> {
    let id = Arc::clone(&item.id);
    let title = item.title.to_string();
    let publisher = item.publisher.to_string();
    let line = item.line.to_string();
    let kind = item.kind.to_string();
    let pages = item.pages;
    let size_mb = item.size_mb;
    let year = item.year;
    let status = item.status;
    let thumb_w = density.thumb_width;
    let thumb_h = thumb_w * 10.0 / 7.0;
    let row_h = density.thumb_row_height;
    let colors = colors.clone();
    let ctx_id = Arc::clone(&id);
    let ctx_entity = entity.clone();
    let ctx_item = item.clone();
    let ctx_tabs = tabs.clone();
    let bounds_entity = entity.clone();
    let bounds_id = Arc::clone(&id);
    let drag_payload = DraggedLibraryItem { title:      title.clone().into(),
                                            member_id:  collection_member_id(item),
                                            product_id: item.product_id, };
    // Approximation: the title column fills the remaining row width after the
    // cover and gap, which depends on the actual panel layout
    // (sidebar/detail panel state) that isn't known synchronously here.
    // `window.viewport_size()` minus the catalog's own side padding, cover
    // width, and gap is used as a best-effort proxy; this may under-detect
    // truncation when side panels are open and the row is narrower than
    // this estimate assumes.
    let title_available_w =
        window.viewport_size().width - (density.catalog_pad_side * 2.0) - px(thumb_w) - px(12.0);
    let title_truncated = is_title_truncated(window,
                                             &title,
                                             text_sm_size(window),
                                             FontWeight::MEDIUM,
                                             title_available_w);
    let ctx_path = dtrpg_core::data::storage::publisher_dir(&storage_root_path, &publisher);

    let cover: AnyElement = if let Some(image) = cover_image {
        img(image).w(px(thumb_w))
                  .h(px(thumb_h))
                  .object_fit(ObjectFit::Cover)
                  .into_any_element()
    }
    else {
        render_generative_cover(item, thumb_w, thumb_h, false).into_any_element()
    };

    let checkbox_id = Arc::clone(&id);
    let checkbox_entity = entity.clone();

    let cover_cell: AnyElement = {
        let mut inner = div().relative()
                             .rounded(px(3.0))
                             .overflow_hidden()
                             .flex_none()
                             .child(cover);
        if selection_mode {
            let checkbox_elem_id: Arc<str> = Arc::from(format!("thumb-select-{}", &*checkbox_id));
            inner = inner.child(div().absolute().top(px(2.0)).left(px(2.0)).child(
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
        if let Some(badge) = render_item_count_badge(item, &colors) {
            inner = inner.child(div().absolute().bottom(px(2.0)).right(px(2.0)).child(badge));
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

    div().id(Arc::clone(&id))
         .w_full()
         .flex()
         .items_center()
         .gap(px(12.0))
         .h(row_h)
         .border_b_1()
         .border_color(colors.border)
         .cursor_pointer()
         // Reported continuously (not just while selected) so that if this
         // row is the one the user clicks, its bounds are already known — it
         // was necessarily visible, and therefore already painted, before
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
         .child({
             let title_tooltip = title.clone();
             let title_el_id: Arc<str> = Arc::from(format!("title-thumb-{}", &*id));
             let mut title_el = div().id(title_el_id)
                                     .text_sm()
                                     .font_weight(FontWeight::MEDIUM)
                                     .text_color(colors.text_primary)
                                     .truncate()
                                     .child(title);
             if title_truncated {
                 title_el = title_el.tooltip(move |window, cx| {
                                        Tooltip::new(title_tooltip.clone()).build(window, cx)
                                    });
             }
             div().flex_1()
                  .min_w_0()
                  .flex()
                  .flex_col()
                  .justify_center()
                  .gap(px(2.0))
                  .child(title_el)
                  .child(div().text_xs()
                              .text_color(colors.text_secondary)
                              .truncate()
                              .child(format!("{publisher} · {line}")))
                  .child(div().flex()
                              .items_center()
                              .gap(px(8.0))
                              .child(div().text_xs().text_color(colors.text_tertiary).child(kind))
                              .child(div().text_xs()
                                          .text_color(colors.text_tertiary)
                                          .child(format!("{pages} pp · {size_mb:.0} MB · {year}")))
                              .children(render_unavailable_badge(item, &colors))
                              .children(render_checking_indicator(is_checking)))
         })
         .child(render_status(&id, status, item.is_offloaded(), activity, &colors))
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

// ── Shared context menu items
// ───────────────────────────────────────────────────

/// Appends the actions every catalog item context menu offers regardless of
/// download status: "Load Thumbnail" and "Manage Collections…".
///
/// Shared by all four catalog item context menu sites (ungrouped/grouped
/// list-layout table delegates, and the thumb/grid row builders) so these
/// actions aren't duplicated four times over.
pub(super) fn append_common_menu_items(menu: PopupMenu, item: &LibraryItem,
                                       entity: &Entity<LibraryController>, _window: &mut Window,
                                       _cx: &mut App)
                                       -> PopupMenu {
    let member_id = collection_member_id(item);
    let product_id = item.product_id;
    let entity = entity.clone();
    let item_title = Arc::clone(&item.title);
    menu.item(thumbnail_menu_item(item, &entity)).item(
        PopupMenuItem::new(t!("catalog.action_manage_collections")).on_click(
            move |_, window, cx| {
                open_manage_collections_dialog(
                    window,
                    cx,
                    entity.clone(),
                    Arc::clone(&item_title),
                    member_id,
                    product_id,
                );
            },
        ),
    )
}
