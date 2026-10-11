//! Sidebar view: smart nav, publisher nav, collections nav.
//!
//! The app wordmark lives in `title_bar_view`, next to the macOS traffic
//! lights — it is not repeated here.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, ElementId, Entity, IntoElement, SharedString, Window, div, px,
};
use gpui_component::IconName;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dialog::{Dialog, DialogButtonProps, DialogHeader, DialogTitle};
use gpui_component::input::{Input, InputState};
use gpui_component::menu::{ContextMenuExt, DropdownMenu, PopupMenuItem};
use gpui_component::sidebar::{
    Sidebar, SidebarCollapsible, SidebarItem, SidebarMenu, SidebarMenuItem,
};
use gpui_component::spinner::Spinner;
use gpui_component::{
    ActiveTheme, Collapsible, Side, Sizable as _, Size, StyledExt as _, WindowExt as _,
};
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::controllers::tabs::{TabTarget, TabsController};
use crate::data::collection::CollectionEntry;
use crate::data::library::SectionCounts;
use crate::data::ui_state::UiState;
use crate::ui::library::drag::DraggedLibraryItem;
use crate::util::filter::SidebarFilter;
use crate::util::matching::name_matches_query;
use crate::util::publisher::PublisherEntry;
use crate::util::sort::{CollectionSortMethod, SortDirection};

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
/// `gpui-component` crate with no way to wrap or extend it from here. This
/// is the only sidebar section that needs to be a drop target (see
/// `catalog-drag-drop-to-collection`), so only this section forgoes
/// `SidebarMenuItem`; smart-filter and publisher rows are unaffected.
///
/// The open/closed state is read from `UiState` on every render (same as
/// before), so — unlike `SidebarMenuItem`, which tracks its own internal
/// open state — no separate collapse-toggle plumbing is needed here.
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

