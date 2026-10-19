//! Sidebar layout rendering.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_sidebar(filter: SidebarFilter, counts: SectionCounts,
                             publishers: Vec<PublisherEntry>, collections: Vec<CollectionEntry>,
                             collections_loaded: bool, catalog_loading: bool,
                             catalog_ids: Arc<HashSet<u64>>,
                             collection_sort: CollectionSortMethod,
                             collection_sort_direction: SortDirection,
                             entity: Entity<LibraryController>, tabs: Entity<TabsController>,
                             collection_name_input: Entity<InputState>,
                             publisher_search: SidebarSectionSearch,
                             collection_search: SidebarSectionSearch, tags: Vec<TagEntry>,
                             sidebar_open: SidebarOpen)
                             -> impl IntoElement + 'static {
    let active = filter.clone();
    let publishers_open = sidebar_open.publishers;
    let collections_open = sidebar_open.collections;

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
                entity_for_pub.update(cx, |ctrl, cx| {
                    ctrl.toggle_sidebar_section(SidebarSection::Publishers, cx);
                });
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
        entity_for_col.update(cx, |ctrl, cx| {
                          ctrl.toggle_sidebar_section(SidebarSection::Collections, cx);
                      });
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
                .child(render_collection_sort_control(
                    collection_sort,
                    collection_sort_direction,
                    entity_for_toggle.clone(),
                ))
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
                                                   tabs: tabs.clone() };

    let sidebar_builder =
        sidebar_builder.child(SidebarContent::Collections(Box::new(collections_section)))
                       .child(SidebarContent::Separator)
                       .child(SidebarContent::Menu(Box::new(pub_menu)));
    // Nothing to list until something has been tagged.
    if tags.is_empty() {
        return sidebar_builder;
    }
    sidebar_builder.child(SidebarContent::Separator)
                   .child(SidebarContent::Menu(Box::new(tags_menu(tags,
                                                                  sidebar_open.tags,
                                                                  &active,
                                                                  entity,
                                                                  tabs))))
}
