//! Expanded detail tab content: full item metadata and actions, filling a
//! tab's content area (opened by double-clicking a catalog item).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, Image, InteractiveElement, IntoElement,
    MouseMoveEvent, ObjectFit, ParentElement, Pixels, SharedString, StatefulInteractiveElement,
    Styled, StyledImage, Window, div, img, px,
};
use gpui_component::Disableable;
use gpui_component::ElementExt as _;
use gpui_component::Icon;
use gpui_component::IconName;
use gpui_component::Sizable;
use gpui_component::Size;
use gpui_component::WindowExt as _;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::clipboard::Clipboard;
use gpui_component::collapsible::Collapsible;
use gpui_component::description_list::{DescriptionItem, DescriptionList};
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::spinner::Spinner;
use gpui_component::table::{Column, DataTable, TableDelegate, TableState};
use gpui_component::text::Text;
use gpui_component::tooltip::Tooltip;
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::controllers::tabs::{TabsController, item_list_table};
use crate::data::enums::ItemStatus;
use crate::data::library::{LibraryItem, LibraryItemFile};
use crate::data::theme::{ColorTokens, LibriTheme};
use crate::ui::library::cover::{cover_style, render_generative_cover};
use crate::ui::views::catalog_view::render_checking_indicator;
use crate::ui::views::manage_collections_dialog::open_manage_collections_dialog;
use crate::ui::widgets::{selectable_text, small_caps_text};
use crate::util::datetime::{format_absolute, format_relative};
use crate::util::matching::{collection_member_id, member_ids_contain};
use crate::util::reveal::reveal_in_file_manager;