/// Renders the full sidebar column using gpui-component `Sidebar`.
///
/// Library totals and the activity indicator, previously shown in the
/// sidebar footer, now live in the status bar (see `status_bar_view.rs`),
/// per `main-window-status-bar`.
#[allow(clippy::too_many_arguments)]
pub fn render_sidebar(filter: SidebarFilter, counts: SectionCounts,
                      publishers: Vec<PublisherEntry>, collections: Vec<CollectionEntry>,
                      collections_loaded: bool, catalog_loading: bool,
                      catalog_ids: HashSet<u64>, collection_sort: CollectionSortMethod,
                      collection_sort_direction: SortDirection,
                      entity: Entity<LibraryController>, tabs: Entity<TabsController>,
                      collection_name_input: Entity<InputState>,
                      publisher_search: SidebarSectionSearch,
                      collection_search: SidebarSectionSearch)
                      -> impl IntoElement + 'static {
    let active = filter.clone();
    let prefs = UiState::load();
    let publishers_open = prefs.publishers_open();
    let collections_open = prefs.collections_open();

    // ── Library smart-filter menu
    // ─────────────────────────────────────────────
    let lib_menu = SidebarMenu::new().child(nav_item(&t!("sidebar.all_titles"),
                                                     counts.all,
                                                     active == SidebarFilter::AllTitles,
                                                     SidebarFilter::AllTitles,
                                                     entity.clone(),
                                                     tabs.clone()))
                                     .child(nav_item(&t!("sidebar.recently_updated"),
                                                     counts.recently_updated,
                                                     active == SidebarFilter::RecentlyUpdated,
                                                     SidebarFilter::RecentlyUpdated,
                                                     entity.clone(),
                                                     tabs.clone()))
                                     .child(nav_item(&t!("sidebar.on_this_device"),
                                                     counts.on_device,
                                                     active == SidebarFilter::OnDevice,
                                                     SidebarFilter::OnDevice,
                                                     entity.clone(),
                                                     tabs.clone()))
                                     .child(nav_item(&t!("sidebar.in_the_cloud"),
                                                     counts.in_cloud,
                                                     active == SidebarFilter::InCloud,
                                                     SidebarFilter::InCloud,
                                                     entity.clone(),
                                                     tabs.clone()));

    let publishers_count = publishers.len();

    // ── Publishers menu
    // ───────────────────────────────────────────────────────
    let mut pub_children: Vec<SidebarMenuItem> =
        publishers.into_iter()
                  .filter(|p| name_matches_query(p.name.as_ref(), &publisher_search.query))
                  .map(|p| {
                      let is_active = active == SidebarFilter::Publisher(Arc::clone(&p.name));
                      let f = SidebarFilter::Publisher(Arc::clone(&p.name));
                      nav_item(p.name.as_ref(),
                               p.count,
                               is_active,
                               f,
                               entity.clone(),
                               tabs.clone())
                  })
                  .collect();

    // Publishers are derived from the catalog itself, so an empty list while
    // the initial fetch is still in flight means "not yet known", not
    // "confirmed zero publishers" — show a loading indicator instead of a
    // blank section.
    if pub_children.is_empty() && catalog_loading {
        pub_children.push(
            SidebarMenuItem::new(t!("sidebar.loading"))
                .disable(true)
                .suffix(|_window, _cx| Spinner::new().with_size(Size::Small)),
        );
    }

    let entity_for_pub = entity.clone();
    let pub_title: SharedString = if publisher_search.open {
        "".into()
    }
    else {
        t!("sidebar.publishers").into()
    };
    let pub_menu = SidebarMenu::new().child(
        SidebarMenuItem::new(pub_title)
            .default_open(publishers_open)
            .click_to_toggle(true)
            .on_click(move |_, _, cx| {
                UiState::load().save_publishers_open(!publishers_open);
                entity_for_pub.update(cx, |ctrl, cx| ctrl.notify_ui_change(cx));
            })
            .suffix(render_section_search_suffix(
                "publishers",
                publishers_count,
                publisher_search,
                entity.clone(),
                LibraryController::toggle_publisher_search,
            ))
            .children(pub_children),
    );

    // ── Sidebar assembly (collections before publishers)
    // ────────────────────── `w_full()` rather than a fixed pixel width:
    // the parent `resizable_panel` in `root_view.rs` already owns width
    // management (its divider IS the drag handle — no separate resize
    // control). A fixed width here would leave the visible sidebar content
    // pinned regardless of the panel's actual dragged width, decoupling
    // what you drag from where the sidebar and catalog actually meet.
    let sidebar_builder = Sidebar::new("sidebar").collapsible(SidebarCollapsible::None)
                                                 .side(Side::Left)
                                                 .w_full()
                                                 .child(SidebarContent::Menu(Box::new(lib_menu)))
                                                 .child(SidebarContent::Separator);

    // ── Collections menu (always present) ────────────────────────────────────
    // Show "?" instead of "0" while the initial collections fetch is still in
    // flight, so an empty list doesn't read as a confirmed zero-collections
    // state.
    let collections_count: SharedString = if collections_loaded {
        collections.len().to_string().into()
    }
    else {
        "?".into()
    };

    let col_rows: Vec<CollectionRow> =
        collections.into_iter()
                   .filter(|c| name_matches_query(c.name.as_ref(), &collection_search.query))
                   .map(|c| {
                       let is_active =
                           matches!(&active, SidebarFilter::Collection(id, _) if *id == c.id);
                       let count = c.member_ids
                                    .iter()
                                    .filter(|id| catalog_ids.contains(id))
                                    .count();
                       CollectionRow { id: c.id,
                                       name: c.name,
                                       count,
                                       is_active }
                   })
                   .collect();

    let entity_for_col = entity.clone();
    let col_title: SharedString = if collection_search.open {
        "".into()
    }
    else {
        t!("sidebar.collections").into()
    };
    let collection_search_open = collection_search.open;
    let collection_search_input = collection_search.input.clone();
    let on_toggle: CollectionsToggleFn = Rc::new(move |_window, cx| {
        UiState::load().save_collections_open(!collections_open);
        entity_for_col.update(cx, |ctrl, cx| ctrl.notify_ui_change(cx));
    });
    let suffix: CollectionsSuffixFn = Rc::new({
        let input = collection_name_input.clone();
        let ctrl = entity.clone();
        let search_input = collection_search_input.clone();
        let entity_for_toggle = entity.clone();
        move |_window, cx| {
            let input = input.clone();
            let ctrl = ctrl.clone();

            if collection_search_open {
                let entity_for_close = entity_for_toggle.clone();
                let search_input_for_close = search_input.clone();
                return div()
                    .id("collections-search-suffix")
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .flex()
                    .items_center()
                    .w_full()
                    .gap(px(4.))
                    .child(div().flex_1().child(Input::new(&search_input).small()))
                    .child(
                        Button::new("collections-search-close")
                            .ghost()
                            .compact()
                            .icon(IconName::Close)
                            .on_click(move |_, window, cx| {
                                entity_for_close.update(cx, |ctrl, cx| {
                                    ctrl.toggle_collection_search(cx);
                                });
                                search_input_for_close.update(cx, |st, cx| {
                                    st.set_value("", window, cx);
                                });
                            }),
                    )
                    .into_any_element();
            }

            div()
                .id("collections-suffix")
                // Stop click propagation so the suffix button does not also
                // fire the SidebarMenuItem header's on_click (which toggles collapse).
                .on_click(|_, _, cx| cx.stop_propagation())
                .flex()
                .items_center()
                .gap(px(4.))
                .child(div().text_xs().child(collections_count.clone()))
                .child(render_collection_sort_control(collection_sort,
                                                       collection_sort_direction,
                                                       entity_for_toggle.clone()))
                .child(
                    Button::new("collections-search-open")
                        .ghost()
                        .compact()
                        .icon(IconName::Search)
                        .tooltip(t!("sidebar.search_tooltip").to_string())
                        .on_click({
                            let entity = entity_for_toggle.clone();
                            let input = search_input.clone();
                            move |_, window, cx| {
                                entity.update(cx, |ctrl, cx| {
                                    ctrl.toggle_collection_search(cx);
                                });
                                input.update(cx, |st, cx| st.focus(window, cx));
                            }
                        }),
                )
                .child(
                    Dialog::new(cx)
                        .trigger(
                            Button::new("add-collection")
                                .ghost()
                                .compact()
                                .icon(IconName::Plus)
                                .tooltip(t!("collections.add_tooltip").to_string()),
                        )
                        .w(px(320.))
                        .close_button(false)
                        .overlay_closable(true)
                        .button_props(
                            DialogButtonProps::default()
                                .ok_text(t!("collections.add_dialog_confirm").to_string())
                                .show_cancel(true)
                                .cancel_text(t!("collections.add_dialog_cancel").to_string()),
                        )
                        .on_ok({
                            let input = input.clone();
                            let ctrl = ctrl.clone();
                            move |_, _, cx| {
                                let name = input.read(cx).value().trim().to_string();
                                if name.is_empty() {
                                    return false;
                                }
                                ctrl.update(cx, |c, cx| c.create_collection(name, cx));
                                true
                            }
                        })
                        .on_cancel(|_, _, _| true)
                        .content({
                            let input = collection_name_input.clone();
                            move |content, _, _| {
                                content
                                    .child(
                                        DialogHeader::new().px_4().pt_4().child(
                                            DialogTitle::new()
                                                .child(t!("collections.add_dialog_title")),
                                        ),
                                    )
                                    .child(div().px_4().py_2().child(Input::new(&input)))
                            }
                        }),
                )
                .into_any_element()
        }
    });

    let collections_section = CollectionsSection { title: col_title,
                                                   open: collections_open,
                                                   suffix,
                                                   on_toggle,
                                                   rows: col_rows,
                                                   loading: !collections_loaded,
                                                   entity: entity.clone(),
                                                   tabs };

    sidebar_builder.child(SidebarContent::Collections(Box::new(collections_section)))
                   .child(SidebarContent::Separator)
                   .child(SidebarContent::Menu(Box::new(pub_menu)))
}

