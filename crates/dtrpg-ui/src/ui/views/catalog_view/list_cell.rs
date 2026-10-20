use super::*;

/// Builds a catalog item's `on_click` handler: a single click opens the item
/// popover; a double click additionally opens (or activates) an expanded
/// detail tab and dismisses the popover. Mirrors the click-count escalation
/// pattern `gpui-kit`'s own `TableState::on_row_left_click` uses.
pub(super) fn item_click_handler(id: Arc<str>, title: String, entity: Entity<LibraryController>,
                                 tabs: Entity<TabsController>)
                                 -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |event, _window, cx| {
        let count = event.click_count();
        entity.update(cx, |ctrl, cx| ctrl.select_item(Arc::clone(&id), cx));
        if count >= 2 {
            tabs.update(cx, |t, cx| {
                    t.open_detail_tab(Arc::clone(&id), title.clone(), cx);
                });
            entity.update(cx, |ctrl, cx| {
                      ctrl.clear_selection(cx);
                      // Reopening a detail tab must show no pre-selected item
                      // (selection is ephemeral, see
                      // `catalog-entry-detail-view`).
                      ctrl.clear_item_selection(&id, cx);
                      ctrl.ensure_detail_cover(&id, cx);
                  });
        }
    }
}

/// Opens a catalog entry's downloaded content. If the entry bundles more
/// than one file, opens (or focuses) its expanded detail tab and clears any
/// stale item selection instead of guessing which file to open — see
/// `catalog-entry-detail-view`'s requirement that multi-file opens route to
/// the persistent item list rather than a dead-end warning log.
pub(crate) fn open_item_or_focus_detail_tab(entry_dir: &Path, item: &LibraryItem,
                                            tabs: &Entity<TabsController>,
                                            controller: &Entity<LibraryController>, cx: &mut App) {
    use crate::util::item_opener::{ItemOpener, OpenError};

    match ItemOpener::open_item(entry_dir, &item.files) {
        Ok(()) => {}
        Err(OpenError::MultipleFilesRequireSelection) => {
            let id = Arc::clone(&item.id);
            let title = item.title.to_string();
            tabs.update(cx, |t, cx| t.open_detail_tab(Arc::clone(&id), title, cx));
            controller.update(cx, |ctrl, cx| {
                          ctrl.clear_item_selection(&id, cx);
                          ctrl.maybe_check_item(Arc::clone(&id), cx);
                          ctrl.ensure_detail_cover(&id, cx);
                      });
        }
        Err(OpenError::FileNotFound(path)) => {
            tracing::warn!("open: file not found: {path}");
        }
        Err(OpenError::NoDefaultApp) => {
            tracing::warn!("open: no default application configured");
        }
        Err(OpenError::OsFailed(msg)) => {
            tracing::warn!("open: OS failed: {msg}");
        }
    }
}

/// Returns the rendered pixel width of `text` shaped at the given
/// `font_size`/`font_weight`.
///
/// The shaped color is irrelevant to width measurement, so a default `Hsla` is
/// used.
pub(super) fn shaped_text_width(window: &Window, text: &str, font_size: Pixels,
                                font_weight: FontWeight)
                                -> Pixels {
    let mut font = window.text_style().font();
    font.weight = font_weight;
    let run = TextRun { len: text.len(),
                        font,
                        color: Hsla::default(),
                        background_color: None,
                        underline: None,
                        strikethrough: None };
    window.text_system()
          .shape_line(text.to_string().into(), font_size, &[run], None)
          .width()
}

/// Returns whether `text`, shaped at the given `font_size`/`font_weight`, is
/// wider than `available_width` — i.e. whether gpui's `.truncate()` would
/// ellipsize it when rendered in a slot of that width.
pub(super) fn is_title_truncated(window: &Window, title: &str, font_size: Pixels,
                                 font_weight: FontWeight, available_width: Pixels)
                                 -> bool {
    shaped_text_width(window, title, font_size, font_weight) > available_width
}

/// Font size for `.text_sm()` styled title text, resolved against the window's
/// rem size.
pub(super) fn text_sm_size(window: &Window) -> Pixels {
    rems(0.875).to_pixels(window.rem_size())
}

/// Font size for `.text_xs()` styled title text, resolved against the window's
/// rem size.
pub(super) fn text_xs_size(window: &Window) -> Pixels {
    rems(0.75).to_pixels(window.rem_size())
}

