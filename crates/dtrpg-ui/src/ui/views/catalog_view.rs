//! Catalog view: list, thumbs, and grid layouts with grouping and empty state.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Entity, FontWeight, Hsla, Image, IntoElement,
    MouseButton, MouseDownEvent, ObjectFit, ParentElement, Pixels, Point, Render, SharedString,
    Styled, StyledImage, TextRun, UniformListScrollHandle, Window, div, img, px, rems,
    uniform_list,
};
use gpui_component::ElementExt;
use gpui_component::WindowExt as _;
use gpui_component::badge::Badge;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem};
use gpui_component::scroll::ScrollableElement;
use gpui_component::spinner::Spinner;
use gpui_component::table::{Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{Sizable, Size};
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::controllers::settings::SettingsController;
use crate::controllers::tabs::TabsController;
use crate::data::constants::{ITEM_POPOVER_MARGIN, ITEM_POPOVER_WIDTH};
use crate::data::enums::CatalogPresentation;
use crate::data::enums::*;
use crate::data::events::LibraryChanged;
use crate::data::library::{LibraryItem, thumbnail_cooldown_elapsed};
use crate::data::theme::{ColorTokens, DensityConstants, LibriTheme};
use crate::ui::library::cover::{CoverCache, render_generative_cover};
use crate::ui::library::drag::DraggedLibraryItem;
use crate::ui::views::item_popover_view::render_item_popover;
use crate::ui::views::manage_collections_dialog::open_manage_collections_dialog;
use crate::util::matching::collection_member_id;
use crate::util::publisher::{PublisherGroup, group_by_publisher};
use crate::util::reveal::reveal_in_file_manager;
use crate::util::sort::{SortDirection, SortMethod};

#[derive(Clone, Copy)]
enum EmptyReason {
    LibraryEmpty,
    NoMatches,
}

/// Builds a catalog item's `on_click` handler: a single click opens the item
/// popover; a double click additionally opens (or activates) an expanded
/// detail tab and dismisses the popover. Mirrors the click-count escalation
/// pattern `gpui-component`'s own `TableState::on_row_left_click` uses.
fn item_click_handler(id: Arc<str>, title: String, entity: Entity<LibraryController>,
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
fn shaped_text_width(window: &Window, text: &str, font_size: Pixels, font_weight: FontWeight)
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
fn is_title_truncated(window: &Window, title: &str, font_size: Pixels, font_weight: FontWeight,
                      available_width: Pixels)
                      -> bool {
    shaped_text_width(window, title, font_size, font_weight) > available_width
}

/// Font size for `.text_sm()` styled title text, resolved against the window's
/// rem size.
fn text_sm_size(window: &Window) -> Pixels {
    rems(0.875).to_pixels(window.rem_size())
}

/// Font size for `.text_xs()` styled title text, resolved against the window's
/// rem size.
fn text_xs_size(window: &Window) -> Pixels {
    rems(0.75).to_pixels(window.rem_size())
}

// ── Shared column definitions
// ─────────────────────────────────────────────────

/// Returns the column definitions used for the list view.
///
/// Used by both the `DataTable` (ungrouped) and the grouped-list header/rows
/// to ensure column widths are always in sync between headers and cells.
fn list_columns() -> Vec<Column> {
    vec![Column::new("title", t!("catalog.col_title")).width(300.)
                                                      .min_width(150.)
                                                      .resizable(true),
         Column::new("publisher", t!("catalog.col_publisher")).width(130.)
                                                              .resizable(true),
         Column::new("system", t!("catalog.col_system")).width(110.)
                                                        .resizable(true),
         Column::new("pages", t!("catalog.col_pages")).width(60.)
                                                      .resizable(true),
         Column::new("size", t!("catalog.col_size")).width(60.)
                                                    .resizable(true),
         Column::new("added", t!("catalog.col_added")).width(80.)
                                                      .resizable(true),
         Column::new("status", "").width(24.)
                                  .resizable(false)
                                  .selectable(false),
         Column::new("open", "").width(28.)
                                .resizable(false)
                                .selectable(false),
         Column::new("ctx", "").width(28.)
                               .resizable(false)
                               .selectable(false),]
}

/// Returns a 2–3 character badge abbreviation for an item kind string.
fn kind_badge(kind: &str) -> &'static str {
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
fn render_item_count_badge(item: &LibraryItem, colors: &ColorTokens)
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
fn render_unavailable_badge(item: &LibraryItem, colors: &ColorTokens)
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

/// Applies a `TableEvent::ColumnWidthsChanged` payload to a delegate's
/// `user_widths`, ignoring mismatched-length payloads (e.g. mid-resize
/// column-count changes).
fn apply_column_widths(user_widths: &mut [Option<Pixels>], widths: &[Pixels]) {
    if widths.len() == user_widths.len() {
        for (slot, &w) in user_widths.iter_mut().zip(widths.iter()) {
            *slot = Some(w);
        }
    }
}

/// Renders a single data cell (column `col_ix`) for `item`. Shared between
/// the ungrouped `CatalogListDelegate` and the grouped
/// `GroupedCatalogListDelegate` so grouped and ungrouped list rows stay
/// visually identical and both benefit from `DataTable`'s virtualization.
// Matches the existing arity of this file's other render_* row functions
// (`render_thumb_row`, `render_grid_card`), which are all-render-context
// parameter lists rather than a natural grouping into a smaller struct.
#[allow(clippy::too_many_arguments)]
fn render_list_item_cell(item: &LibraryItem, col_ix: usize, colors: &ColorTokens,
                         storage_root: &Path, controller: &Entity<LibraryController>,
                         tabs: &Entity<TabsController>, column_width: Pixels, window: &Window,
                         is_checking: bool, is_verifying: bool)
                         -> AnyElement {
    match col_ix {
        0 => {
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
            div().h_full()
                 .flex()
                 .items_center()
                 .gap(px(6.0))
                 .min_w_0()
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

        1 => div().h_full()
                  .flex()
                  .items_center()
                  .text_sm()
                  .text_color(colors.text_secondary)
                  .truncate()
                  .child(item.publisher.to_string())
                  .into_any_element(),

        2 => div().h_full()
                  .flex()
                  .items_center()
                  .text_sm()
                  .text_color(colors.text_secondary)
                  .truncate()
                  .child(item.line.to_string())
                  .into_any_element(),

        3 => div().h_full()
                  .flex()
                  .items_center()
                  .text_sm()
                  .text_color(colors.text_secondary)
                  .child(item.pages.to_string())
                  .into_any_element(),

        4 => div().h_full()
                  .flex()
                  .items_center()
                  .text_sm()
                  .text_color(colors.text_secondary)
                  .child(format!("{:.0} MB", item.size_mb))
                  .into_any_element(),

        5 => div().h_full()
                  .flex()
                  .items_center()
                  .text_sm()
                  .text_color(colors.text_secondary)
                  .child(item.year.to_string())
                  .into_any_element(),

        6 => div().h_full()
                  .flex()
                  .items_center()
                  .justify_center()
                  .child(render_status(&item.id, item.status, is_verifying, colors))
                  .into_any_element(),

        7 => {
            if item.status == ItemStatus::Downloaded {
                let item_open_path =
                    crate::data::storage::publisher_dir(storage_root, &item.publisher);
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

        8 => {
            let ctrl = controller.clone();
            div().h_full()
                 .flex()
                 .items_center()
                 .justify_center()
                 .child(render_thumbnail_menu(item, ctrl))
                 .into_any_element()
        }

        _ => div().into_any_element(),
    }
}

// ── CatalogListDelegate
// ───────────────────────────────────────────────────────

/// `TableDelegate` for the ungrouped list view. Backed by `LibraryController`.
struct CatalogListDelegate {
    controller:   Entity<LibraryController>,
    /// Used to open/focus a multi-item entry's expanded detail tab when the
    /// row's inline open action hits
    /// `OpenError::MultipleFilesRequireSelection`.
    tabs:         Entity<TabsController>,
    storage_root: PathBuf,
    columns:      Vec<Column>,
    /// User-adjusted column widths captured from
    /// `TableEvent::ColumnWidthsChanged`. `None` means use the static
    /// default from `list_columns()`.
    user_widths:  Vec<Option<Pixels>>,
}

impl TableDelegate for CatalogListDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, cx: &App) -> usize {
        self.controller.read(cx).visible_items_count()
    }

    fn loading(&self, cx: &App) -> bool {
        self.controller.read(cx).is_loading()
    }

    fn column(&self, col_ix: usize, cx: &App) -> Column {
        let snap = self.controller.read(cx).snapshot();
        let active_col = match snap.sort {
            SortMethod::Title => Some(0usize),
            SortMethod::Publisher => Some(1),
            SortMethod::PageCount => Some(3),
            SortMethod::DateAdded => Some(5),
            SortMethod::Custom { col_key: "publisher", } => Some(1),
            SortMethod::Custom { col_key: "system" } => Some(2),
            SortMethod::Custom { col_key: "pages" } => Some(3),
            SortMethod::Custom { col_key: "size" } => Some(4),
            SortMethod::Custom { col_key: "added" } => Some(5),
            SortMethod::Custom { .. } => None,
        };
        let mut col = self.columns[col_ix].clone();
        if let Some(Some(w)) = self.user_widths.get(col_ix) {
            col = col.width(w.as_f32());
        }
        if active_col == Some(col_ix) {
            let sort = match snap.sort_direction {
                SortDirection::Ascending => ColumnSort::Ascending,
                SortDirection::Descending => ColumnSort::Descending,
            };
            col.sort(sort)
        }
        else if col_ix < 6 {
            col.sort(ColumnSort::Default)
        }
        else {
            col
        }
    }

    fn perform_sort(&mut self, col_ix: usize, sort: ColumnSort, _window: &mut Window,
                    cx: &mut Context<TableState<Self>>) {
        if col_ix >= 6 {
            return;
        }
        let method = match col_ix {
            0 => SortMethod::Title,
            1 => SortMethod::Custom { col_key: "publisher", },
            2 => SortMethod::Custom { col_key: "system" },
            3 => SortMethod::Custom { col_key: "pages" },
            4 => SortMethod::Custom { col_key: "size" },
            5 => SortMethod::Custom { col_key: "added" },
            _ => return,
        };
        let (method, direction) = match sort {
            ColumnSort::Ascending => (method, SortDirection::Ascending),
            ColumnSort::Descending => (method, SortDirection::Descending),
            ColumnSort::Default => (SortMethod::Title, SortDirection::Ascending),
        };
        self.controller.update(cx, |ctrl, cx| {
                           ctrl.set_sort(method, cx);
                           ctrl.set_sort_direction(direction, cx);
                       });
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, window: &mut Window,
                    cx: &mut Context<TableState<Self>>)
                    -> PopupMenu {
        let items = self.controller
                        .read(cx)
                        .visible_items_slice(row_ix..row_ix + 1);
        let Some(item) = items.into_iter().next()
        else {
            return menu;
        };
        let id = Arc::clone(&item.id);
        let status = item.status;
        let title = item.title.to_string();
        let entity = self.controller.clone();
        let menu = match status {
            ItemStatus::Downloaded => {
                let item_path =
                    crate::data::storage::publisher_dir(&self.storage_root, &item.publisher);
                let entity_remove = entity.clone();
                let remove_id = Arc::clone(&id);
                let item_path_reveal = item_path.clone();
                let item_for_open = item.clone();
                let tabs_for_open = self.tabs.clone();
                let controller_for_open = entity.clone();
                menu.item(PopupMenuItem::new(t!("catalog.action_open")).on_click(
                    move |_, _, cx| {
                        open_item_or_focus_detail_tab(
                            &item_path,
                            &item_for_open,
                            &tabs_for_open,
                            &controller_for_open,
                            cx,
                        );
                    },
                ))
                .item(
                    PopupMenuItem::new(platform_reveal_label()).on_click(move |_, _, _| {
                        if !item_path_reveal.exists() {
                            tracing::warn!(
                                path = %item_path_reveal.display(),
                                "reveal: file not found"
                            );
                            return;
                        }
                        if let Err(e) = reveal_in_file_manager(&item_path_reveal) {
                            tracing::warn!("reveal_in_file_manager failed: {e}");
                        }
                    }),
                )
                .item({
                    let title_for_remove = title.clone();
                    PopupMenuItem::new(t!("catalog.action_remove_download")).on_click(
                        move |_, window, cx| {
                            let entity_remove = entity_remove.clone();
                            let remove_id = Arc::clone(&remove_id);
                            let title_for_remove = title_for_remove.clone();
                            window.open_alert_dialog(cx, move |alert, _, _| {
                                let entity_remove = entity_remove.clone();
                                let remove_id = Arc::clone(&remove_id);
                                alert
                                    .confirm()
                                    .title(t!("catalog.remove_download_confirm_title",
                                               title = title_for_remove.clone())
                                        .to_string())
                                    .description(t!("catalog.remove_download_confirm_description")
                                        .to_string())
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
            ItemStatus::Cloud => menu.item(
                PopupMenuItem::new(t!("catalog.action_download")).on_click(move |_, _, cx| {
                    entity.update(cx, |ctrl, cx| ctrl.request_download(&id, title.clone(), cx));
                }),
            ),
        };
        append_collection_menu_items(menu, &item, &self.controller, window, cx)
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        let items = self.controller
                        .read(cx)
                        .visible_items_slice(row_ix..row_ix + 1);
        let Some(item) = items.into_iter().next()
        else {
            return div().into_any_element();
        };
        let colors = cx.global::<LibriTheme>().colors.clone();
        let column_width = self.column(col_ix, cx).width;
        let is_checking = self.controller.read(cx).is_checking(&item.id);
        let is_verifying = self.controller.read(cx).is_verifying_downloads(&item.id);
        render_list_item_cell(&item,
                              col_ix,
                              &colors,
                              &self.storage_root,
                              &self.controller,
                              &self.tabs,
                              column_width,
                              window,
                              is_checking,
                              is_verifying)
    }

    fn render_th(&mut self, col_ix: usize, _window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        // `gpui-component`'s default `render_th` renders the column name in a
        // plain non-flex `div`, so the text sits at the top of the
        // header cell instead of vertically centered like every
        // `render_td` cell. Match the cell style here.
        let name = self.column(col_ix, cx).name;
        let label_font_family = cx.global::<LibriTheme>().fonts.label_font.clone();
        div().h_full()
             .flex()
             .items_center()
             .text_sm()
             .font_family(label_font_family.to_string())
             .font_weight(FontWeight::MEDIUM)
             .child(name)
    }
}

// ── GroupedCatalogListDelegate
// ────────────────────────────────────────────

/// A row in the flattened, publisher-grouped list: either a group header or
/// a data row. Grouping is flattened into a single row list so the grouped
/// list view can reuse `DataTable`'s virtualization instead of hand-rolling
/// (and fully materializing) its own rows — see `CatalogView::grouped_items`.
enum GroupedRow {
    Header {
        publisher: Arc<str>,
        count:     usize,
    },
    Item(Box<LibraryItem>),
}

/// `TableDelegate` for the grouped list view. Backed by a flattened row list
/// built from `group_by_publisher`, rebuilt only when the controller's
/// visible items change (see `CatalogView::grouped_items`).
struct GroupedCatalogListDelegate {
    controller:   Entity<LibraryController>,
    /// Used to open/focus a multi-item entry's expanded detail tab when the
    /// row's inline open action hits
    /// `OpenError::MultipleFilesRequireSelection`.
    tabs:         Entity<TabsController>,
    storage_root: PathBuf,
    columns:      Vec<Column>,
    user_widths:  Vec<Option<Pixels>>,
    rows:         Vec<GroupedRow>,
}

impl GroupedCatalogListDelegate {
    fn row_at(&self, row_ix: usize) -> Option<&GroupedRow> {
        self.rows.get(row_ix)
    }
}

impl TableDelegate for GroupedCatalogListDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn loading(&self, cx: &App) -> bool {
        self.controller.read(cx).is_loading()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        let mut col = self.columns[col_ix].clone();
        if let Some(Some(w)) = self.user_widths.get(col_ix) {
            col = col.width(w.as_f32());
        }
        col
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, window: &mut Window,
                    cx: &mut Context<TableState<Self>>)
                    -> PopupMenu {
        let item = match self.row_at(row_ix) {
            Some(GroupedRow::Header { publisher, .. }) => {
                let publisher = publisher.to_string();
                let entity = self.controller.clone();
                return menu.item(
                    PopupMenuItem::new(t!("catalog.publisher_download_all")).on_click(
                        move |_, _, cx| {
                            entity.update(cx, |ctrl, cx| {
                                ctrl.request_download_all_for_publisher(&publisher, cx);
                            });
                        },
                    ),
                );
            }
            Some(GroupedRow::Item(item)) => item,
            None => return menu,
        };
        let id = Arc::clone(&item.id);
        let status = item.status;
        let title = item.title.to_string();
        let entity = self.controller.clone();
        let menu = match status {
            ItemStatus::Downloaded => {
                let item_path =
                    crate::data::storage::publisher_dir(&self.storage_root, &item.publisher);
                let entity_remove = entity.clone();
                let remove_id = Arc::clone(&id);
                let item_path_reveal = item_path.clone();
                let item_for_open = item.clone();
                let tabs_for_open = self.tabs.clone();
                let controller_for_open = entity.clone();
                menu.item(PopupMenuItem::new(t!("catalog.action_open")).on_click(
                    move |_, _, cx| {
                        open_item_or_focus_detail_tab(
                            &item_path,
                            &item_for_open,
                            &tabs_for_open,
                            &controller_for_open,
                            cx,
                        );
                    },
                ))
                .item(
                    PopupMenuItem::new(platform_reveal_label()).on_click(move |_, _, _| {
                        if !item_path_reveal.exists() {
                            tracing::warn!(
                                path = %item_path_reveal.display(),
                                "reveal: file not found"
                            );
                            return;
                        }
                        if let Err(e) = reveal_in_file_manager(&item_path_reveal) {
                            tracing::warn!("reveal_in_file_manager failed: {e}");
                        }
                    }),
                )
                .item({
                    let title_for_remove = title.clone();
                    PopupMenuItem::new(t!("catalog.action_remove_download")).on_click(
                        move |_, window, cx| {
                            let entity_remove = entity_remove.clone();
                            let remove_id = Arc::clone(&remove_id);
                            let title_for_remove = title_for_remove.clone();
                            window.open_alert_dialog(cx, move |alert, _, _| {
                                let entity_remove = entity_remove.clone();
                                let remove_id = Arc::clone(&remove_id);
                                alert
                                    .confirm()
                                    .title(t!("catalog.remove_download_confirm_title",
                                               title = title_for_remove.clone())
                                        .to_string())
                                    .description(t!("catalog.remove_download_confirm_description")
                                        .to_string())
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
            ItemStatus::Cloud => menu.item(
                PopupMenuItem::new(t!("catalog.action_download")).on_click(move |_, _, cx| {
                    entity.update(cx, |ctrl, cx| ctrl.request_download(&id, title.clone(), cx));
                }),
            ),
        };
        append_collection_menu_items(menu, item, &self.controller, window, cx)
    }

    fn render_tr(&mut self, row_ix: usize, _window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> gpui::Stateful<gpui::Div> {
        let id = ("grouped-row", row_ix);
        match self.row_at(row_ix) {
            Some(GroupedRow::Header { .. }) => {
                let colors = cx.global::<LibriTheme>().colors.clone();
                div().id(id).bg(colors.hover)
            }
            _ => div().id(id),
        }
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        let colors = cx.global::<LibriTheme>().colors.clone();
        match self.row_at(row_ix) {
            Some(GroupedRow::Header { publisher, count }) if col_ix == 0 => {
                div().h_full()
                     .flex()
                     .items_center()
                     .gap(px(8.0))
                     .child(div().text_sm()
                                 .font_weight(FontWeight::SEMIBOLD)
                                 .text_color(colors.text_primary)
                                 .child(publisher.to_string()))
                     .child(div().text_xs()
                                 .text_color(colors.text_tertiary)
                                 .child(count.to_string()))
                     .into_any_element()
            }
            Some(GroupedRow::Header { .. }) => div().h_full().into_any_element(),
            Some(GroupedRow::Item(item)) => {
                let column_width = self.column(col_ix, cx).width;
                let is_checking = self.controller.read(cx).is_checking(&item.id);
                let is_verifying = self.controller.read(cx).is_verifying_downloads(&item.id);
                render_list_item_cell(item,
                                      col_ix,
                                      &colors,
                                      &self.storage_root,
                                      &self.controller,
                                      &self.tabs,
                                      column_width,
                                      window,
                                      is_checking,
                                      is_verifying)
            }
            None => div().into_any_element(),
        }
    }

    fn render_th(&mut self, col_ix: usize, _window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        let name = self.column(col_ix, cx).name;
        let label_font_family = cx.global::<LibriTheme>().fonts.label_font.clone();
        div().h_full()
             .flex()
             .items_center()
             .text_sm()
             .font_family(label_font_family.to_string())
             .font_weight(FontWeight::MEDIUM)
             .child(name)
    }
}

// ── CatalogView
// ───────────────────────────────────────────────────────────────

/// GPUI view for the catalog area. Holds scroll state and delegates to
/// `DataTable` for list layout and `uniform_list` for thumbs/grid layouts.
pub struct CatalogView {
    controller:                 Entity<LibraryController>,
    settings:                   Entity<SettingsController>,
    tabs:                       Entity<TabsController>,
    scroll_handle:              UniformListScrollHandle,
    catalog_list_table:         Entity<TableState<CatalogListDelegate>>,
    /// Backs the grouped list presentation. Rows are a flattened list of
    /// group headers and items (see `GroupedRow`), rebuilt only when
    /// `grouped_cache` is refreshed — this is what lets the grouped list
    /// reuse `DataTable`'s row virtualization instead of hand-rolling and
    /// fully materializing its own rows.
    catalog_grouped_list_table: Entity<TableState<GroupedCatalogListDelegate>>,
    /// Cached items-per-row for the grid layout; updated each render from
    /// the window viewport width. Initialized to 4 as a safe default.
    items_per_row:              usize,
    /// Cached publisher grouping of the controller's visible items.
    /// Invalidated (set to `None`) on `LibraryChanged`; repopulated lazily
    /// during `render()` so grouped presentation modes avoid re-grouping on
    /// every hover-triggered re-render. Also drives the flattened rows
    /// pushed into `catalog_grouped_list_table`.
    grouped_cache:              Option<Vec<PublisherGroup>>,
    /// Cursor position captured at the mouse-down that opened the single-click
    /// item popover (see `render_item_popover`). Frozen for the lifetime of
    /// the popover — never updated by subsequent mouse movement — so the
    /// popover stays put while the pointer wanders after opening it, instead
    /// of following the cursor. Cleared to `None` when the popover closes.
    ///
    /// Used only as a fallback anchor rectangle (zero-sized, at the click
    /// point) for the first frame after selection and for presentations
    /// (List) whose entries don't report their own bounds — see
    /// `LibraryController::popover_anchor_bounds` for the precise rectangle
    /// Grid/Thumbs entries supply once painted.
    popover_anchor_pos:         Option<Point<Pixels>>,
}

impl CatalogView {
    /// Creates a new `CatalogView` connected to the given controller and
    /// settings.
    pub fn new(window: &mut Window, cx: &mut Context<Self>,
               controller: Entity<LibraryController>, settings: Entity<SettingsController>,
               tabs: Entity<TabsController>)
               -> Self {
        let storage_root = settings.read(cx).snapshot().storage_root_path;
        let cols = list_columns();
        let col_count = cols.len();
        let delegate = CatalogListDelegate { controller:   controller.clone(),
                                             tabs:         tabs.clone(),
                                             storage_root: storage_root.clone(),
                                             columns:      cols.clone(),
                                             user_widths:  vec![None; col_count], };
        let catalog_list_table = cx.new(|cx| {
                                       TableState::new(delegate, window, cx).row_selectable(true)
                                                                            .col_selectable(false)
                                                                            .col_movable(false)
                                                                            .col_resizable(true)
                                                                            .sortable(true)
                                   });

        let grouped_delegate = GroupedCatalogListDelegate { controller: controller.clone(),
                                                            tabs: tabs.clone(),
                                                            storage_root,
                                                            columns: cols,
                                                            user_widths: vec![None; col_count],
                                                            rows: Vec::new() };
        let catalog_grouped_list_table = cx.new(|cx| {
                                               TableState::new(grouped_delegate, window, cx)
                .row_selectable(true)
                .col_selectable(false)
                .col_movable(false)
                .col_resizable(true)
                .sortable(false)
                                           });

        cx.subscribe(&controller, {
              let table = catalog_list_table.clone();
              move |this, ctrl, _event: &LibraryChanged, cx| {
                  table.update(cx, |state, cx| state.refresh(cx));
                  this.grouped_cache = None;
                  if ctrl.read(cx).selected_item().is_none() {
                      this.popover_anchor_pos = None;
                  }
              }
          })
          .detach();

        cx.subscribe(&catalog_list_table,
                     |this, table, event: &TableEvent, cx| match event {
                         TableEvent::SelectRow(row_ix) => {
                             let row_ix = *row_ix;
                             let items = this.controller
                                             .read(cx)
                                             .visible_items_slice(row_ix..row_ix + 1);
                             if let Some(item) = items.first() {
                                 let id = Arc::clone(&item.id);
                                 this.controller
                                     .update(cx, |ctrl, cx| ctrl.select_item(id, cx));
                             }
                         }
                         TableEvent::DoubleClickedRow(row_ix) => {
                             let row_ix = *row_ix;
                             let items = this.controller
                                             .read(cx)
                                             .visible_items_slice(row_ix..row_ix + 1);
                             if let Some(item) = items.first() {
                                 let id = Arc::clone(&item.id);
                                 let title = item.title.to_string();
                                 this.tabs.update(cx, |t, cx| {
                                              t.open_detail_tab(Arc::clone(&id), title, cx);
                                          });
                                 this.controller.update(cx, |ctrl, cx| {
                                                    ctrl.clear_selection(cx);
                                                    ctrl.clear_item_selection(&id, cx);
                                                    ctrl.maybe_check_item(Arc::clone(&id), cx);
                                                    ctrl.ensure_detail_cover(&id, cx);
                                                });
                             }
                         }
                         TableEvent::ColumnWidthsChanged(widths) => {
                             let widths = widths.clone();
                             table.update(cx, |state, _cx| {
                                      apply_column_widths(&mut state.delegate_mut().user_widths,
                                                          &widths);
                                  });
                             this.catalog_grouped_list_table.update(cx, |state, _cx| {
                                                                apply_column_widths(
                                &mut state.delegate_mut().user_widths,
                                &widths,
                            );
                                                            });
                         }
                         _ => {}
                     })
          .detach();

        cx.subscribe(&catalog_grouped_list_table,
                     |this, table, event: &TableEvent, cx| match event {
                         // Group header rows aren't selectable items — ignore clicks on them.
                         TableEvent::SelectRow(row_ix) => {
                             let row_ix = *row_ix;
                             let id = table.read(cx).delegate().row_at(row_ix).and_then(|row| {
                                                                                  match row {
                            GroupedRow::Item(item) => Some(Arc::clone(&item.id)),
                            GroupedRow::Header { .. } => None,
                        }
                                                                              });
                             if let Some(id) = id {
                                 this.controller
                                     .update(cx, |ctrl, cx| ctrl.select_item(id, cx));
                             }
                         }
                         TableEvent::DoubleClickedRow(row_ix) => {
                             let row_ix = *row_ix;
                             let item = table.read(cx).delegate().row_at(row_ix).and_then(|row| {
                                                                                    match row {
                            GroupedRow::Item(item) => Some(item.clone()),
                            GroupedRow::Header { .. } => None,
                        }
                                                                                });
                             if let Some(item) = item {
                                 let id = Arc::clone(&item.id);
                                 let title = item.title.to_string();
                                 this.tabs.update(cx, |t, cx| {
                                              t.open_detail_tab(Arc::clone(&id), title, cx);
                                          });
                                 this.controller.update(cx, |ctrl, cx| {
                                                    ctrl.clear_selection(cx);
                                                    ctrl.clear_item_selection(&id, cx);
                                                    ctrl.maybe_check_item(Arc::clone(&id), cx);
                                                    ctrl.ensure_detail_cover(&id, cx);
                                                });
                             }
                         }
                         TableEvent::ColumnWidthsChanged(widths) => {
                             let widths = widths.clone();
                             table.update(cx, |state, _cx| {
                                      apply_column_widths(&mut state.delegate_mut().user_widths,
                                                          &widths);
                                  });
                             this.catalog_list_table.update(cx, |state, _cx| {
                                                        apply_column_widths(
                                &mut state.delegate_mut().user_widths,
                                &widths,
                            );
                                                    });
                         }
                         _ => {}
                     })
          .detach();

        Self { controller,
               settings,
               tabs,
               scroll_handle: UniformListScrollHandle::default(),
               catalog_list_table,
               catalog_grouped_list_table,
               items_per_row: 4,
               grouped_cache: None,
               popover_anchor_pos: None }
    }

    /// Returns the cached publisher grouping, computing and storing it first if
    /// stale. Also pushes the flattened row list into
    /// `catalog_grouped_list_table` whenever the grouping is recomputed, so
    /// the grouped `DataTable` stays in sync without re-flattening on every
    /// render.
    fn grouped_items(&mut self, cx: &mut Context<Self>) -> Vec<PublisherGroup> {
        if self.grouped_cache.is_none() {
            let items = self.controller.read(cx).visible_items();
            let groups = group_by_publisher(items);
            self.grouped_cache = Some(groups.clone());

            let mut rows = Vec::new();
            for group in groups {
                rows.push(GroupedRow::Header { publisher: Arc::clone(&group.publisher),
                                               count:     group.items.len(), });
                rows.extend(group.items
                                 .into_iter()
                                 .map(|item| GroupedRow::Item(Box::new(item))));
            }
            self.catalog_grouped_list_table.update(cx, |state, cx| {
                                               state.delegate_mut().rows = rows;
                                               state.refresh(cx);
                                           });
        }
        self.grouped_cache.clone().unwrap_or_default()
    }
}

/// Computes the top-left corner at which to draw the item popover so it sits
/// beside `entry` — the clicked catalog entry's on-screen bounds — rather
/// than on top of it.
///
/// Prefers the entry's right edge; falls back to its left edge when the
/// popover wouldn't fit within `window_width` on the right. Vertically, the
/// popover is always top-aligned with the entry, so it never covers it.
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

impl Render for CatalogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snap = self.controller.read(cx).snapshot();
        let item_count = self.controller.read(cx).visible_items_count();
        let theme = cx.global::<LibriTheme>().clone();
        let colors = theme.colors;
        let density = theme.density_constants;
        let storage_root = self.settings.read(cx).snapshot().storage_root_path;

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
        self.items_per_row = ((usable_w / card_pitch) as usize).max(1);

        let items_per_row = self.items_per_row;
        let scroll_handle = self.scroll_handle.clone();
        let ctrl = self.controller.clone();
        let tabs_entity = self.tabs.clone();

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
                    render_no_matches_state(&snap.search_query, colors.text_tertiary)
                        .into_any_element()
                }
            };
            return outer.child(root.child(empty_state)).into_any_element();
        }

        let content: AnyElement = match (snap.presentation, snap.grouped) {
            // ── List, ungrouped — DataTable (handles header/row alignment) ──
            (CatalogPresentation::List, false) => {
                use gpui_component::Size;
                root.px(pad_side)
                    .child(
                        DataTable::new(&self.catalog_list_table)
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
                use gpui_component::Size;
                self.grouped_items(cx);
                root.px(pad_side)
                    .child(
                        DataTable::new(&self.catalog_grouped_list_table)
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
                        let checking: Vec<(bool, bool)> = {
                            let ctrl_ref = ctrl.read(cx);
                            items.iter()
                                 .map(|item| {
                                     (ctrl_ref.is_checking(&item.id),
                                      ctrl_ref.is_verifying_downloads(&item.id))
                                 })
                                 .collect()
                        };
                        items.iter()
                             .zip(covers)
                             .zip(checking)
                             .map(|((item, cover), (is_checking, is_verifying))| {
                                 render_thumb_row(item,
                                                  cover,
                                                  &c,
                                                  &d,
                                                  ctrl.clone(),
                                                  t.clone(),
                                                  s.clone(),
                                                  window,
                                                  is_checking,
                                                  is_verifying).into_any_element()
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
                let groups = self.grouped_items(cx);
                let cover_cache = {
                    let cache = cx.global::<CoverCache>();
                    cache.images.clone()
                };
                let checking_items = self.controller.read(cx).checking_items_snapshot();
                let verifying_downloads = self.controller.read(cx).verifying_downloads_snapshot();
                root.overflow_y_scrollbar()
                    .px(pad_side)
                    .children(groups.into_iter().map(|g| {
                        let c = colors.clone();
                        let d = density.clone();
                        let e = self.controller.clone();
                        let t = tabs_entity.clone();
                        let s = storage_root.clone();
                        let cc = cover_cache.clone();
                        let checking_items = checking_items.clone();
                        let verifying_downloads = verifying_downloads.clone();
                        // Reborrow as `&Window` (Copy) so the nested
                        // `move` closure can capture it on every
                        // outer iteration without moving `window`.
                        let window: &Window = &*window;
                        div().child(render_group_header(&g.publisher, g.items.len(), &c,
                                                        e.clone()))
                             .children(g.items.into_iter().map(move |item| {
                                                              let cover = cc.get(&item.id).cloned();
                                                              let is_checking =
                                                                  checking_items.contains(&item.id);
                                                              let is_verifying = verifying_downloads
                                                                  .contains(&item.id);
                                                              render_thumb_row(&item,
                                                                               cover,
                                                                               &c,
                                                                               &d,
                                                                               e.clone(),
                                                                               t.clone(),
                                                                               s.clone(),
                                                                               window,
                                                                               is_checking,
                                                                               is_verifying)
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
                        let checking: Vec<(bool, bool)> = {
                            let ctrl_ref = ctrl.read(cx);
                            items.iter()
                                 .map(|item| {
                                     (ctrl_ref.is_checking(&item.id),
                                      ctrl_ref.is_verifying_downloads(&item.id))
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
                                                                    &(is_checking, is_verifying),
                                                                )| {
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
                                                                        is_verifying,
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
                let groups = self.grouped_items(cx);
                let cover_cache = {
                    let cache = cx.global::<CoverCache>();
                    cache.images.clone()
                };
                let checking_items = self.controller.read(cx).checking_items_snapshot();
                let verifying_downloads = self.controller.read(cx).verifying_downloads_snapshot();
                root.overflow_y_scrollbar()
                    .px(pad_side)
                    .children(groups.into_iter().map(|g| {
                                                    let c = colors.clone();
                                                    let d = density.clone();
                                                    let e = self.controller.clone();
                                                    let t = tabs_entity.clone();
                                                    let s = storage_root.clone();
                                                    let cc = cover_cache.clone();
                                                    let ci = checking_items.clone();
                                                    let vd = verifying_downloads.clone();
                                                    div().child(render_group_header(&g.publisher,
                                                                                    g.items.len(),
                                                                                    &c,
                                                                                    e.clone()))
                                                         .child(render_grid(g.items, cc, ci, vd,
                                                                            c, d, e, t, s,
                                                                            &*window))
                                                }))
                    .into_any_element()
            }
        };

        let mut result =
            outer.relative()
                 .on_mouse_down(MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, _window, _cx| {
                                      this.popover_anchor_pos = Some(event.position);
                                  }))
                 .child(content);

        if let Some(item) = snap.selected_item.as_ref() {
            let entry_bounds = self.controller
                                   .read(cx)
                                   .entry_bounds(&item.id)
                                   .unwrap_or(Bounds { origin: self.popover_anchor_pos
                                                                   .unwrap_or_default(),
                                                       size:   gpui::Size::default(), });
            let anchor = popover_anchor_point(entry_bounds,
                                              px(ITEM_POPOVER_WIDTH),
                                              px(ITEM_POPOVER_MARGIN),
                                              window.viewport_size().width);
            let is_checking = self.controller.read(cx).is_checking(&item.id);
            let is_verifying = self.controller.read(cx).is_verifying_downloads(&item.id);
            result = result.child(render_item_popover(item,
                                                      anchor,
                                                      self.controller.clone(),
                                                      tabs_entity.clone(),
                                                      &colors,
                                                      is_checking,
                                                      is_verifying));
        }

        result.into_any_element()
    }
}

// ── Empty state
// ───────────────────────────────────────────────────────────────

fn render_no_matches_state(search_query: &str, text_color: Hsla) -> impl IntoElement + 'static {
    let hint = if search_query.is_empty() {
        t!("catalog.try_different_section")
    }
    else {
        t!("catalog.try_clear_search")
    };

    div().flex()
         .flex_col()
         .items_center()
         .justify_center()
         .h_full()
         .gap(px(12.0))
         .child(div().text_2xl().text_color(text_color).child("⊘"))
         .child(div().text_sm()
                     .text_color(text_color)
                     .child(t!("catalog.no_titles_match")))
         .child(div().text_xs().text_color(text_color).child(hint))
}

fn render_library_empty_state(text_color: Hsla) -> impl IntoElement + 'static {
    div().flex()
         .flex_col()
         .items_center()
         .justify_center()
         .h_full()
         .gap(px(12.0))
         .child(div().text_2xl().text_color(text_color).child("⊘"))
         .child(div().text_sm()
                     .text_color(text_color)
                     .child(t!("catalog.library_empty")))
}

// ── Group header
// ──────────────────────────────────────────────────────────────

fn render_group_header(publisher: &str, count: usize, colors: &ColorTokens,
                       entity: Entity<LibraryController>)
                       -> impl IntoElement + 'static + use<> {
    let text_primary = colors.text_primary;
    let text_tertiary = colors.text_tertiary;
    let publisher = publisher.to_string();
    let menu_publisher = publisher.clone();
    div().id(SharedString::from(format!("group-header-{publisher}")))
         .flex()
         .items_center()
         .gap(px(8.0))
         .py(px(10.0))
         .child(div().text_sm()
                     .font_weight(FontWeight::SEMIBOLD)
                     .text_color(text_primary)
                     .child(publisher))
         .child(div().text_xs()
                     .text_color(text_tertiary)
                     .child(count.to_string()))
         .context_menu(move |menu, _, _| {
             let entity = entity.clone();
             let publisher = menu_publisher.clone();
             menu.item(PopupMenuItem::new(t!("catalog.publisher_download_all")).on_click(
                 move |_, _, cx| {
                     entity.update(cx,
                                   |ctrl, cx| ctrl.request_download_all_for_publisher(&publisher, cx));
                 },
             ))
         })
}

// ── Status glyph
// ──────────────────────────────────────────────────────────────

/// Renders the download-status glyph for `id`. While `is_verifying` is true
/// (a file-presence check per `verify-downloaded-status-against-disk` is in
/// flight for this item), the glyph is replaced by a tinted spinner with its
/// own tooltip rather than showing a downloaded/cloud state that may be
/// stale.
fn render_status(id: &Arc<str>, status: ItemStatus, is_verifying: bool, colors: &ColorTokens)
                 -> AnyElement {
    if is_verifying {
        return render_verifying_indicator(id, colors).into_any_element();
    }
    let accent = colors.accent;
    let text_tertiary = colors.text_tertiary;
    match status {
        ItemStatus::Downloaded => div().size(px(7.0))
                                       .rounded_full()
                                       .bg(accent)
                                       .flex_none()
                                       .into_any_element(),
        ItemStatus::Cloud => div().text_xs()
                                  .text_color(text_tertiary)
                                  .flex_none()
                                  .child("☁")
                                  .into_any_element(),
    }
}

/// Renders a small tinted spinner with a "checking download status" tooltip
/// for a catalog entry whose file-presence verification is currently in
/// flight (`LibraryController::is_verifying_downloads`, see
/// `verify-downloaded-status-against-disk`). Distinct from
/// [`render_checking_indicator`]'s untinted spinner (a network-bound
/// availability check) so the two kinds of in-flight check are never
/// visually conflated.
fn render_verifying_indicator(id: &Arc<str>, colors: &ColorTokens)
                              -> impl IntoElement + 'static + use<> {
    let elem_id: Arc<str> = Arc::from(format!("verifying-{id}"));
    div().id(elem_id)
         .flex_none()
         .child(Spinner::new().with_size(Size::XSmall)
                              .color(colors.text_tertiary))
         .tooltip(|window, cx| {
             Tooltip::new(t!("detail.tooltip_verifying_download").to_string()).build(window, cx)
         })
}

// ── Thumbnail context menu
// ─────────────────────────────────────────────────────

/// Renders a small "⋯" button that opens a dropdown with a "Load Thumbnail"
/// action.  The action is disabled when no `cover_url` is present or when the
/// cooldown guard is active.
fn render_thumbnail_menu(item: &LibraryItem, entity: Entity<LibraryController>)
                         -> impl IntoElement + 'static + use<> {
    let can_load = item.cover_url.is_some() && thumbnail_cooldown_elapsed(item);
    let cover_url = item.cover_url.clone();
    let btn_id: Arc<str> = Arc::from(format!("ctx-thumb-{}", &*item.id));

    Button::new(btn_id).ghost()
                       .small()
                       .label("\u{22ef}")
                       .dropdown_menu(move |menu, _, _| {
                           let eu = entity.clone();
                           let url = cover_url.clone();
                           menu.item(
                PopupMenuItem::new(t!("catalog.action_load_thumbnail"))
                    .disabled(!can_load)
                    .on_click(move |_, _, cx| {
                        if let Some(ref u) = url {
                            eu.update(cx, |ctrl, cx| ctrl.load_thumbnail(Arc::clone(u), cx));
                        }
                    }),
            )
                       })
}

// ── Reveal action
// ─────────────────────────────────────────────────────────────

fn platform_reveal_label() -> std::borrow::Cow<'static, str> {
    #[cfg(target_os = "macos")]
    return t!("catalog.action_show_in_finder");
    #[cfg(target_os = "windows")]
    return t!("catalog.action_show_in_explorer");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return t!("catalog.action_show_in_files");
}

// ── Thumbs layout
// ─────────────────────────────────────────────────────────────

// See the `render_grid` allow above — `window` is needed for title-truncation
// measurement.
#[allow(clippy::too_many_arguments)]
fn render_thumb_row(item: &LibraryItem, cover_image: Option<Arc<Image>>, colors: &ColorTokens,
                    density: &DensityConstants, entity: Entity<LibraryController>,
                    tabs: Entity<TabsController>, storage_root_path: PathBuf, window: &Window,
                    is_checking: bool, is_verifying: bool)
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
    let ctx_menu = render_thumbnail_menu(item, entity.clone());
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
    let ctx_path = crate::data::storage::publisher_dir(&storage_root_path, &publisher);

    let cover: AnyElement = if let Some(image) = cover_image {
        img(image).w(px(thumb_w))
                  .h(px(thumb_h))
                  .object_fit(ObjectFit::Cover)
                  .into_any_element()
    }
    else {
        render_generative_cover(item, thumb_w, thumb_h, false).into_any_element()
    };

    let cover_cell: AnyElement = {
        let mut inner = div().relative()
                             .rounded(px(3.0))
                             .overflow_hidden()
                             .flex_none()
                             .child(cover);
        if let Some(badge) = render_item_count_badge(item, &colors) {
            inner = inner.child(div().absolute().bottom(px(2.0)).right(px(2.0)).child(badge));
        }
        if status == ItemStatus::Downloaded {
            Badge::new().dot()
                        .color(gpui::green())
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
         .child(render_status(&id, status, is_verifying, &colors))
         .child(ctx_menu)
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
                                        .title(t!("catalog.remove_download_confirm_title",
                                                   title = remove_title.clone())
                                            .to_string())
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
             append_collection_menu_items(menu, &ctx_item, &ctx_entity, window, cx)
         })
}

// ── Collections context menu items
// ───────────────────────────────────────

/// Appends an "Add to…" submenu (checked per current membership) and, when
/// the active sidebar filter is a collection, a "Remove from this
/// collection" item to `menu` for `item`.
///
/// Shared by all four catalog item context menu sites (ungrouped/grouped
/// list-layout table delegates, and the thumb/grid row builders) so the
/// collections logic isn't duplicated four times over.
fn append_collection_menu_items(menu: PopupMenu, item: &LibraryItem,
                                entity: &Entity<LibraryController>, _window: &mut Window,
                                _cx: &mut App)
                                -> PopupMenu {
    let member_id = collection_member_id(item);
    let product_id = item.product_id;
    let entity = entity.clone();
    let item_title = Arc::clone(&item.title);
    menu.item(
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

// ── Grid layout
// ───────────────────────────────────────────────────────────────

// `window` is required to measure title-truncation width for the tooltip;
// grouping the remaining rendering context into a struct is left for a
// follow-up since these call sites are shared with the grouped/ungrouped list
// delegates' own parameter lists.
#[allow(clippy::too_many_arguments)]
fn render_grid(items: Vec<LibraryItem>, cover_cache: HashMap<Arc<str>, Arc<Image>>,
               checking_items: HashSet<Arc<str>>, verifying_downloads: HashSet<Arc<str>>,
               colors: ColorTokens, density: DensityConstants,
               entity: Entity<LibraryController>, tabs: Entity<TabsController>,
               storage_root_path: PathBuf, window: &Window)
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
                                        let is_verifying = verifying_downloads.contains(&item.id);
                                        render_grid_card(&item,
                                                         cover,
                                                         &colors,
                                                         min_w,
                                                         entity.clone(),
                                                         tabs.clone(),
                                                         storage_root_path.clone(),
                                                         window,
                                                         is_checking,
                                                         is_verifying)
                                    }))
}

// See the `render_grid` allow above — `window` is needed for title-truncation
// measurement.
#[allow(clippy::too_many_arguments)]
fn render_grid_card(item: &LibraryItem, cover_image: Option<Arc<Image>>, colors: &ColorTokens,
                    card_w: f32, entity: Entity<LibraryController>,
                    tabs: Entity<TabsController>, storage_root_path: PathBuf, window: &Window,
                    is_checking: bool, is_verifying: bool)
                    -> impl IntoElement + 'static + use<> {
    let id = Arc::clone(&item.id);
    let title = item.title.to_string();
    let publisher = item.publisher.to_string();
    let status = item.status;
    let cover_h = card_w * 10.0 / 7.0;
    let reveal_item_id = Arc::clone(&item.id);
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
    let ctx_menu = render_thumbnail_menu(item, entity.clone());
    let ctx_id = Arc::clone(&id);
    let ctx_entity = entity.clone();
    let ctx_item = item.clone();
    let ctx_tabs = tabs.clone();
    let bounds_entity = entity.clone();
    let bounds_id = Arc::clone(&id);
    let ctx_path = crate::data::storage::publisher_dir(&storage_root_path, &publisher);
    let drag_payload = DraggedLibraryItem { title:      title.clone().into(),
                                            member_id:  collection_member_id(item),
                                            product_id: item.product_id, };

    let reveal_row: AnyElement = if status == ItemStatus::Downloaded {
        let item_reveal_path = crate::data::storage::publisher_dir(&storage_root_path, &publisher);
        let reveal_elem_id: Arc<str> = Arc::from(format!("reveal-grid-{}", &*reveal_item_id));
        div().id(reveal_elem_id)
             .mt(px(2.0))
             .cursor_pointer()
             .on_click(move |_, _, _| {
                 if !item_reveal_path.exists() {
                     tracing::warn!(
                         path = %item_reveal_path.display(),
                         "reveal: file not found — item may need re-download"
                     );
                     return;
                 }
                 if let Err(e) = reveal_in_file_manager(&item_reveal_path) {
                     tracing::warn!("reveal_in_file_manager failed: {e}");
                 }
             })
             .child(div().text_xs()
                         .text_color(colors.text_tertiary)
                         .child(platform_reveal_label()))
             .into_any_element()
    }
    else {
        div().into_any_element()
    };

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
        if status == ItemStatus::Downloaded {
            Badge::new().dot()
                        .color(gpui::green())
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
                                 .child(render_status(&id, status, is_verifying, &colors)))
                     .child(reveal_row)
                     .child(ctx_menu))
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
                                        .title(t!("catalog.remove_download_confirm_title",
                                                   title = remove_title.clone())
                                            .to_string())
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
             append_collection_menu_items(menu, &ctx_item, &ctx_entity, window, cx)
         })
}