/// Renders the Collections header's sort control: an icon button whose
/// dropdown menu offers the three sort methods plus an ascending/descending
/// toggle, following `toolbar_view::render_sort_selector`'s `PopupMenuItem`
/// construction.
fn render_collection_sort_control(current: CollectionSortMethod, direction: SortDirection,
                                  entity: Entity<LibraryController>)
                                  -> impl IntoElement + 'static {
    Button::new("collections-sort").ghost()
                                   .compact()
                                   .icon(if direction == SortDirection::Ascending {
                                             IconName::SortAscending
                                         }
                                         else {
                                             IconName::SortDescending
                                         })
                                   .tooltip(t!("sidebar.collections_sort_tooltip").to_string())
                                   .dropdown_menu(move |menu, _, _| {
                                       let e_name = entity.clone();
                                       let e_date = entity.clone();
                                       let e_count = entity.clone();
                                       let e_asc = entity.clone();
                                       let e_desc = entity.clone();
                                       menu.item(
                PopupMenuItem::new(t!("sidebar.collections_sort_name"))
                    .checked(current == CollectionSortMethod::Name)
                    .on_click(move |_, _, cx| {
                        e_name.update(cx, |ctrl, cx| {
                            ctrl.set_collection_sort(CollectionSortMethod::Name, cx);
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(t!("sidebar.collections_sort_date_created"))
                    .checked(current == CollectionSortMethod::DateCreated)
                    .on_click(move |_, _, cx| {
                        e_date.update(cx, |ctrl, cx| {
                            ctrl.set_collection_sort(CollectionSortMethod::DateCreated, cx);
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(t!("sidebar.collections_sort_item_count"))
                    .checked(current == CollectionSortMethod::ItemCount)
                    .on_click(move |_, _, cx| {
                        e_count.update(cx, |ctrl, cx| {
                            ctrl.set_collection_sort(CollectionSortMethod::ItemCount, cx);
                        });
                    }),
            )
            .separator()
            .item(
                PopupMenuItem::new(t!("sidebar.collections_sort_ascending"))
                    .checked(direction == SortDirection::Ascending)
                    .on_click(move |_, _, cx| {
                        e_asc.update(cx, |ctrl, cx| {
                            ctrl.set_collection_sort_direction(SortDirection::Ascending, cx);
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(t!("sidebar.collections_sort_descending"))
                    .checked(direction == SortDirection::Descending)
                    .on_click(move |_, _, cx| {
                        e_desc.update(cx, |ctrl, cx| {
                            ctrl.set_collection_sort_direction(SortDirection::Descending, cx);
                        });
                    }),
            )
                                   })
}

// ── Helpers
// ───────────────────────────────────────────────────────────────────

fn nav_item(label: &str, count: usize, is_active: bool, filter: SidebarFilter,
            entity: Entity<LibraryController>, tabs: Entity<TabsController>)
            -> SidebarMenuItem {
    let label = label.to_string();
    SidebarMenuItem::new(label).active(is_active)
                               .suffix(move |_, _| div().text_xs().child(count.to_string()))
                               .on_click(move |_, _, cx| {
                                   entity.update(cx, |ctrl, cx| {
                                             ctrl.set_filter(filter.clone(), cx)
                                         });
                                   tabs.update(cx, |ctrl, cx| {
                                           ctrl.activate(TabTarget::Catalog, cx)
                                       });
                               })
}

/// Builds a `SidebarMenuItem` suffix that toggles between a plain item count
/// and a full-width search input, for sections like Publishers whose header
/// carries no other actions.
///
/// When `search.open` is `false`, renders the item count plus a
/// magnifying-glass button. When `true`, renders the search input (backed by
/// `search.input`) plus a close button that toggles the section closed and
/// clears the input's visible text.
fn render_section_search_suffix(id_prefix: &'static str, count: usize,
                                search: SidebarSectionSearch, entity: Entity<LibraryController>,
                                toggle: impl Fn(&mut LibraryController,
                                   &mut Context<LibraryController>)
                                + Copy
                                + 'static)
                                -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |_window, _cx| {
        if search.open {
            let input = search.input.clone();
            let entity_for_close = entity.clone();
            let input_for_close = input.clone();
            div()
                .id(SharedString::from(format!("{id_prefix}-search-suffix")))
                .on_click(|_, _, cx| cx.stop_propagation())
                .flex()
                .items_center()
                .w_full()
                .gap(px(4.))
                .child(div().flex_1().child(Input::new(&input).small()))
                .child(
                    Button::new(SharedString::from(format!("{id_prefix}-search-close")))
                        .ghost()
                        .compact()
                        .icon(IconName::Close)
                        .on_click(move |_, window, cx| {
                            entity_for_close.update(cx, |ctrl, cx| toggle(ctrl, cx));
                            input_for_close.update(cx, |st, cx| st.set_value("", window, cx));
                        }),
                )
                .into_any_element()
        }
        else {
            let entity_for_open = entity.clone();
            let input_for_open = search.input.clone();
            div()
                .id(SharedString::from(format!("{id_prefix}-search-suffix")))
                .on_click(|_, _, cx| cx.stop_propagation())
                .flex()
                .items_center()
                .gap(px(4.))
                .child(div().text_xs().child(count.to_string()))
                .child(
                    Button::new(SharedString::from(format!("{id_prefix}-search-open")))
                        .ghost()
                        .compact()
                        .icon(IconName::Search)
                        .tooltip(t!("sidebar.search_tooltip").to_string())
                        .on_click(move |_, window, cx| {
                            entity_for_open.update(cx, |ctrl, cx| toggle(ctrl, cx));
                            input_for_open.update(cx, |st, cx| st.focus(window, cx));
                        }),
                )
                .into_any_element()
        }
    }
}
