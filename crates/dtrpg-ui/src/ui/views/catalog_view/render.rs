//! The catalog view's render tree: list/grouped-list/thumbs/grid layouts
//! composed from the controller's snapshot.

use super::*;

fn popover_anchor_point(entry: Bounds<Pixels>, popover_width: Pixels, margin: Pixels,
                        window_width: Pixels)
                        -> Point<Pixels> {
    let right_x = entry.origin.x + entry.size.width + margin;
    let x = if right_x + popover_width <= window_width {
        right_x
    }
    else {
        (entry.origin.x - margin - popover_width).max(px(0.0))
    };
    Point { x,
            y: entry.origin.y }
}

pub(super) fn render_catalog_view(view: &mut CatalogView, window: &mut Window,
                                  cx: &mut Context<CatalogView>)
                                  -> impl IntoElement {
    let snap = view.controller.read(cx).snapshot();
    let item_count = view.controller.read(cx).visible_items_count();
    let theme = cx.global::<LibriTheme>().clone();
    let colors = theme.colors;
    let density = theme.density_constants;
    let storage_root = view.settings.read(cx).snapshot().storage_root_path;

    let pad_top = density.catalog_pad_top;
    let pad_side = density.catalog_pad_side;
    let pad_bottom = density.catalog_pad_bottom;
    // List presentation renders its own skeleton loading view via the
    // `TableDelegate::loading()` hook, so it must fall through to the
    // content match below instead of the shared empty/loading short
    // circuits used by the Thumbs/Grid presentations.
    let is_list_presentation = matches!(snap.presentation, CatalogPresentation::List);
    let empty_reason = if item_count == 0 && !(is_list_presentation && snap.catalog_loading) {
        Some(if snap.total_count == 0 {
                 EmptyReason::LibraryEmpty
             }
             else {
                 EmptyReason::NoMatches
             })
    }
    else {
        None
    };

    // Update items_per_row estimate for grid layout using the viewport
    // width. Subtract a rough sidebar width (220px) and both side
    // pads.
    let viewport_w = window.viewport_size().width.as_f32();
    let usable_w = (viewport_w - 220.0 - pad_side.as_f32() * 2.0).max(0.0);
    let card_pitch = density.card_min_width + density.card_gap_x.as_f32();
    view.items_per_row = ((usable_w / card_pitch) as usize).max(1);

    let items_per_row = view.items_per_row;
    let scroll_handle = view.scroll_handle.clone();
    let ctrl = view.controller.clone();
    let tabs_entity = view.tabs.clone();

    let outer = div().flex_1().min_h_0().flex().flex_col();

    // Non-virtualized branches (grouped Thumbs/Grid, empty/loading states)
    // size their content to its natural height and need `root`
    // itself to scroll. Virtualized branches (uniform_list-backed
    // Thumbs/Grid, and DataTable-backed List) manage their own
    // scrolling and scrollbar via `scroll_handle` / internally, so
    // wrapping them in a second, unrelated scroll container here
    // would leave the outer scrollbar tracking a handle that never reflects
    // the true (virtualized) content size — the bug behind a
    // missing grid scrollbar.
    let root = div().flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .pt(pad_top)
                    .pb(pad_bottom);

    if snap.catalog_loading && item_count == 0 && !is_list_presentation {
        return outer.child(root.justify_center()
                               .items_center()
                               .child(Spinner::new().with_size(Size::Large)))
                    .into_any_element();
    }

    if let Some(reason) = empty_reason {
        let empty_state = match reason {
            EmptyReason::LibraryEmpty => {
                render_library_empty_state(colors.text_tertiary).into_any_element()
            }
            EmptyReason::NoMatches => {
                render_no_matches_state(&snap.search_query, colors.text_tertiary).into_any_element()
            }
        };
        return outer.child(root.child(empty_state)).into_any_element();
    }

    let group_by = snap.group_by;
    let content: AnyElement = match (snap.presentation, group_by.is_grouped()) {
        // ── List, ungrouped — DataTable (handles header/row alignment) ──
        (CatalogPresentation::List, false) => {
            use gpui_kit::component::Size;
            root.px(pad_side)
                    .child(
                        DataTable::new(&view.catalog_list_table)
                            .with_size(Size::Size(density.row_text_height))
                            .bordered(false)
                            .scrollbar_visible(true, false),
                    )
                    .into_any_element()
        }

        // ── List, grouped — DataTable over a flattened header/item row
        // list, so it gets the same virtualization as the ungrouped
        // branch instead of hand-rolled, fully-materialized rows ──
        (CatalogPresentation::List, true) => {
            use gpui_kit::component::Size;
            view.grouped_items(cx);
            root.px(pad_side)
                    .child(
                        DataTable::new(&view.catalog_grouped_list_table)
                            .with_size(Size::Size(density.row_text_height))
                            .bordered(false)
                            .scrollbar_visible(true, false),
                    )
                    .into_any_element()
        }

        // ── Thumbs, ungrouped — virtualized ───────────────────────────
        (CatalogPresentation::Thumbs, false) => {
            let c = colors.clone();
            let d = density.clone();
            let s = storage_root.clone();
            let t = tabs_entity.clone();
            let selection_mode = snap.bulk_selection_mode;
            let selected_ids = snap.bulk_selected_ids.clone();
            root.px(pad_side)
                .child(
                       div().relative()
                            .flex_1()
                            .min_h_0()
                            .child(
                uniform_list("catalog-thumbs", item_count, move |range, window, cx| {
                    let items = ctrl.read(cx).visible_items_slice(range);
                    let covers: Vec<Option<Arc<Image>>> = {
                        let cache = cx.global::<CoverCache>();
                        items.iter().map(|item| cache.get(&item.id)).collect()
                    };
                    let checking: Vec<(bool, ItemActivity)> = {
                        let ctrl_ref = ctrl.read(cx);
                        items.iter()
                             .map(|item| {
                                 (ctrl_ref.is_checking(&item.id), ctrl_ref.item_activity(&item.id))
                             })
                             .collect()
                    };
                    items.iter()
                         .zip(covers)
                         .zip(checking)
                         .map(|((item, cover), (is_checking, activity))| {
                             let is_selected = selected_ids.contains(&item.id);
                             render_thumb_row(item,
                                              cover,
                                              &c,
                                              &d,
                                              ctrl.clone(),
                                              t.clone(),
                                              s.clone(),
                                              window,
                                              is_checking,
                                              activity,
                                              selection_mode,
                                              is_selected).into_any_element()
                         })
                         .collect()
                }).track_scroll(&scroll_handle)
                  .size_full(),
            )
                            .vertical_scrollbar(&scroll_handle),
            )
                .into_any_element()
        }

        // ── Thumbs, grouped — non-virtualized ─────────────────────────
        (CatalogPresentation::Thumbs, true) => {
            let groups = view.grouped_items(cx);
            let cover_cache = {
                let cache = cx.global::<CoverCache>();
                cache.images.clone()
            };
            let checking_items = view.controller.read(cx).checking_items_snapshot();
            let item_activity = view.controller.read(cx).item_activity_snapshot();
            let selection_mode = snap.bulk_selection_mode;
            let selected_ids = snap.bulk_selected_ids.clone();
            root.overflow_y_scrollbar()
                    .px(pad_side)
                    .children(groups.into_iter().map(|g| {
                        let c = colors.clone();
                        let d = density.clone();
                        let e = view.controller.clone();
                        let t = tabs_entity.clone();
                        let s = storage_root.clone();
                        let cc = cover_cache.clone();
                        let checking_items = checking_items.clone();
                        let item_activity = item_activity.clone();
                        let selected_ids = selected_ids.clone();
                        // Reborrow as `&Window` (Copy) so the nested
                        // `move` closure can capture it on every
                        // outer iteration without moving `window`.
                        let window: &Window = &*window;
                        div()
                            .child(render_group_header(
                                &g.title,
                                group_by,
                                g.items.len(),
                                &c,
                                e.clone(),
                            ))
                            .children(g.items.into_iter().map(move |item| {
                                let cover = cc.get(&item.id).cloned();
                                let is_checking = checking_items.contains(&item.id);
                                let activity = item_activity.get(&item.id).copied().unwrap_or_default();
                                let is_selected = selected_ids.contains(&item.id);
                                render_thumb_row(
                                    &item,
                                    cover,
                                    &c,
                                    &d,
                                    e.clone(),
                                    t.clone(),
                                    s.clone(),
                                    window,
                                    is_checking,
                                    activity,
                                    selection_mode,
                                    is_selected,
                                )
                            }))
                    }))
                    .into_any_element()
        }

        // ── Grid, ungrouped — row-virtualized ─────────────────────────
        (CatalogPresentation::Grid, false) => {
            let row_count = item_count.div_ceil(items_per_row);
            let c = colors.clone();
            let d = density.clone();
            let s = storage_root.clone();
            let t = tabs_entity.clone();
            let selection_mode = snap.bulk_selection_mode;
            let selected_ids = snap.bulk_selected_ids.clone();
            root.px(pad_side)
                    .child(
                           div().relative()
                                .flex_1()
                                .min_h_0()
                                .child(
                    uniform_list("catalog-grid", row_count, move |row_range, window, cx| {
                        let range_start = row_range.start;
                        let item_start = range_start * items_per_row;
                        let item_end = (row_range.end * items_per_row).min(item_count);
                        let items = ctrl.read(cx).visible_items_slice(item_start..item_end);
                        let covers: Vec<Option<Arc<Image>>> = {
                            let cache = cx.global::<CoverCache>();
                            items.iter().map(|item| cache.get(&item.id)).collect()
                        };
                        let checking: Vec<(bool, ItemActivity)> = {
                            let ctrl_ref = ctrl.read(cx);
                            items.iter()
                                 .map(|item| {
                                     (ctrl_ref.is_checking(&item.id), ctrl_ref.item_activity(&item.id))
                                 })
                                 .collect()
                        };
                        row_range.map(|row| {
                                     let offset = (row - range_start) * items_per_row;
                                     let row_end = (offset + items_per_row).min(items.len());
                                     let row_items = &items[offset..row_end];
                                     let row_covers = &covers[offset..row_end];
                                     let row_checking = &checking[offset..row_end];
                                     div()
                                                    .flex()
                                                    .gap(d.card_gap_x)
                                                    .mb(d.card_gap_y)
                                                    .children(
                                                        row_items
                                                            .iter()
                                                            .zip(row_covers.iter())
                                                            .zip(row_checking.iter())
                                                            .map(
                                                                |(
                                                                    (item, cover),
                                                                    &(is_checking, activity),
                                                                )| {
                                                                    let is_selected = selected_ids
                                                                        .contains(&item.id);
                                                                    render_grid_card(
                                                                        item,
                                                                        cover.clone(),
                                                                        &c,
                                                                        d.card_min_width,
                                                                        ctrl.clone(),
                                                                        t.clone(),
                                                                        s.clone(),
                                                                        window,
                                                                        is_checking,
                                                                        activity,
                                                                        selection_mode,
                                                                        is_selected,
                                                                    )
                                                                },
                                                            ),
                                                    )
                                                    .into_any_element()
                                 })
                                 .collect()
                    }).track_scroll(&scroll_handle)
                      .size_full(),
                )
                                .vertical_scrollbar(&scroll_handle),
                )
                    .into_any_element()
        }

        // ── Grid, grouped — non-virtualized ───────────────────────────
        (CatalogPresentation::Grid, true) => {
            let groups = view.grouped_items(cx);
            let cover_cache = {
                let cache = cx.global::<CoverCache>();
                cache.images.clone()
            };
            let checking_items = view.controller.read(cx).checking_items_snapshot();
            let item_activity = view.controller.read(cx).item_activity_snapshot();
            let selection_mode = snap.bulk_selection_mode;
            let selected_ids = snap.bulk_selected_ids.clone();
            root.overflow_y_scrollbar()
                .px(pad_side)
                .children(groups.into_iter().map(|g| {
                                                let c = colors.clone();
                                                let d = density.clone();
                                                let e = view.controller.clone();
                                                let t = tabs_entity.clone();
                                                let s = storage_root.clone();
                                                let cc = cover_cache.clone();
                                                let ci = checking_items.clone();
                                                let vd = item_activity.clone();
                                                let sel = selected_ids.clone();
                                                div().child(render_group_header(&g.title,
                                                                                group_by,
                                                                                g.items.len(),
                                                                                &c,
                                                                                e.clone()))
                                                     .child(render_grid(g.items,
                                                                        cc,
                                                                        ci,
                                                                        vd,
                                                                        c,
                                                                        d,
                                                                        e,
                                                                        t,
                                                                        s,
                                                                        &*window,
                                                                        selection_mode,
                                                                        sel))
                                            }))
                .into_any_element()
        }
    };

    let mut result = outer.relative()
                          .on_mouse_down(MouseButton::Left,
                                         cx.listener(|this, event: &MouseDownEvent, _window, _cx| {
                                               this.popover_anchor_pos = Some(event.position);
                                           }))
                          .child(content);

    if let Some(item) = snap.selected_item.as_ref() {
        let entry_bounds = view.controller
                               .read(cx)
                               .entry_bounds(&item.id)
                               .unwrap_or(Bounds { origin: view.popover_anchor_pos
                                                               .unwrap_or_default(),
                                                   size:   gpui::Size::default(), });
        let anchor = popover_anchor_point(entry_bounds,
                                          px(ITEM_POPOVER_WIDTH),
                                          px(ITEM_POPOVER_MARGIN),
                                          window.viewport_size().width);
        let is_checking = view.controller.read(cx).is_checking(&item.id);
        let is_verifying = view.controller.read(cx).is_verifying_downloads(&item.id);
        result = result.child(render_item_popover(item,
                                                  anchor,
                                                  view.controller.clone(),
                                                  tabs_entity.clone(),
                                                  &colors,
                                                  is_checking,
                                                  is_verifying));
    }

    result.into_any_element()
}
