//! Sidebar view: smart nav, publisher nav, collections nav.
//!
//! The app wordmark lives in `title_bar_view`, next to the macOS traffic
//! lights — it is not repeated here.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use dtrpg_core::data::collection::CollectionEntry;
use dtrpg_core::data::library::SectionCounts;
use dtrpg_core::data::tag_library::TagEntry;
use dtrpg_core::util::filter::SidebarFilter;
use dtrpg_core::util::matching::name_matches_query;
use dtrpg_core::util::sort::{CollectionSortMethod, SortDirection};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, ElementId, Entity, IntoElement, SharedString, Window, div, px,
};
use gpui_kit::component::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::{Dialog, DialogButtonProps, DialogHeader, DialogTitle};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{ContextMenuExt, DropdownMenu, PopupMenuItem};
use gpui_kit::component::sidebar::{
    Sidebar, SidebarCollapsible, SidebarItem, SidebarMenu, SidebarMenuItem,
};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{
    ActiveTheme, Collapsible, Side, Sizable as _, Size, StyledExt as _, WindowExt as _,
};
use rust_i18n::t;

use crate::controllers::library::{LibraryController, SidebarOpen, SidebarSection};
use crate::controllers::tabs::{TabTarget, TabsController};
use crate::ui::library::drag::DraggedLibraryItem;
use crate::ui::views::sidebar_tags::tags_menu;
use crate::util::publisher::PublisherEntry;

/// Inline search bar state for a collapsible sidebar section (Publishers or
/// Collections).
///
/// Session-only: the search query is never persisted and is cleared whenever
/// the section's search bar is collapsed.
pub struct SidebarSectionSearch {
    /// Whether the section's search bar is currently expanded.
    pub open:  bool,
    /// Current filter text; empty means no filtering.
    pub query: String,
    /// Backing text input, owned by the root view so its subscription lives
    /// for the lifetime of the window.
    pub input: Entity<InputState>,
}

/// A sidebar child that is either a [`SidebarMenu`], the hand-rolled
/// Collections section, or a thin horizontal divider.
#[derive(Clone)]
enum SidebarContent {
    Menu(Box<SidebarMenu>),
    Collections(Box<CollectionsSection>),
    Separator,
}

impl Collapsible for SidebarContent {
    fn collapsed(self, collapsed: bool) -> Self {
        match self {
            Self::Menu(m) => Self::Menu(Box::new(m.collapsed(collapsed))),
            Self::Collections(c) => Self::Collections(c),
            Self::Separator => Self::Separator,
        }
    }

    fn is_collapsed(&self) -> bool {
        match self {
            Self::Menu(m) => m.is_collapsed(),
            Self::Collections(_) | Self::Separator => false,
        }
    }
}

impl SidebarItem for SidebarContent {
    fn render(self, id: impl Into<ElementId>, window: &mut Window, cx: &mut App)
              -> impl IntoElement {
        match self {
            Self::Menu(m) => m.render(id, window, cx).into_any_element(),
            Self::Collections(c) => c.render(id, window, cx).into_any_element(),
            Self::Separator => div().h(px(1.))
                                    .w_full()
                                    .my_1()
                                    .bg(cx.theme().sidebar_border)
                                    .into_any_element(),
        }
    }
}

// ── Collections section (hand-rolled drop target)
// ──────────────────────────

/// One collection nav row within the Collections section.
#[derive(Clone)]
struct CollectionRow {
    id:        u64,
    name:      Arc<str>,
    count:     usize,
    is_active: bool,
}