/// Renders the expanded detail tab's content: a large cover, title,
/// description, actions, and metadata, filling the tab's content area.
///
/// Has no absolute positioning, resize handle, or close button of its own —
/// it's opened as a full tab (double-click on a catalog item, see
/// `main-window-tabs`) and closed via the tab strip.
///
/// For multi-item entries (`item.is_multi_item()`), renders a persistent
/// item list alongside the entry tier; selecting a row updates the item
/// metadata area in place (see `catalog-entry-detail-view`). Single-item
/// entries keep the existing inline entry-tier metadata table.
#[allow(clippy::too_many_arguments)]
pub fn render_detail_tab_content(item: &LibraryItem, storage_root_path: PathBuf,
                                 entity: Entity<LibraryController>,
                                 tabs: &Entity<TabsController>, colors: &ColorTokens,
                                 cover_image: Option<Arc<Image>>, window: &mut Window,
                                 cx: &mut App)
                                 -> AnyElement {
    let surface = colors.surface;
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;
    let label_font_family = cx.global::<LibriTheme>().fonts.label_font.clone();
    let value_font_family = cx.global::<LibriTheme>().fonts.value_font.clone();

    let item = item.clone();
    let is_checking = entity.read(cx).is_checking(&item.id);
    let is_verifying = entity.read(cx).is_verifying_downloads(&item.id);
    let entity_download = entity.clone();
    let entity_refresh_thumbnail = entity.clone();
    let entity_other_details = entity.clone();
    let entity_collections = entity.clone();
    let entity_item_tier = entity;
    let item_id = Arc::clone(&item.id);
    let is_downloaded = item.status == ItemStatus::Downloaded;
    let download_title = item.title.to_string();

    let cover_w = crate::data::constants::DETAIL_PANEL_COVER_MAX_WIDTH * 1.5;
    let cover_h = cover_w * 10.0 / 7.0;
    let cover: AnyElement = if let Some(image) = cover_image {
        img(image).w(px(cover_w))
                  .h(px(cover_h))
                  .object_fit(ObjectFit::Cover)
                  .into_any_element()
    }
    else {
        render_generative_cover(&item, cover_w, cover_h, true).into_any_element()
    };
    let has_cover = item.detail_cover_url.is_some() || item.cover_url.is_some();
    let refresh_item_id = Arc::clone(&item_id);

    div()
        .id("detail-tab-content")
        .flex_1()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_row()
        .bg(surface)
        .child({
            let mut cover_box =
                div().relative().w(px(cover_w)).flex_none().overflow_hidden().child(cover);
            if has_cover {
                cover_box = cover_box.child(
                    div()
                        .id("detail-tab-refresh-thumbnail")
                        .absolute()
                        .top(px(8.0))
                        .left(px(8.0))
                        .size(px(24.0))
                        .rounded_full()
                        .bg(colors.scrim)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_sm()
                        .text_color(colors.accent_on)
                        .tooltip(|window, cx| {
                            Tooltip::new(t!("detail.refresh_thumbnail_tooltip").to_string())
                                .build(window, cx)
                        })
                        .on_click(move |_, _, cx| {
                            entity_refresh_thumbnail.update(cx, |ctrl, cx| {
                                                         ctrl.refresh_detail_cover(
                                                             Arc::clone(&refresh_item_id),
                                                             cx,
                                                         );
                                                     });
                        })
                        .child("\u{27f3}"),
                );
            }
            div().flex_none().pl(px(20.0)).pr(px(16.0)).py(px(16.0)).child(cover_box)
        })
        .child(
            div().flex_1().min_h_0().flex().flex_col().child(
                div()
                    .overflow_y_scrollbar()
                    .p(px(20.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                selectable_text("detail-tab-publisher", item.publisher.to_string())
                                    .text_xs()
                                    .text_color(colors.text_tertiary),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        selectable_text("detail-tab-title", item.title.to_string())
                                            .text_xl()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(text_primary),
                                    )
                                    .child(render_status_icon(is_downloaded, is_verifying,
                                                              text_secondary))
                                    .children(render_checking_indicator(is_checking)),
                            )
                            .child(
                                selectable_text("detail-tab-line", item.line.to_string())
                                    .text_sm()
                                    .text_color(text_secondary),
                            ),
                    )
                    .child(
                        selectable_text("detail-tab-desc", item.desc.to_string())
                            .text_sm()
                            .text_color(text_secondary),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child({
                                let read_path =
                                    crate::data::storage::publisher_dir(&storage_root_path,
                                                                        &item.publisher);
                                Button::new("detail-tab-read")
                                    .primary()
                                    .icon(IconName::BookOpen)
                                    .disabled(!is_downloaded)
                                    .when(!is_downloaded, |b| {
                                        b.tooltip(format!("{}\n{}",
                                            t!("detail.read_button"),
                                            t!("detail.tooltip_download_first")))
                                    })
                                    .when(is_downloaded, |b| {
                                        let read_item = item.clone();
                                        b.tooltip(t!("detail.read_button")).on_click(
                                            move |_, _, _| {
                                                use crate::util::item_opener::{
                                                    ItemOpener, OpenError,
                                                };

                                                match ItemOpener::open_item(&read_path,
                                                                            &read_item.files)
                                                {
                                                    Ok(()) => {}
                                                    // The item list is already rendered in this
                                                    // same tab (see `render_item_tier`) — no
                                                    // further navigation needed, see
                                                    // `catalog-entry-detail-view`.
                                                    Err(OpenError::MultipleFilesRequireSelection) => {}
                                                    Err(OpenError::FileNotFound(path)) => {
                                                        tracing::warn!(
                                                            "open: file not found: {path}"
                                                        );
                                                    }
                                                    Err(OpenError::NoDefaultApp) => {
                                                        tracing::warn!(
                                                            "open: no default application configured"
                                                        );
                                                    }
                                                    Err(OpenError::OsFailed(msg)) => {
                                                        tracing::warn!("open: OS failed: {msg}");
                                                    }
                                                }
                                            },
                                        )
                                    })
                            })
                            .child(
                                Button::new("detail-tab-download")
                                    .ghost()
                                    .outline()
                                    .disabled(is_verifying)
                                    .loading(is_verifying)
                                    .icon(if is_downloaded {
                                        IconName::CircleCheck
                                    } else {
                                        IconName::ArrowDown
                                    })
                                    .tooltip(if is_verifying {
                                        t!("detail.tooltip_verifying_download")
                                    } else if is_downloaded {
                                        t!("detail.downloaded_button")
                                    } else {
                                        t!("detail.download_button")
                                    })
                                    .on_click(move |_, window, cx| {
                                        let id = Arc::clone(&item_id);
                                        if is_downloaded {
                                            let entity_download = entity_download.clone();
                                            let remove_title = download_title.clone();
                                            window.open_alert_dialog(cx, move |alert, _, _| {
                                                let entity_download = entity_download.clone();
                                                let id = Arc::clone(&id);
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
                                                        entity_download.update(cx, |ctrl, cx| {
                                                                          ctrl.remove_download(&id, cx)
                                                                      });
                                                        true
                                                    })
                                            });
                                        }
                                        else {
                                            entity_download.update(cx, |ctrl, cx| {
                                                ctrl.request_download(&id, download_title.clone(), cx);
                                            });
                                        }
                                    }),
                            )
                            .when(is_downloaded, |row| {
                                let item_path =
                                    crate::data::storage::publisher_dir(&storage_root_path,
                                                                        &item.publisher);
                                row.child(
                                    Button::new("detail-tab-reveal")
                                        .ghost()
                                        .outline()
                                        .icon(IconName::FolderOpen)
                                        .tooltip(platform_reveal_label().into_owned())
                                        .on_click(move |_, _, _cx| {
                                            if !item_path.exists() {
                                                tracing::warn!(
                                                    path = %item_path.display(),
                                                    "reveal: file not found — item may need re-download"
                                                );
                                                return;
                                            }
                                            if let Err(e) = reveal_in_file_manager(&item_path) {
                                                tracing::warn!(
                                                    "reveal_in_file_manager failed: {e}"
                                                );
                                            }
                                        }),
                                )
                            }),
                    )
                    .child(render_metadata_table(&item,
                                                 &storage_root_path,
                                                 &label_font_family,
                                                 &value_font_family))
                    // Always shown when the entry has file metadata — not just for
                    // multi-item entries — so a single-file entry whose one file is itself
                    // a Zip archive still gets a row to hover/click for content preview
                    // (see `detail-file-list`, `zip-content-preview`). Cache entries written
                    // before `LibraryItem::files` existed have an empty list and render no
                    // row here.
                    .when(!item.files.is_empty(), |this| {
                        this.child(render_item_tier(&item, &storage_root_path,
                                                    entity_item_tier.clone(), tabs, colors, window,
                                                    cx))
                    })
                    .child(render_collections_section(&item, entity_collections, colors, cx))
                    .child(render_other_details(&item, entity_other_details, colors, cx)),
            ),
        )
        .into_any_element()
}