/// Returns a 2–3 character badge abbreviation for an item kind string.
pub(super) fn kind_badge(kind: &str) -> &'static str {
    if kind.contains("Core") {
        "CR"
    }
    else if kind.contains("Supplement") {
        "SUP"
    }
    else if kind.contains("Adventure") {
        "ADV"
    }
    else if kind.contains("Map") {
        "MAP"
    }
    else if kind.contains("Token") {
        "TOK"
    }
    else if kind.contains("Bundle") || kind.contains("PDF") {
        "PDF"
    }
    else {
        "OTH"
    }
}

/// Renders a small item-count badge for multi-item catalog entries, shown on
/// list rows and grid tiles so users can identify bundled entries before
/// opening the detail view (see `catalog-entry-detail-view`). Returns
/// `None` for single-item entries — callers should render nothing.
pub(super) fn render_item_count_badge(item: &LibraryItem, colors: &ColorTokens)
                                      -> Option<impl IntoElement + 'static + use<>> {
    if !item.is_multi_item() {
        return None;
    }
    let count = item.files.len();
    let badge_id: Arc<str> = Arc::from(format!("item-count-badge-{}", &*item.id));
    let tooltip_text = t!("detail.item_count_badge_tooltip", count = count).to_string();
    Some(div().id(badge_id)
              .flex_none()
              .text_xs()
              .font_weight(FontWeight::SEMIBOLD)
              .text_color(colors.accent_on)
              .px(px(4.0))
              .py(px(1.0))
              .rounded(px(3.0))
              .bg(colors.accent)
              .tooltip(move |window, cx| Tooltip::new(tooltip_text.clone()).build(window, cx))
              .child(count.to_string()))
}

/// Renders a small "unavailable" badge for catalog items no longer returned
/// by the server (`is_available == false`, see `catalog-availability-flag`),
/// following the same small-pill visual pattern as
/// [`render_item_count_badge`]. Returns `None` for available items — callers
/// should render nothing.
pub(super) fn render_unavailable_badge(item: &LibraryItem, colors: &ColorTokens)
                                       -> Option<impl IntoElement + 'static + use<>> {
    if item.is_available {
        return None;
    }
    let badge_id: Arc<str> = Arc::from(format!("unavailable-badge-{}", &*item.id));
    let tooltip_text = t!("catalog.unavailable_badge_tooltip").to_string();
    Some(div().id(badge_id)
              .flex_none()
              .text_xs()
              .font_weight(FontWeight::SEMIBOLD)
              .text_color(colors.warning_text)
              .px(px(4.0))
              .py(px(1.0))
              .rounded(px(3.0))
              .bg(colors.warning_bg)
              .tooltip(move |window, cx| Tooltip::new(tooltip_text.clone()).build(window, cx))
              .child(t!("catalog.unavailable_badge").to_string()))
}

/// Renders a small spinner overlay for a catalog entry with an availability
/// check currently in flight (`LibraryController::is_checking`, see
/// `catalog-item-level-reconciliation`). Returns `None` when no check is in
/// flight — callers should render nothing.
pub(crate) fn render_checking_indicator(is_checking: bool)
                                        -> Option<impl IntoElement + 'static + use<>> {
    if !is_checking {
        return None;
    }
    Some(Spinner::new().with_size(Size::XSmall))
}