/// Builds the Collections section header's trailing suffix element (item
/// count, search toggle, add-collection dialog).
type CollectionsSuffixFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// Toggles the Collections section's persisted open/closed `UiState` state.
type CollectionsToggleFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// The Collections section, rendered as plain `div`s rather than
/// [`SidebarMenuItem`], because `SidebarMenuItem` has no `on_drop`/
/// `drag_over` hooks — its row is built entirely inside the vendored
/// `gpui-kit` crate with no way to wrap or extend it from here. This
/// is the only sidebar section that needs to be a drop target (see
/// `catalog-drag-drop-to-collection`), so only this section forgoes
/// `SidebarMenuItem`; smart-filter and publisher rows are unaffected.
///
/// The open/closed state comes from the controller on every render, so — unlike
/// `SidebarMenuItem`, which tracks its own internal open state — no separate
/// collapse-toggle plumbing is needed here.
#[derive(Clone)]
struct CollectionsSection {
    title:     SharedString,
    open:      bool,
    suffix:    CollectionsSuffixFn,
    on_toggle: CollectionsToggleFn,
    rows:      Vec<CollectionRow>,
    /// Whether the initial collections fetch is still in flight. While
    /// `true` and `rows` is empty, the section shows a loading indicator
    /// instead of a blank submenu.
    loading:   bool,
    entity:    Entity<LibraryController>,
    tabs:      Entity<TabsController>,
}

impl Collapsible for CollectionsSection {
    // The app's `Sidebar` is built with
    // `.collapsible(SidebarCollapsible::None)`, so the icon-only rail state
    // this trait exists for is never reached.
    fn collapsed(self, _collapsed: bool) -> Self {
        self
    }

    fn is_collapsed(&self) -> bool {
        false
    }
}

impl SidebarItem for CollectionsSection {
    fn render(self, id: impl Into<ElementId>, window: &mut Window, cx: &mut App)
              -> impl IntoElement {
        let id = id.into();
        let on_toggle = self.on_toggle.clone();
        let suffix = (self.suffix)(window, cx);
        let open = self.open;
        let loading = self.loading;
        let entity = self.entity.clone();
        let tabs = self.tabs.clone();

        let header =
            div().id(SharedString::from(format!("{id:?}-header")))
                 .w_full()
                 .flex()
                 .items_center()
                 .overflow_x_hidden()
                 .flex_shrink_0()
                 .p_2()
                 .gap_x_2()
                 .rounded(cx.theme().radius)
                 .text_sm()
                 .h_7()
                 .hover(|this| {
                     this.bg(cx.theme().sidebar_accent.opacity(0.8))
                         .text_color(cx.theme().sidebar_accent_foreground)
                 })
                 .on_click(move |_, window, cx| on_toggle(window, cx))
                 .child(div().flex_1()
                             .flex()
                             .items_center()
                             .justify_between()
                             .gap_x_2()
                             .overflow_x_hidden()
                             .child(div().flex_1().overflow_x_hidden().child(self.title.clone()))
                             .child(suffix));

        let mut rows: Vec<AnyElement> = self.rows
                                            .into_iter()
                                            .enumerate()
                                            .map(|(ix, row)| {
                                                render_collection_row(format!("{id:?}-row-{ix}"),
                                                                      row,
                                                                      entity.clone(),
                                                                      tabs.clone(),
                                                                      cx)
                                            })
                                            .collect();

        // Collections not yet known (initial fetch in flight) show a loading
        // indicator instead of a blank submenu.
        if rows.is_empty() && loading {
            rows.push(div().id("collections-loading")
                           .flex()
                           .items_center()
                           .gap_x_2()
                           .p_2()
                           .text_sm()
                           .child(Spinner::new().with_size(Size::Small))
                           .child(t!("sidebar.loading"))
                           .into_any_element());
        }

        div().id(id)
             .w_full()
             .flex()
             .flex_col()
             .child(header)
             .when(open, |this| {
                 this.child(div().id("submenu")
                                 .border_l_1()
                                 .border_color(cx.theme().sidebar_border)
                                 .flex()
                                 .flex_col()
                                 .gap_1()
                                 .ml_3p5()
                                 .pl_2p5()
                                 .py_0p5()
                                 .children(rows))
             })
    }
}