/// Returns the column definitions used for the multi-item entry's item list.
///
/// Name's `width(240.)` is only the fallback default before the table's
/// rendered width is known; once known, [`ItemListDelegate::column`]
/// recomputes it each render to fill whatever space Type/Size/Status don't
/// use, so Name absorbs panel resizes while the other three stay narrow and
/// right-aligned as a group. All four are still user-resizable — see
/// `detail-item-list-column-resize`.
pub(crate) fn item_list_columns() -> Vec<Column> {
    vec![Column::new("name", t!("detail.item_list_column_name")).width(240.)
                                                                .min_width(120.)
                                                                .resizable(true),
         Column::new("type", t!("detail.item_list_column_type")).width(90.)
                                                                .min_width(60.)
                                                                .resizable(true),
         Column::new("size", t!("detail.item_list_column_size")).width(90.)
                                                                .min_width(60.)
                                                                .resizable(true),
         Column::new("info", t!("detail.item_list_column_info")).width(100.)
                                                                .min_width(60.)
                                                                .resizable(true),
         Column::new("status", t!("detail.item_list_column_status")).width(90.)
                                                                    .min_width(60.)
                                                                    .resizable(true),]
}

/// `TableDelegate` for a multi-item entry's item list. Backed by
/// `LibraryController`, reading `item.files` live via `item_by_id` rather
/// than cloning the file list, mirroring `CatalogListDelegate`'s
/// read-through-controller pattern.
pub(crate) struct ItemListDelegate {
    pub(crate) controller:  Entity<LibraryController>,
    pub(crate) entry_id:    Arc<str>,
    pub(crate) columns:     Vec<Column>,
    /// User-adjusted column widths. `None` means use the static default from
    /// `item_list_columns()`.
    pub(crate) user_widths: Vec<Option<Pixels>>,
    /// The entry's on-disk download directory, used to resolve each file's
    /// on-disk size suffix in the Size column (see `render_metadata_table`'s
    /// equivalent computation).
    pub(crate) entry_dir:   PathBuf,
    /// Current rendered width of the item list table, captured by
    /// `render_item_tier`'s `on_prepaint` hook. Drives the Name column's
    /// fill-remaining-width sizing in [`Self::column`] — `gpui-component`'s
    /// `Column` has no built-in flex/stretch behavior, so Name's width is
    /// recomputed each time this changes rather than left fixed.
    pub(crate) table_width: Option<Pixels>,
}

impl TableDelegate for ItemListDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, cx: &App) -> usize {
        self.controller
            .read(cx)
            .item_by_id(&self.entry_id)
            .map_or(0, |item| item.files.len())
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        let mut col = self.columns[col_ix].clone();
        if let Some(Some(w)) = self.user_widths.get(col_ix) {
            col = col.width(w.as_f32());
        }
        else if col_ix == 0
                  && let Some(table_width) = self.table_width
        {
            // Name has no user override yet: fill whatever width
            // Type/Size/Status don't use, so those three stay
            // narrow and pushed to the right instead of leaving
            // blank trailing space when the panel is wider than the
            // four columns' fixed defaults.
            let others: f32 = self.columns[1..].iter().map(|c| c.width.as_f32()).sum();
            let name_width = (table_width.as_f32() - others).max(col.min_width.as_f32());
            col = col.width(name_width);
        }
        col
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        let Some(item) = self.controller.read(cx).item_by_id(&self.entry_id).cloned()
        else {
            return div().into_any_element();
        };
        let Some(file) = item.files.get(row_ix)
        else {
            return div().into_any_element();
        };
        let colors = cx.global::<LibriTheme>().colors.clone();
        let value_font_family = cx.global::<LibriTheme>().fonts.value_font.clone();