/// Renders a single data cell (column `col_ix`) for `item`. Shared between
/// the ungrouped `CatalogListDelegate` and the grouped
/// `GroupedCatalogListDelegate` so grouped and ungrouped list rows stay
/// visually identical and both benefit from `DataTable`'s virtualization.
// Matches the existing arity of this file's other render_* row functions
// (`render_thumb_row`, `render_grid_card`), which are all-render-context
// parameter lists rather than a natural grouping into a smaller struct.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_list_item_cell(item: &LibraryItem, column: ListColumn,
                                    tag_names: &[Arc<str>], colors: &ColorTokens,
                                    storage_root: &Path, controller: &Entity<LibraryController>,
                                    tabs: &Entity<TabsController>, column_width: Pixels,
                                    window: &Window, is_checking: bool, activity: ItemActivity,
                                    selection_mode: bool, selected: bool)
                                    -> AnyElement {
    match column {
        ListColumn::Title => {
            let title = item.title.to_string();
            let badge = kind_badge(&item.kind);
            let badge_w =
                shaped_text_width(window, badge, text_xs_size(window), FontWeight::default());
            // Container gap (6px) + badge horizontal padding (4px each side).
            let title_available_w = column_width - px(6.0) - badge_w - px(8.0);
            let title_truncated = is_title_truncated(window,
                                                     &title,
                                                     text_sm_size(window),
                                                     FontWeight::default(),
                                                     title_available_w);
            let title_el_id: Arc<str> = Arc::from(format!("title-list-{}", &*item.id));
            let mut title_el = div().id(title_el_id)
                                    .text_sm()
                                    .text_color(colors.text_primary)
                                    .truncate()
                                    .child(title.clone());
            if title_truncated {
                title_el = title_el.tooltip(move |window, cx| {
                                       Tooltip::new(title.clone()).build(window, cx)
                                   });
            }
            let checkbox: Option<AnyElement> = if selection_mode {
                let checkbox_id = Arc::clone(&item.id);
                let checkbox_entity = controller.clone();
                let checkbox_elem_id: Arc<str> = Arc::from(format!("list-select-{}", &*item.id));
                Some(Checkbox::new(checkbox_elem_id).checked(selected)
                                                    .on_click(move |new_checked, _window, cx| {
                                                        checkbox_entity.update(cx, |ctrl, cx| {
                                if *new_checked {
                                    ctrl.bulk_select_item(Arc::clone(&checkbox_id), cx);
                                } else {
                                    ctrl.bulk_deselect_item(&checkbox_id, cx);
                                }
                            });
                                                    })
                                                    .into_any_element())
            }
            else {
                None
            };
            div().h_full()
                 .flex()
                 .items_center()
                 .gap(px(6.0))
                 .min_w_0()
                 .children(checkbox)
                 .child(title_el)
                 .child(div().flex_none()
                             .text_xs()
                             .text_color(colors.text_tertiary)
                             .px(px(4.0))
                             .py(px(1.0))
                             .rounded(px(3.0))
                             .bg(colors.hover)
                             .child(badge))
                 .children(render_item_count_badge(item, colors))
                 .children(render_unavailable_badge(item, colors))
                 .children(render_checking_indicator(is_checking))
                 .into_any_element()
        }

        ListColumn::Publisher => div().h_full()
                                      .flex()
                                      .items_center()
                                      .text_sm()
                                      .text_color(colors.text_secondary)
                                      .truncate()
                                      .child(item.publisher.to_string())
                                      .into_any_element(),

        ListColumn::System => div().h_full()
                                   .flex()
                                   .items_center()
                                   .text_sm()
                                   .text_color(colors.text_secondary)
                                   .truncate()
                                   .child(item.line.to_string())
                                   .into_any_element(),

        ListColumn::Pages => div().h_full()
                                  .flex()
                                  .items_center()
                                  .text_sm()
                                  .text_color(colors.text_secondary)
                                  .child(item.pages.to_string())
                                  .into_any_element(),

        ListColumn::Size => div().h_full()
                                 .flex()
                                 .items_center()
                                 .text_sm()
                                 .text_color(colors.text_secondary)
                                 .child(format!("{:.0} MB", item.size_mb))
                                 .into_any_element(),

        ListColumn::Added => div().h_full()
                                  .flex()
                                  .items_center()
                                  .text_sm()
                                  .text_color(colors.text_secondary)
                                  .child(item.year.to_string())
                                  .into_any_element(),

        ListColumn::Tags => div().h_full()
                                 .flex()
                                 .items_center()
                                 .text_sm()
                                 .text_color(colors.text_secondary)
                                 .truncate()
                                 .child(tag_names.iter()
                                                 .map(|name| name.as_ref())
                                                 .collect::<Vec<_>>()
                                                 .join(", "))
                                 .into_any_element(),

        ListColumn::Status => {
            div().h_full()
                 .flex()
                 .items_center()
                 .justify_center()
                 .child(render_status(&item.id, item.status, item.is_offloaded(), activity, colors))
                 .into_any_element()
        }

        ListColumn::Open => {
            if item.status == ItemStatus::Downloaded {
                let item_open_path =
                    dtrpg_core::data::storage::publisher_dir(storage_root, &item.publisher);
                let open_elem_id: Arc<str> = Arc::from(format!("open-row-{}", &*item.id));
                let item_for_open = item.clone();
                let tabs_for_open = tabs.clone();
                let controller_for_open = controller.clone();
                div().id(open_elem_id)
                     .h_full()
                     .flex()
                     .items_center()
                     .justify_center()
                     .cursor_pointer()
                     .on_click(move |_, _, cx| {
                         open_item_or_focus_detail_tab(&item_open_path,
                                                       &item_for_open,
                                                       &tabs_for_open,
                                                       &controller_for_open,
                                                       cx);
                     })
                     .child(div().text_xs().text_color(colors.text_tertiary).child("▶"))
                     .into_any_element()
            }
            else {
                div().h_full().into_any_element()
            }
        }
    }
}
