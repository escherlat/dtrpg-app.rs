//! Expanded detail tab content: full item metadata and actions, filling a
//! tab's content area (opened by double-clicking a catalog item).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dtrpg_core::data::enums::ItemStatus;
use dtrpg_core::data::library::{LibraryItem, LibraryItemFile};
use dtrpg_core::data::storage::StorageConfig;
use dtrpg_core::platform::{Platform, PlatformIntegration, PlatformKind};
use dtrpg_core::util::matching::{collection_member_id, member_ids_contain};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, Entity, Image, InteractiveElement, IntoElement, ObjectFit,
    ParentElement, Pixels, SharedString, StatefulInteractiveElement, Styled, StyledImage, Window,
    div, img, px,
};
use gpui_kit::component::Disableable;
use gpui_kit::component::ElementExt as _;
use gpui_kit::component::Icon;
use gpui_kit::component::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::Size;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::collapsible::Collapsible;
use gpui_kit::component::description_list::{DescriptionItem, DescriptionList};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::table::{Column, DataTable, TableDelegate, TableState};
use gpui_kit::component::text::Text;
use gpui_kit::component::tooltip::Tooltip;
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::controllers::tabs::{TabsController, item_list_table};
use crate::data::theme::{ColorTokens, LibriTheme};
use crate::ui::library::cover::{cover_style, render_generative_cover};
use crate::ui::views::catalog_view::render_checking_indicator;
use crate::ui::views::manage_collections_dialog::open_manage_collections_dialog;
use crate::ui::views::tags_section_view::render_tags_section;
use crate::ui::widgets::{selectable_text, small_caps_text};
use crate::util::datetime::{format_absolute, format_relative};
use crate::util::file_presence::resolved_item_dir;
use crate::util::reveal::reveal_in_file_manager;
use crate::view_models::inspector::{
    InspectorContentsState, InspectorPreviewState, InspectorTab, InspectorViewModel,
};

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
    let inspector_tab = entity.read(cx).inspector_tab_for(&item.id);
    if inspector_tab != InspectorTab::Overview {
        return render_secondary_inspector_tab(&item,
                                              inspector_tab,
                                              InspectorRenderContext { storage_root_path:
                                                                           &storage_root_path,
                                                                       entity,
                                                                       tabs,
                                                                       colors,
                                                                       window,
                                                                       cx });
    }
    let inspector = InspectorViewModel::new(&item);
    let snapshot = inspector.snapshot();
    let is_checking = entity.read(cx).is_checking(&item.id);
    let is_verifying = entity.read(cx).is_verifying_downloads(&item.id);
    let entity_download = entity.clone();
    let entity_refresh_thumbnail = entity.clone();
    let entity_other_details = entity.clone();
    let entity_collections = entity.clone();
    let entity_tags = entity.clone();
    let entity_tabs = entity.clone();
    let inspector_entry_id = Arc::clone(&item.id);
    let entity_item_tier = entity;
    let item_id = Arc::clone(&item.id);
    let is_downloaded = item.status == ItemStatus::Downloaded;
    let is_offloaded = item.is_offloaded();
    let download_title = item.title.to_string();

    let cover_w = dtrpg_core::data::constants::DETAIL_PANEL_COVER_MAX_WIDTH * 1.5;
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
            div().flex_1()
                   .min_h_0()
                   .flex()
                   .flex_col()
                   .child(render_inspector_tab_bar(inspector_entry_id,
                                                   entity_tabs,
                                                   inspector_tab,
                                                   colors))
                   .child(div()
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
                                selectable_text("detail-tab-publisher", snapshot.publisher.to_string())
                                    .text_xs()
                                    .text_color(colors.text_tertiary),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        selectable_text("detail-tab-title", snapshot.title.to_string())
                                            .text_xl()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(text_primary),
                                    )
                                    .child(render_status_icon(is_downloaded, is_offloaded, is_verifying,
                                                              text_secondary))
                                    .children(render_checking_indicator(is_checking)),
                            )
                            .child(
                                selectable_text("detail-tab-line", snapshot.line.to_string())
                                    .text_sm()
                                    .text_color(text_secondary),
                            ),
                    )
                    .child(
                        selectable_text("detail-tab-desc", snapshot.description.to_string())
                            .text_sm()
                            .text_color(text_secondary),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child({
                                let read_path = resolved_item_dir(&StorageConfig::load(), &item);
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
                                    dtrpg_core::data::storage::publisher_dir(&storage_root_path,
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
                    .child(render_tags_section(&item, entity_tags, colors, cx))
                    .child(render_collections_section(&item, entity_collections, colors, cx))
                    .child(render_other_details(&item, entity_other_details, colors, cx)),
            ),
        )
        .into_any_element()
}

mod inspector;
mod item_list_delegate;
mod item_tier;
mod metadata_table;
mod other_details;

use inspector::*;
pub(crate) use item_list_delegate::*;
use item_tier::*;
use metadata_table::*;
use other_details::*;