        match col_ix {
            // Name column. Zip-typed files (see `zip-content-preview`) additionally get a
            // folder-glyph affordance and hover/click wiring for the content preview
            // popover — moving the mouse over the row opens it, leaving without clicking
            // closes it, and a click pins it open (a second click unpins).
            0 => {
                let is_zip = file.format.eq_ignore_ascii_case("zip");
                let name_cell = div().h_full()
                                     .flex()
                                     .items_center()
                                     .gap(px(4.0))
                                     .text_sm()
                                     .font_family(value_font_family.to_string())
                                     .text_color(colors.text_primary)
                                     .child(div().flex_1()
                                                 .min_w_0()
                                                 .truncate()
                                                 .child(file.name.to_string()));

                if !is_zip {
                    return name_cell.into_any_element();
                }

                let controller_move = self.controller.clone();
                let entry_id_move = Arc::clone(&self.entry_id);
                let controller_hover_end = self.controller.clone();
                let entry_id_hover_end = Arc::clone(&self.entry_id);
                let controller_click = self.controller.clone();
                let entry_id_click = Arc::clone(&self.entry_id);

                name_cell
                    .id(SharedString::from(format!("item-row-{row_ix}-zip-name")))
                    .cursor_pointer()
                    .child(div().flex_none()
                               .child(Icon::new(IconName::FolderOpen)
                                          .text_color(colors.text_tertiary)))
                    .on_mouse_move(move |event: &MouseMoveEvent, _window, cx| {
                        controller_move.update(cx, |ctrl, cx| {
                                            ctrl.hover_zip_preview(Arc::clone(&entry_id_move),
                                                                   row_ix,
                                                                   event.position,
                                                                   cx);
                                        });
                    })
                    .on_hover(move |hovered: &bool, _window, cx| {
                        if !hovered {
                            controller_hover_end.update(cx, |ctrl, cx| {
                                                     ctrl.clear_hover_zip_preview(
                                                         &entry_id_hover_end,
                                                         row_ix,
                                                         cx,
                                                     );
                                                 });
                        }
                    })
                    .on_click(move |event: &ClickEvent, _window, cx| {
                        controller_click.update(cx, |ctrl, cx| {
                                             ctrl.toggle_zip_preview_pin(Arc::clone(
                                                                             &entry_id_click,
                                                                         ),
                                                                         row_ix,
                                                                         event.position(),
                                                                         cx);
                                         });
                    })
                    .into_any_element()
            }
            1 => div().h_full()
                      .flex()
                      .items_center()
                      .text_sm()
                      .font_family(value_font_family.to_string())
                      .text_color(colors.text_secondary)
                      .truncate()
                      .child(file.format.to_string())
                      .into_any_element(),
            2 => {
                let catalog_str = format!("{:.1} {}", file.size_mb, t!("size.mb"));
                let on_disk =
                    crate::util::file_size::on_disk_file_size(&self.entry_dir, file.name.as_ref());
                div().h_full()
                     .flex()
                     .items_center()
                     .text_sm()
                     .font_family(value_font_family.to_string())
                     .text_color(colors.text_secondary)
                     .child(crate::util::file_size::prefer_on_disk_size(catalog_str, on_disk))
                     .into_any_element()
            }
            3 => {
                let entry_id = Arc::clone(&self.entry_id);
                let entry_dir = self.entry_dir.clone();
                let info = self.controller.update(cx, |ctrl, _cx| {
                                              ctrl.file_info(&entry_id, row_ix, &entry_dir)
                                          });
                let text = info.display().unwrap_or_else(|| "\u{2014}".to_string());
                div().h_full()
                     .flex()
                     .items_center()
                     .text_sm()
                     .font_family(value_font_family.to_string())
                     .text_color(colors.text_secondary)
                     .truncate()
                     .child(text)
                     .into_any_element()
            }
            4 => {
                // Status: downloaded checkmark, in-progress indicator, or a
                // download action for this specific item — independent of
                // sibling rows and of the entry-level download button (see
                // `queue-per-item-downloads`). Cancelling an in-progress
                // download is done from the activity panel rather than a
                // second cancel control here.
                let is_downloading = self.controller
                                         .read(cx)
                                         .is_file_queued_or_active(&self.entry_id, row_ix as u32);
                let is_verifying = self.controller
                                       .read(cx)
                                       .is_verifying_downloads(&self.entry_id);

                if is_verifying {
                    div().h_full()
                         .flex()
                         .items_center()
                         .id(SharedString::from(format!("item-row-{row_ix}-verifying")))
                         .tooltip(move |window, cx| {
                             Tooltip::new(t!("detail.tooltip_verifying_download").to_string())
                                .build(window, cx)
                         })
                         .child(Spinner::new().with_size(Size::XSmall)
                                              .color(colors.text_tertiary))
                         .into_any_element()
                }
                else if file.downloaded {
                    div().h_full()
                         .flex()
                         .items_center()
                         .text_sm()
                         .text_color(colors.text_secondary)
                         .id(SharedString::from(format!("item-row-{row_ix}-status")))
                         .tooltip(move |window, cx| {
                             Tooltip::new(t!("detail.item_status_local").to_string()).build(window,
                                                                                            cx)
                         })
                         // Checkmark for "on this device", matching
                         // `render_status_icon`'s glyph convention rather than a
                         // separate `IconName` so both status indicators read the
                         // same way.
                         .child("\u{2713}")
                         .into_any_element()
                }
                else if is_downloading {
                    div().h_full()
                         .flex()
                         .items_center()
                         .text_sm()
                         .text_color(colors.text_tertiary)
                         .child(t!("detail.item_status_downloading").to_string())
                         .into_any_element()
                }
                else {
                    let controller = self.controller.clone();
                    let entry_id = Arc::clone(&self.entry_id);
                    let title = self.controller
                                    .read(cx)
                                    .item_by_id(&self.entry_id)
                                    .map(|item| item.title.to_string())
                                    .unwrap_or_default();
                    div().h_full()
                         .flex()
                         .items_center()
                         .text_sm()
                         .text_color(colors.text_secondary)
                         .id(SharedString::from(format!("item-row-{row_ix}-download")))
                         .cursor_pointer()
                         .tooltip(move |window, cx| {
                             Tooltip::new(t!("detail.item_download_button").to_string())
                                .build(window, cx)
                         })
                         // Cloud glyph for "in the cloud, not yet on this device" —
                         // an arrow-down here previously read as an already-completed
                         // download indicator rather than a pending one.
                         .child("\u{2601}")
                         // Stop propagation so clicking download doesn't also fire the
                         // row's own click (which selects this row) — same class of bug
                         // fixed in the activity panel's cancel button.
                         .on_click(move |_, _, cx| {
                             cx.stop_propagation();
                             controller.update(cx, |ctrl, cx| {
                                           ctrl.request_item_download(&entry_id,
                                                                      row_ix as u32,
                                                                      title.clone(),
                                                                      cx);
                                       });
                         })
                         .into_any_element()
                }
            }
            _ => div().into_any_element(),
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
             .font_weight(gpui::FontWeight::MEDIUM)
             .child(name)
    }
}

