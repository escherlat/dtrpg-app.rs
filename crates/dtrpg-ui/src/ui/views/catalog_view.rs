//! Catalog view: list, thumbs, and grid layouts with grouping and empty state.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dtrpg_core::data::constants::{ITEM_POPOVER_MARGIN, ITEM_POPOVER_WIDTH};
use dtrpg_core::data::enums::*;
use dtrpg_core::data::enums::{CatalogPresentation, GroupBy};
use dtrpg_core::data::library::{LibraryItem, thumbnail_cooldown_elapsed};
use dtrpg_core::platform::{Platform, PlatformIntegration, PlatformKind};
use dtrpg_core::util::matching::collection_member_id;
use dtrpg_core::util::sort::{SortDirection, SortMethod};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Entity, FontWeight, Hsla, Image, IntoElement,
    MouseButton, MouseDownEvent, ObjectFit, ParentElement, Pixels, Point, Render, SharedString,
    Styled, StyledImage, TextRun, UniformListScrollHandle, Window, div, img, px, rems,
    uniform_list,
};
use gpui_kit::component::ElementExt;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::badge::Badge;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::table::{
    Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState,
};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable, Size};
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::controllers::settings::SettingsController;
use crate::controllers::tabs::TabsController;
use crate::data::events::LibraryChanged;
use crate::data::theme::{ColorTokens, DensityConstants, LibriTheme};
use crate::ui::library::cover::{CoverCache, render_generative_cover};
use crate::ui::library::drag::DraggedLibraryItem;
use crate::ui::views::item_popover_view::render_item_popover;
use crate::ui::views::list_columns::{
    ListColumnSet, column_chooser_menu, column_sorted_by, sort_method_for,
};
use crate::ui::views::manage_collections_dialog::open_manage_collections_dialog;
use crate::util::publisher::ItemGroup;
use crate::util::reveal::reveal_in_file_manager;

#[derive(Clone, Copy)]
enum EmptyReason {
    LibraryEmpty,
    NoMatches,
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
    /// Cached reference to the controller's grouped items. Compared by Arc
    /// pointer so the grouped list only rebuilds its flattened rows when the
    /// controller cache actually changes, not on every hover-triggered
    /// re-render. Also drives the flattened rows pushed into
    /// `catalog_grouped_list_table`.
    grouped_cache:              Option<Arc<Vec<ItemGroup>>>,

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
    popover_anchor_pos: Option<Point<Pixels>>,
}

impl CatalogView {
    /// Creates a new `CatalogView` connected to the given controller and
    /// settings.
    pub fn new(window: &mut Window, cx: &mut Context<Self>,
               controller: Entity<LibraryController>, settings: Entity<SettingsController>,
               tabs: Entity<TabsController>)
               -> Self {
        let storage_root = settings.read(cx).snapshot().storage_root_path;
        let chosen_columns = controller.read(cx).list_columns.clone();
        let delegate = CatalogListDelegate { controller:   controller.clone(),
                                             tabs:         tabs.clone(),
                                             storage_root: storage_root.clone(),
                                             cols:         ListColumnSet::new(&chosen_columns), };
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
                                                            cols:
                                                                ListColumnSet::new(&chosen_columns),
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
                  let chosen = ctrl.read(cx).list_columns.clone();
                  this.catalog_grouped_list_table.update(cx, |state, cx| {
                                                     if state.delegate_mut()
                                                             .cols
                                                             .set_chosen(&chosen)
                                                     {
                                                         state.refresh(cx);
                                                     }
                                                 });
                  table.update(cx, |state, cx| {
                           state.delegate_mut().cols.set_chosen(&chosen);
                           state.refresh(cx);
                       });
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
                                      state.delegate_mut().cols.apply_widths(&widths);
                                  });
                             this.catalog_grouped_list_table.update(cx, |state, _cx| {
                                                                state.delegate_mut()
                                                                     .cols
                                                                     .apply_widths(&widths);
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
                                      state.delegate_mut().cols.apply_widths(&widths);
                                  });
                             this.catalog_list_table.update(cx, |state, _cx| {
                                                        state.delegate_mut()
                                                             .cols
                                                             .apply_widths(&widths);
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

    /// Returns the controller's grouped items, rebuilding the flattened rows
    /// for `catalog_grouped_list_table` only when the controller's cache has
    /// changed.
    fn grouped_items(&mut self, cx: &mut Context<Self>) -> Vec<ItemGroup> {
        let controller = self.controller.read(cx);
        let groups = controller.visible_items_grouped();
        let rebuild = self.grouped_cache
                          .as_ref()
                          .is_none_or(|cached| !Arc::ptr_eq(cached, &groups));
        if rebuild {
            self.grouped_cache = Some(Arc::clone(&groups));

            let group_kind = controller.group_by;
            let mut rows = Vec::new();
            for group in groups.iter() {
                rows.push(GroupedRow::Header { title: Arc::clone(&group.title),
                                               count: group.items.len(),
                                               kind:  group_kind, });
                rows.extend(group.items
                                 .iter()
                                 .map(|item| GroupedRow::Item(Box::new(item.clone()))));
            }
            self.catalog_grouped_list_table.update(cx, |state, cx| {
                                               state.delegate_mut().rows = rows;
                                               state.refresh(cx);
                                           });
        }
        (*groups).clone()
    }
}

impl Render for CatalogView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        render::render_catalog_view(self, window, cx)
    }
}

mod empty_status;
mod grid;
mod grouped_delegate;
mod list_cell;
mod list_delegate;
mod render;
mod thumb_row;

use empty_status::*;
use grid::*;
use grouped_delegate::*;
pub(crate) use list_cell::*;
use list_delegate::*;
use thumb_row::*;
