//! Sidebar helper controls.

use super::*;

pub(super) fn render_collection_sort_control(current: CollectionSortMethod,
                                             direction: SortDirection,
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

pub(super) fn nav_item(label: &str, count: usize, is_active: bool, filter: SidebarFilter,
                       entity: Entity<LibraryController>, tabs: Entity<TabsController>)
                       -> SidebarMenuItem {
    let label = label.to_string();
    let click_filter = filter.clone();
    let download_entity = entity.clone();
    let download_filter = filter;
    SidebarMenuItem::new(label).active(is_active)
                               .suffix(move |_, _| div().text_xs().child(count.to_string()))
                               .on_click(move |_, _, cx| {
                                   entity.update(cx, |ctrl, cx| {
                                             ctrl.set_filter(click_filter.clone(), cx)
                                         });
                                   tabs.update(cx, |ctrl, cx| {
                                           ctrl.activate(TabTarget::Catalog, cx)
                                       });
                               })
                               .context_menu(move |menu, _, _| {
                                   let entity = download_entity.clone();
                                   let filter = download_filter.clone();
                                   menu.item(
                PopupMenuItem::new(t!("sidebar.download_all")).on_click(move |_, _, cx| {
                    entity.update(cx, |ctrl, cx| {
                        if let SidebarFilter::Publisher(name) = &filter {
                            ctrl.request_download_all_for_publisher(name, cx);
                        } else {
                            ctrl.request_download_all_for_filter(filter.clone(), cx);
                        }
                    });
                }),
            )
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
pub(super) fn render_section_search_suffix(
    id_prefix: &'static str, count: usize, search: SidebarSectionSearch,
    entity: Entity<LibraryController>,
    toggle: impl Fn(&mut LibraryController, &mut Context<LibraryController>) + Copy + 'static)
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