/// Renders the item tier for a multi-item entry: a persistent, selectable
/// item list and an item metadata area that updates in place on selection.
///
/// See `catalog-entry-detail-view`'s persistent-item-list and
/// update-in-place requirements.
#[allow(clippy::too_many_arguments)]
fn render_item_tier(item: &LibraryItem, storage_root_path: &Path,
                    entity: Entity<LibraryController>, tabs: &Entity<TabsController>,
                    colors: &ColorTokens, window: &mut Window, cx: &mut App)
                    -> impl IntoElement + 'static {
    let entry_id = Arc::clone(&item.id);
    let selected_ix = entity.read(cx).selected_item_file(&entry_id);
    let entry_dir = crate::data::storage::publisher_dir(storage_root_path, &item.publisher);

    let table = item_list_table(tabs, &entity, &entry_id, entry_dir, window, cx);
    let table_for_prepaint = table.clone();

    // `DataTable` virtualizes its rows and sizes itself to its container
    // (`.size_full()` internally) — unlike the old stateless `Table`, it
    // renders nothing if given an unbounded height, which is what the
    // surrounding naturally-scrolling detail-tab content provides. Give it
    // an explicit height sized to the row count (capped so a very large
    // bundle scrolls internally instead of pushing the rest of the tab off
    // screen).
    let row_height = Size::default().table_row_height();
    let visible_rows = item.files.len().min(8) as f32;
    let table_height = row_height * (visible_rows + 1.0);
    let item_list =
        div().h(table_height)
             .child(DataTable::new(&table).bordered(false)
                                          .scrollbar_visible(true, false))
             // Captures the table's rendered width so the Name column can
             // fill whatever space Type/Size/Status don't use — see
             // `ItemListDelegate::table_width`. Mirrors
             // `LibraryController::set_entry_bounds`'s guarded-update
             // pattern so an unchanged width doesn't re-notify every frame.
             .on_prepaint(move |bounds, _window, cx| {
                 table_for_prepaint.update(cx, |state, cx| {
                                       if state.delegate().table_width != Some(bounds.size.width) {
                                           state.delegate_mut().table_width =
                                               Some(bounds.size.width);
                                           cx.notify();
                                       }
                                   });
             });

    let selected_file = selected_ix.and_then(|ix| item.files.get(ix).map(|file| (ix, file)));

    // Only ever `Some` while this entry's detail tab is active — inactive
    // detail tabs never call `render_item_tier` at all (see `root_view`'s
    // match on the active `TabTarget`), so the popover can't leak into
    // another tab without extra gating here.
    let zip_popover: AnyElement = match entity.read(cx).zip_preview_for(&entry_id) {
        Some((row_ix, anchor_pos, _pinned)) => match item.files.get(row_ix) {
            Some(file) => {
                let file_path =
                    crate::data::storage::publisher_dir(storage_root_path, &item.publisher)
                        .join(file.name.as_ref());
                crate::ui::views::zip_preview_popover::render_zip_preview_popover(
                    Arc::clone(&entry_id),
                    &file_path,
                    anchor_pos,
                    entity.clone(),
                    colors,
                )
            }
            None => div().into_any_element(),
        },
        None => div().into_any_element(),
    };

    div().flex()
         .flex_col()
         .gap(px(12.0))
         .child(div().text_sm()
                     .font_weight(gpui::FontWeight::SEMIBOLD)
                     .text_color(colors.text_primary)
                     .child(t!("detail.items_heading").to_string()))
         .child(div().text_sm()
                     .text_color(colors.text_tertiary)
                     .child(t!("detail.item_prompt_select").to_string())
                     .into_any_element())
         .child(item_list)
         .child(match selected_file {
                    Some((ix, file)) => render_item_metadata(item,
                                                             file,
                                                             ix,
                                                             storage_root_path,
                                                             entity.clone(),
                                                             colors,
                                                             cx).into_any_element(),
                    None => div().into_any_element(),
                })
         .child(zip_popover)
}