/// Renders a single collection nav row: click to filter, right-click to
/// download all/reload/delete, and drop target for [`DraggedLibraryItem`]
/// (adds the dragged catalog item as a member of this collection).
fn render_collection_row(id: impl Into<ElementId>, row: CollectionRow,
                         entity: Entity<LibraryController>, tabs: Entity<TabsController>,
                         cx: &mut App)
                         -> AnyElement {
    let is_active = row.is_active;
    let filter = SidebarFilter::Collection(row.id, Arc::clone(&row.name));
    let click_entity = entity.clone();
    let click_tabs = tabs;
    let download_entity = entity.clone();
    let reload_entity = entity.clone();
    let delete_entity = entity.clone();
    let drop_entity = entity;
    let col_id = row.id;
    let row_name_for_delete = row.name.to_string();

    div().id(id.into())
         .w_full()
         .flex()
         .items_center()
         .overflow_x_hidden()
         .flex_shrink_0()
         .p_2()
         .gap_x_2()
         .rounded(cx.theme().radius)
         .text_sm()
         .h_7()
         .when(!is_active, |this| {
             this.hover(|this| {
                     this.bg(cx.theme().sidebar_accent.opacity(0.8))
                         .text_color(cx.theme().sidebar_accent_foreground)
                 })
         })
         .when(is_active, |this| {
             this.font_medium()
                 .bg(cx.theme().tokens.sidebar_accent)
                 .text_color(cx.theme().sidebar_accent_foreground)
         })
         .on_click(move |_, _, cx| {
             click_entity.update(cx, |ctrl, cx| ctrl.set_filter(filter.clone(), cx));
             click_tabs.update(cx, |ctrl, cx| ctrl.activate(TabTarget::Catalog, cx));
         })
         .child(div().flex_1()
                     .flex()
                     .items_center()
                     .justify_between()
                     .gap_x_2()
                     .overflow_x_hidden()
                     .child(div().flex_1()
                                 .overflow_x_hidden()
                                 .child(row.name.to_string()))
                     .child(div().text_xs().child(row.count.to_string())))
         .drag_over::<DraggedLibraryItem>(|this, _, _, cx| this.bg(cx.theme().tokens.drop_target))
         .on_drop(move |drag: &DraggedLibraryItem, _, cx| {
             drop_entity.update(cx, |ctrl, cx| {
                            ctrl.add_item_to_collection(col_id,
                                                        drag.member_id,
                                                        drag.product_id,
                                                        cx);
                        });
         })
         .context_menu(move |menu, _, _| {
             menu.item(PopupMenuItem::new(t!("collections.download_all")).on_click({
                           let entity = download_entity.clone();
                           move |_, _, cx| {
                               entity.update(cx, |ctrl, cx| {
                                         ctrl.request_download_all_for_collection(col_id, cx);
                                     });
                           }
                       }))
                 .item(PopupMenuItem::new(t!("collections.reload")).on_click({
                           let entity = reload_entity.clone();
                           move |_, _, cx| {
                               entity.update(cx, |ctrl, cx| ctrl.load_collections(cx));
                           }
                       }))
                 .item(PopupMenuItem::new(t!("collections.delete")).on_click({
                           let entity = delete_entity.clone();
                           let collection_name = row_name_for_delete.clone();
                           move |_, window, cx| {
                               let entity = entity.clone();
                               let title = t!("collections.delete_confirm_title",
                                              name = collection_name).to_string();
                               window.open_alert_dialog(cx, move |alert, _, _| {
                                         let entity = entity.clone();
                                         alert
                            .confirm()
                            .title(title.clone())
                            .description(t!("collections.delete_confirm_description").to_string())
                            .on_ok(move |_, _, cx| {
                                entity.update(cx, |ctrl, cx| {
                                    ctrl.delete_collection(col_id, cx);
                                });
                                true
                            })
                                     });
                           }
                       }))
         })
         .into_any_element()
}

#[path = "sidebar_render.rs"]
mod sidebar_render;

pub(super) use sidebar_render::render_sidebar;

#[path = "sidebar_helpers.rs"]
mod sidebar_helpers;

use sidebar_helpers::{nav_item, render_collection_sort_control, render_section_search_suffix};