/// Renders the metadata area for a single selected item within a multi-item
/// entry: name, type, format, file size, the entry's download state
/// (individual files share the entry's on-disk download state today; see
/// `define-rust-catalog-entry-detail-view`'s open questions for future
/// per-file tracking), and a per-file "Other details" disclosure (file id,
/// download location).
///
/// `row_ix` is the file's position within the entry's `files` list — used to
/// key element ids and disclosure state, since the API has been observed to
/// reuse the same download id across genuinely distinct files within a
/// bundle (see `LibraryItem::dedupe_files`).
fn render_item_metadata(item: &LibraryItem, file: &LibraryItemFile, row_ix: usize,
                        storage_root_path: &Path, entity: Entity<LibraryController>,
                        colors: &ColorTokens, cx: &App)
                        -> impl IntoElement + 'static {
    let label_font_family = cx.global::<LibriTheme>().fonts.label_font.clone();
    let value_font_family = cx.global::<LibriTheme>().fonts.value_font.clone();
    let name_value = copyable_value(SharedString::from(format!("file-name-{row_ix}")),
                                    file.name.to_string(),
                                    &value_font_family);

    let file_size_value = {
        let entry_dir = crate::data::storage::publisher_dir(storage_root_path, &item.publisher);
        let on_disk = crate::util::file_size::on_disk_file_size(&entry_dir, file.name.as_ref());
        crate::util::file_size::prefer_on_disk_size(format!("{:.1} {}",
                                                            file.size_mb,
                                                            t!("size.mb")),
                                                    on_disk)
    };

    let metadata = DescriptionList::vertical()
        .columns(2)
        .bordered(false)
        .child(DescriptionItem::new(styled_label(t!("detail.item_list_column_name").to_string(),
                                                 &label_font_family))
                   .value(name_value)
                   .span(2))
        .child(DescriptionItem::new(styled_label(t!("detail.field_format").to_string(),
                                                 &label_font_family))
                   .value(styled_value(file.format.to_string(), &value_font_family)))
        .child(DescriptionItem::new(styled_label(t!("detail.field_file_size").to_string(),
                                                 &label_font_family))
                   .value(styled_value(file_size_value, &value_font_family)))
        // .child(DescriptionItem::new(t!("detail.field_status").to_string())
        //            .value(if item.status == ItemStatus::Downloaded {
        //                t!("detail.status_on_device").to_string()
        //            } else {
        //                t!("detail.status_in_cloud").to_string()
        //            })
        //            .span(2))
    ;

    div().flex()
         .flex_col()
         .gap(px(12.0))
         .child(metadata)
         .child(render_file_other_details(FileOtherDetailsContext { entry_id: &item.id,
                                                                    publisher: &item.publisher,
                                                                    row_ix,
                                                                    is_downloaded:
                                                                        file.downloaded,
                                                                    storage_root_path },
                                          file,
                                          entity,
                                          colors,
                                          cx))
}

/// Builds the shared toggle-state key for a single file's "Other details"
/// disclosure within a multi-item entry's item tier.
fn file_other_details_key(entry_id: &str, row_ix: usize) -> Arc<str> {
    Arc::from(format!("{entry_id}:{row_ix}"))
}

/// Grouped context for [`render_file_other_details`], kept below Rust's
/// preferred argument count.
struct FileOtherDetailsContext<'a> {
    entry_id:          &'a str,
    publisher:         &'a str,
    row_ix:            usize,
    is_downloaded:     bool,
    storage_root_path: &'a Path,
}

/// Renders a single file's "Other details" disclosure section: a
/// clickable header that toggles a collapsed-by-default panel showing the
/// file's id and its on-disk download location.
///
/// The download location is the entry's shared item folder — per-file paths
/// aren't tracked separately (see `LibraryItemFile`), so every file in a
/// bundle reports the same location.
fn render_file_other_details(ctx: FileOtherDetailsContext<'_>, file: &LibraryItemFile,
                             entity: Entity<LibraryController>, colors: &ColorTokens, cx: &App)
                             -> impl IntoElement + 'static {
    let FileOtherDetailsContext { entry_id,
                                  publisher,
                                  row_ix,
                                  is_downloaded,
                                  storage_root_path, } = ctx;
    let toggle_key = file_other_details_key(entry_id, row_ix);
    let open = entity.read(cx).is_file_other_details_open(&toggle_key);
    let label_font_family = cx.global::<LibriTheme>().fonts.label_font.clone();
    let value_font_family = cx.global::<LibriTheme>().fonts.value_font.clone();

    let toggle_entity = entity;
    let toggle_key_for_click = Arc::clone(&toggle_key);

    let header = div().id(SharedString::from(format!("file-other-details-header-{toggle_key}")))
                      .flex()
                      .items_center()
                      .gap(px(6.0))
                      .cursor_pointer()
                      .text_sm()
                      .font_weight(gpui::FontWeight::SEMIBOLD)
                      .text_color(colors.text_primary)
                      .on_click(move |_, _, cx| {
                          toggle_entity.update(cx, |ctrl, cx| {
                              ctrl.toggle_file_other_details(Arc::clone(&toggle_key_for_click), cx);
                          });
                      })
                      .child(Icon::new(if open {
                                           IconName::ChevronUp
                                       }
                                       else {
                                           IconName::ChevronDown
                                       }).text_color(colors.text_secondary))
                      .child(t!("detail.other_details_heading").to_string());

    let path_value = if is_downloaded {
        crate::data::storage::publisher_dir(storage_root_path, publisher).display()
                                                                         .to_string()
    }
    else {
        value_or_dash("")
    };

    let content = DescriptionList::vertical()
        .columns(2)
        .bordered(false)
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_file_id").to_string(),
                                              &label_font_family))
                .value(copyable_value(
                    SharedString::from(format!("file-id-{toggle_key}")),
                    file.id.to_string(),
                    &value_font_family,
                ))
                .span(2),
        )
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_download_location").to_string(),
                                              &label_font_family))
                .value(copyable_value(
                    SharedString::from(format!("file-path-{toggle_key}")),
                    path_value,
                    &value_font_family,
                ))
                .span(2),
        );

    Collapsible::new().gap(px(8.0))
                      .open(open)
                      .child(header)
                      .content(content)
}

/// Renders the "Other details" disclosure section: a clickable header that
/// toggles a collapsed-by-default panel of fields not already shown in the
/// primary metadata table or item tier (stable id, numeric id, order product
/// id, product id, added-order value, and the generative cover color).
///
/// See `catalog-entry-detail-advanced-disclosure`.
fn render_other_details(item: &LibraryItem, entity: Entity<LibraryController>,
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
            DescriptionItem::new(styled_label(t!("detail.field_stable_id").to_string(),
                                              &label_font_family))
                .value(copyable_value(
                    SharedString::from("other-details-stable-id"),
                    item.id.to_string(),
                    &value_font_family,
                )),
        )
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_numeric_id").to_string(),
                                              &label_font_family))
                .value(copyable_value(
                    SharedString::from("other-details-numeric-id"),
                    item.numeric_id.to_string(),
                    &value_font_family,
                )),
        )
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_order_product_id").to_string(),
                                              &label_font_family))
                .value(copyable_value(
                    SharedString::from("other-details-order-product-id"),
                    item.order_product_id.to_string(),
                    &value_font_family,
                )),
        )
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_product_id").to_string(),
                                              &label_font_family))
                .value(copyable_value(
                    SharedString::from("other-details-product-id"),
                    item.product_id.to_string(),
                    &value_font_family,
                )),
        )
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_added_order").to_string(),
                                              &label_font_family))
                .value(styled_value(item.added_order.to_string(), &value_font_family)),
        )
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_cover_color").to_string(),
                                              &label_font_family))
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
fn render_collections_section(item: &LibraryItem, entity: Entity<LibraryController>,
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

/// Renders a text value with an appear-on-hover copy button, in the app's
/// dedicated value font.
///
/// `field_id` must be unique within the surrounding view — it doubles as the
/// hover group name and the copy button's element id.
fn copyable_value(field_id: SharedString, value: impl Into<SharedString>,
                  value_font_family: &str)
                  -> AnyElement {
    let value: SharedString = value.into();

    div()
        .id(SharedString::from(format!("{field_id}-row")))
        .group(field_id.clone())
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(
            selectable_text(SharedString::from(format!("{field_id}-text")), value.clone())
                .font_family(value_font_family.to_string()),
        )
        .child(
            div()
                .invisible()
                .group_hover(field_id.clone(), |d| d.visible())
                .child(
                    Clipboard::new(SharedString::from(format!("{field_id}-copy")))
                        .value(value)
                        .tooltip(t!("detail.copy_tooltip").to_string()),
                ),
        )
        .into_any_element()
}

/// Renders a `DescriptionList` label in the app's dedicated label font and in
/// small caps, distinguishing field names from their values (rendered in the
/// dedicated value font — see [`styled_value`]).
fn styled_label(label: impl Into<SharedString>, label_font_family: &str) -> AnyElement {
    div().font_family(label_font_family.to_string())
         .child(small_caps_text(label))
         .into_any_element()
}

/// Renders a `DescriptionList` value in the app's dedicated value font, for
/// plain text values that don't need [`copyable_value`]'s copy button.
fn styled_value(value: impl Into<SharedString>, value_font_family: &str) -> AnyElement {
    div().font_family(value_font_family.to_string())
         .child(value.into())
         .into_any_element()
}

fn platform_reveal_label() -> std::borrow::Cow<'static, str> {
    #[cfg(target_os = "macos")]
    return t!("detail.show_in_finder");
    #[cfg(target_os = "windows")]
    return t!("detail.show_in_explorer");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return t!("detail.show_in_files");
}

/// Renders a small status icon next to the item title: a checkmark when
/// downloaded, a cloud glyph otherwise. Replaces the old text-only "Status" row
/// in the metadata table.
/// Renders the detail tab's title-row status glyph. While `is_verifying` is
/// true (a file-presence check per `verify-downloaded-status-against-disk`
/// is in flight), shows a distinct pending glyph and tooltip rather than a
/// downloaded/cloud state that may be stale.
fn render_status_icon(is_downloaded: bool, is_verifying: bool, color: gpui::Hsla)
                      -> impl IntoElement + 'static {
    let (glyph, tooltip_text) = if is_verifying {
        ("\u{22ef}", t!("detail.tooltip_verifying_download").to_string())
    }
    else if is_downloaded {
        ("\u{2713}", t!("detail.status_on_device").to_string())
    }
    else {
        ("\u{2601}", t!("detail.status_in_cloud").to_string())
    };

    div().id("detail-status-icon")
         .text_sm()
         .text_color(color)
         .tooltip(move |window, cx| Tooltip::new(tooltip_text.clone()).build(window, cx))
         .child(glyph)
}

/// Renders a metadata value, falling back to an em dash when the underlying
/// data is empty (e.g. the API did not report a game system/line for this
/// item).
fn value_or_dash(value: &str) -> String {
    if value.trim().is_empty() {
        "\u{2014}".to_string()
    }
    else {
        value.to_string()
    }
}

fn render_metadata_table(item: &LibraryItem, storage_root_path: &Path, label_font_family: &str,
                         value_font_family: &str)
                         -> impl IntoElement + 'static + use<> {
    let file_size_label = if item.files.len() > 1 {
        t!("detail.field_total_file_size").to_string()
    }
    else {
        t!("detail.field_file_size").to_string()
    };

    // Three display modes depending on how many of the entry's files are
    // downloaded: none → catalog (cloud) size only; all → on-disk (local)
    // size only, since it's the exact figure once every file is present;
    // some → both, so the user can see progress against the total.
    let file_size_value = {
        let catalog_mb: f64 = if item.files.is_empty() {
            item.size_mb
        }
        else {
            item.files.iter().map(|f| f.size_mb).sum()
        };
        let catalog_str = format!("{:.0} {}", catalog_mb, t!("size.mb"));

        if item.files.is_empty() {
            catalog_str
        }
        else {
            let entry_dir = crate::data::storage::publisher_dir(storage_root_path, &item.publisher);
            let downloaded_count = item.files.iter().filter(|f| f.downloaded).count();
            let resolved: Vec<u64> =
                item.files
                    .iter()
                    .filter(|f| f.downloaded)
                    .filter_map(|f| {
                        crate::util::file_size::on_disk_file_size(&entry_dir, f.name.as_ref())
                    })
                    .collect();

            if downloaded_count == 0 {
                catalog_str
            }
            else if downloaded_count == item.files.len() && resolved.len() == item.files.len() {
                crate::util::file_size::format_bytes(resolved.iter().sum())
            }
            else {
                let on_disk_bytes = if resolved.is_empty() {
                    None
                }
                else {
                    Some(resolved.iter().sum())
                };
                crate::util::file_size::with_on_disk_suffix(catalog_str, on_disk_bytes)
            }
        }
    };

    let mut list = DescriptionList::vertical()
        .columns(2)
        .bordered(false)
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_system").to_string(), label_font_family))
                .value(Text::from(selectable_text(
                    "detail-field-system",
                    value_or_dash(&item.line),
                ).font_family(value_font_family.to_string()))),
        )
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_released").to_string(), label_font_family))
                .value(Text::from(selectable_text(
                    "detail-field-released",
                    item.year.to_string(),
                ).font_family(value_font_family.to_string()))),
        )
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_format").to_string(), label_font_family)).value(
                Text::from(selectable_text("detail-field-format", item.format.to_string())
                    .font_family(value_font_family.to_string())),
            ),
        )
        .child(
            DescriptionItem::new(styled_label(file_size_label, label_font_family)).value(
                Text::from(selectable_text("detail-field-file-size", file_size_value)
                    .font_family(value_font_family.to_string())),
            ),
        )
        .child(
            DescriptionItem::new(styled_label(t!("detail.field_category").to_string(),
                                              label_font_family))
                .value(Text::from(selectable_text(
                    "detail-field-kind",
                    item.kind.to_string(),
                ).font_family(value_font_family.to_string())))
                .span(2),
        );

    // The DriveThruRPG order-product API does not always report a page count;
    // omit the row entirely rather than showing a misleading "0".
    if item.pages > 0 {
        list = list.child(
            DescriptionItem::new(styled_label(t!("detail.field_pages").to_string(), label_font_family))
                .value(Text::from(selectable_text(
                    "detail-field-pages",
                    item.pages.to_string(),
                ).font_family(value_font_family.to_string())))
                .span(2),
        );
    }

    if let Some(ts) = item.date_added {
        let value = render_relative_date_value(&item.id, "added", ts, value_font_family);
        list = list.child(DescriptionItem::new(styled_label(t!("detail.field_added").to_string(),
                                                            label_font_family)).value(value)
                                                                               .span(2));
    }

    if let Some(ts) = item.date_updated {
        let value = render_relative_date_value(&item.id, "updated", ts, value_font_family);
        list =
            list.child(DescriptionItem::new(styled_label(t!("detail.field_updated").to_string(),
                                                         label_font_family)).value(value)
                                                                            .span(2));
    }

    list
}

/// Renders a stateful div showing a relative date label, with a tooltip that
/// reveals the absolute human-readable date/time on hover.
///
/// `slot` disambiguates the element id (e.g. `"added"`, `"updated"`) so
/// multiple date rows on the same item don't collide.
fn render_relative_date_value(item_id: &str, slot: &str, ts: i64, value_font_family: &str)
                              -> AnyElement {
    let relative = format_relative(ts);
    let absolute = format_absolute(ts);
    let id = SharedString::from(format!("detail-{slot}-{item_id}"));
    div().id(id)
         .child(selectable_text(SharedString::from(format!("detail-{slot}-{item_id}-text")),
                                relative).font_family(value_font_family.to_string()))
         .tooltip(move |window, cx| Tooltip::new(absolute.clone()).build(window, cx))
         .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_or_dash_passes_through_non_empty_value() {
        assert_eq!(value_or_dash("Pathfinder"), "Pathfinder");
    }

    #[test]
    fn value_or_dash_falls_back_to_em_dash_on_empty_string() {
        assert_eq!(value_or_dash(""), "\u{2014}");
    }

    #[test]
    fn value_or_dash_falls_back_to_em_dash_on_whitespace_only() {
        assert_eq!(value_or_dash("   "), "\u{2014}");
    }
}
