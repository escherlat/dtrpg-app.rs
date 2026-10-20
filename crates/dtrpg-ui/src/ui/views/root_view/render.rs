//! The root view's render tree: composes the title bar, sidebar, active tab
//! content, and status bar from the controllers' snapshots, and wires the
//! window-level menu/keyboard actions.

use super::*;

pub(super) fn render_root_view(view: &mut LibraryRootView, window: &mut gpui::Window,
                               cx: &mut Context<LibraryRootView>)
                               -> impl IntoElement {
    // `Root` (gpui-kit, wrapping this view — see `gpui_kit::open_window`
    // in `ui/app/mod.rs`) renders its own dialog/sheet/notification
    // layers automatically via its own `Render` impl; this view does
    // not need to compose them.

    // Adjusting the shared rem size scales every `rems(...)`-based size
    // utility (`.text_sm()`, `.text_xs()`, etc.) in this window, like
    // zooming a page — see `LibraryController::set_ui_text_size`.
    window.set_rem_size(cx.global::<LibriTheme>().fonts.ui_text_size);

    let lib_entity = view.controller.clone();
    let settings_entity = view.settings.clone();
    let activity_entity = view.activity.clone();
    let auth_entity = view.auth_state.clone();

    let snap = view.controller.read(cx).snapshot();
    let tags = snap.tags;
    let (filter,
         counts,
         publishers,
         collections,
         collections_loaded,
         catalog_loading,
         catalog_ids,
         total_count,
         total_mb,
         matched_count,
         sort,
         sort_direction,
         collection_sort,
         collection_sort_direction,
         group_by,
         presentation,
         bulk_selection_mode,
         bulk_selected_ids,
         bulk_selection_count,
         bulk_items) = (snap.filter,
                        snap.counts,
                        snap.publishers,
                        snap.collections,
                        snap.collections_loaded,
                        snap.catalog_loading,
                        snap.catalog_ids,
                        snap.total_count,
                        snap.total_mb,
                        snap.matched_count,
                        snap.sort,
                        snap.sort_direction,
                        snap.collection_sort,
                        snap.collection_sort_direction,
                        snap.group_by,
                        snap.presentation,
                        snap.bulk_selection_mode,
                        snap.bulk_selected_ids,
                        snap.bulk_selection_count,
                        snap.items);

    let settings_snap = view.settings.read(cx).snapshot();
    let activity_snap = view.activity.read(cx).snapshot();
    let notices: Vec<_> = view.auth_state
                              .read(cx)
                              .active_notices()
                              .into_iter()
                              .cloned()
                              .collect();

    let theme = cx.global::<LibriTheme>().clone();
    let colors = &theme.colors;

    let bulk_action_bar =
        bulk_selection_mode.then(|| {
                               render_bulk_action_bar(bulk_selection_count,
                                                      &bulk_selected_ids,
                                                      &bulk_items,
                                                      &collections,
                                                      lib_entity.clone(),
                                                      view.bulk_pattern_input.clone(),
                                                      view.bulk_pattern_field.clone(),
                                                      colors)
                           });

    let publisher_search =
        crate::ui::views::sidebar_view::SidebarSectionSearch { open:  snap.publisher_search_open,
                                                               query: snap.publisher_search_query,
                                                               input: view.publisher_search_input
                                                                          .clone(), };
    let collection_search =
        crate::ui::views::sidebar_view::SidebarSectionSearch { open:  snap.collection_search_open,
                                                               query: snap.collection_search_query,
                                                               input: view.collection_search_input
                                                                          .clone(), };
    let sidebar = render_sidebar(filter.clone(),
                                 counts,
                                 publishers,
                                 collections,
                                 collections_loaded,
                                 catalog_loading,
                                 catalog_ids,
                                 collection_sort,
                                 collection_sort_direction,
                                 lib_entity.clone(),
                                 view.tabs.clone(),
                                 view.collection_name_input.clone(),
                                 publisher_search,
                                 collection_search,
                                 tags,
                                 snap.sidebar_open);
    let toolbar = render_toolbar(&filter,
                                 snap.filter_count,
                                 matched_count,
                                 total_count,
                                 &snap.search_query,
                                 view.search_input.clone(),
                                 sort,
                                 sort_direction,
                                 group_by,
                                 presentation,
                                 bulk_selection_mode,
                                 lib_entity.clone(),
                                 colors);
    let title_bar = render_title_bar(&settings_snap.auth, settings_entity.clone(), colors, cx);
    let tab_strip = render_tab_strip(view.tabs.clone(), cx);
    let banner = render_notification_banner(notices, auth_entity, cx.entity(), colors);

    let tabs_snap = view.tabs.read(cx).snapshot();
    let (active_tab_label, active_tab_count) = match &tabs_snap.active {
        TabTarget::Catalog => {
            (crate::ui::views::toolbar_view::section_title_for(&filter), matched_count)
        }
        TabTarget::Detail(id) => {
            (tabs_snap.titles
                      .get(id)
                      .cloned()
                      .unwrap_or_else(|| t!("tabs.detail_tab_fallback").to_string()),
             1)
        }
    };
    let alert_snap = view.activity.read(cx).alert_snapshot();
    let current_locale =
        crate::i18n::Locale::from_code(&rust_i18n::locale()).unwrap_or(crate::i18n::Locale::En);
    let status_bar = render_status_bar(StatusBarSnapshot { total_count,
                                                           total_mb,
                                                           active_tab_label,
                                                           active_tab_count,
                                                           theme_key: theme.id.clone(),
                                                           current_locale },
                                       lib_entity.clone(),
                                       ActivityBarData { entity:     activity_entity.clone(),
                                                         snap:       &activity_snap,
                                                         alert_snap: &alert_snap, },
                                       colors);

    let active_tab_content: AnyElement = match &tabs_snap.active {
        TabTarget::Catalog => div().flex_1()
                                   .min_h_0()
                                   .flex()
                                   .flex_col()
                                   .child(toolbar)
                                   .children(bulk_action_bar)
                                   .child(banner)
                                   .child(view.catalog_view.clone())
                                   .into_any_element(),
        TabTarget::Detail(id) => {
            let controller_ref = view.controller.read(cx);
            match controller_ref.item_by_id(id) {
                Some(item) => {
                    let item = item.clone();
                    let cache = cx.global::<crate::ui::library::cover::CoverCache>();
                    let detail_key = crate::ui::library::cover::detail_cache_key(&item.id);
                    let cover_image = cache.get(&detail_key).or_else(|| cache.get(&item.id));
                    render_detail_tab_content(&item,
                                              settings_snap.storage_root_path.clone(),
                                              lib_entity.clone(),
                                              &view.tabs,
                                              colors,
                                              cover_image,
                                              window,
                                              cx)
                }
                None => div().flex_1().min_h_0().into_any_element(),
            }
        }
    };

    let surface = colors.surface;
    let text_primary = colors.text_primary;

    // Settings renders in its own window (see `settings_window_view`), not
    // as an overlay here, so the main content area no longer
    // branches on `settings_snap.is_open`.
    let main_content = div().flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .relative()
                            .bg(surface)
                            .child(tab_strip)
                            .child(active_tab_content);

    let sidebar_col = div().size_full().relative().child(sidebar);

    let this_entity = cx.entity();
    let this_entity_for_about = this_entity.clone();
    let this_entity_for_add = this_entity.clone();
    let this_entity_for_activity = this_entity.clone();
    let this_entity_for_alert_history = this_entity.clone();
    let this_entity_for_report_bug = this_entity.clone();
    let controller_for_reload = view.controller.clone();
    let controller_for_refresh_thumbnails = view.controller.clone();
    let controller_for_check_availability = view.controller.clone();
    let controller_for_view_list = view.controller.clone();
    let controller_for_view_thumbs = view.controller.clone();
    let controller_for_view_grid = view.controller.clone();
    let controller_for_sort_title = view.controller.clone();
    let controller_for_sort_publisher = view.controller.clone();
    let controller_for_sort_date_added = view.controller.clone();
    let controller_for_sort_pages = view.controller.clone();
    let controller_for_sort_asc = view.controller.clone();
    let controller_for_sort_desc = view.controller.clone();
    let controller_for_group_toggle = view.controller.clone();
    let controller_for_group_by_tag = view.controller.clone();
    let search_input_for_focus = view.search_input.clone();
    let tabs_for_select_0 = view.tabs.clone();
    let tabs_for_select_1 = view.tabs.clone();
    let tabs_for_select_2 = view.tabs.clone();
    let tabs_for_select_3 = view.tabs.clone();
    let tabs_for_select_4 = view.tabs.clone();
    let tabs_for_select_5 = view.tabs.clone();
    let tabs_for_select_6 = view.tabs.clone();
    let tabs_for_select_7 = view.tabs.clone();
    let tabs_for_select_8 = view.tabs.clone();
    let tabs_for_select_9 = view.tabs.clone();
    let sidebar_initial = view.sidebar_width;

    div()
        .size_full()
        .bg(surface)
        .text_color(text_primary)
        .relative()
        .track_focus(&view.root_focus)
        .on_action(move |_: &ShowSettings, _, cx| {
            this_entity.update(cx, |view, cx| view.show_settings(cx));
        })
        .on_action(move |_: &About, _, cx| {
            this_entity_for_about.update(cx, |view, cx| view.show_about(cx));
        })
        .on_action(move |_: &ReportFeatureBug, _, cx| {
            this_entity_for_report_bug.update(cx, |view, cx| view.show_report_feature_bug(cx));
        })
        .on_action(move |_: &ReloadCatalog, _, cx| {
            controller_for_reload.update(cx, |ctrl, cx| ctrl.reload_catalog(cx));
        })
        .on_action(move |_: &RefreshThumbnails, _, cx| {
            controller_for_refresh_thumbnails
                .update(cx, |ctrl, cx| ctrl.refresh_all_thumbnails(cx));
        })
        .on_action(move |_: &CheckItemAvailability, _, cx| {
            controller_for_check_availability
                .update(cx, |ctrl, cx| ctrl.request_manual_check_batch(cx));
        })
        .on_action(move |_: &ViewAsList, _, cx| {
            controller_for_view_list.update(cx, |ctrl, cx| {
                ctrl.set_presentation(CatalogPresentation::List, cx)
            });
        })
        .on_action(move |_: &ViewAsThumbs, _, cx| {
            controller_for_view_thumbs.update(cx, |ctrl, cx| {
                ctrl.set_presentation(CatalogPresentation::Thumbs, cx);
            });
        })
        .on_action(move |_: &ViewAsGrid, _, cx| {
            controller_for_view_grid.update(cx, |ctrl, cx| {
                ctrl.set_presentation(CatalogPresentation::Grid, cx)
            });
        })
        .on_action(move |_: &SortByTitle, _, cx| {
            controller_for_sort_title
                .update(cx, |ctrl, cx| ctrl.set_sort(SortMethod::Title, cx));
        })
        .on_action(move |_: &SortByPublisher, _, cx| {
            controller_for_sort_publisher
                .update(cx, |ctrl, cx| ctrl.set_sort(SortMethod::Publisher, cx));
        })
        .on_action(move |_: &SortByDateAdded, _, cx| {
            controller_for_sort_date_added
                .update(cx, |ctrl, cx| ctrl.set_sort(SortMethod::DateAdded, cx));
        })
        .on_action(move |_: &SortByPages, _, cx| {
            controller_for_sort_pages
                .update(cx, |ctrl, cx| ctrl.set_sort(SortMethod::PageCount, cx));
        })
        .on_action(move |_: &SortAscending, _, cx| {
            controller_for_sort_asc.update(cx, |ctrl, cx| {
                ctrl.set_sort_direction(SortDirection::Ascending, cx);
            });
        })
        .on_action(move |_: &SortDescending, _, cx| {
            controller_for_sort_desc.update(cx, |ctrl, cx| {
                ctrl.set_sort_direction(SortDirection::Descending, cx);
            });
        })
        .on_action(move |_: &ToggleGroupByPublisher, _, cx| {
            controller_for_group_toggle.update(cx, |ctrl, cx| {
                ctrl.toggle_group_by(GroupBy::Publisher, cx);
            });
        })
        .on_action(move |_: &ToggleGroupByTag, _, cx| {
            controller_for_group_by_tag.update(cx, |ctrl, cx| {
                ctrl.toggle_group_by(GroupBy::Tag, cx);
            });
        })
        .on_action(move |_: &FocusSearch, window, cx| {
            search_input_for_focus.update(cx, |input, cx| input.focus(window, cx));
        })
        .on_action(move |_: &AddCollection, window, cx| {
            this_entity_for_add.update(cx, |view, cx| {
                view.show_add_collection_dialog(window, cx);
            });
        })
        .on_action(move |_: &ShowActivity, _, cx| {
            this_entity_for_activity.update(cx, |view, cx| view.show_activity(cx));
        })
        .on_action(move |_: &ShowAlertHistory, _, cx| {
            this_entity_for_alert_history.update(cx, |view, cx| view.show_alert_history(cx));
        })
        .when(tab_target_at(&tabs_snap, 0).is_some(), |this| {
            this.on_action(move |_: &SelectTab0, _, cx| {
                if let Some(target) = tab_target_at(&tabs_for_select_0.read(cx).snapshot(), 0) {
                    tabs_for_select_0.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                }
            })
        })
        .when(tab_target_at(&tabs_snap, 1).is_some(), |this| {
            this.on_action(move |_: &SelectTab1, _, cx| {
                if let Some(target) = tab_target_at(&tabs_for_select_1.read(cx).snapshot(), 1) {
                    tabs_for_select_1.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                }
            })
        })
        .when(tab_target_at(&tabs_snap, 2).is_some(), |this| {
            this.on_action(move |_: &SelectTab2, _, cx| {
                if let Some(target) = tab_target_at(&tabs_for_select_2.read(cx).snapshot(), 2) {
                    tabs_for_select_2.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                }
            })
        })
        .when(tab_target_at(&tabs_snap, 3).is_some(), |this| {
            this.on_action(move |_: &SelectTab3, _, cx| {
                if let Some(target) = tab_target_at(&tabs_for_select_3.read(cx).snapshot(), 3) {
                    tabs_for_select_3.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                }
            })
        })
        .when(tab_target_at(&tabs_snap, 4).is_some(), |this| {
            this.on_action(move |_: &SelectTab4, _, cx| {
                if let Some(target) = tab_target_at(&tabs_for_select_4.read(cx).snapshot(), 4) {
                    tabs_for_select_4.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                }
            })
        })
        .when(tab_target_at(&tabs_snap, 5).is_some(), |this| {
            this.on_action(move |_: &SelectTab5, _, cx| {
                if let Some(target) = tab_target_at(&tabs_for_select_5.read(cx).snapshot(), 5) {
                    tabs_for_select_5.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                }
            })
        })
        .when(tab_target_at(&tabs_snap, 6).is_some(), |this| {
            this.on_action(move |_: &SelectTab6, _, cx| {
                if let Some(target) = tab_target_at(&tabs_for_select_6.read(cx).snapshot(), 6) {
                    tabs_for_select_6.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                }
            })
        })
        .when(tab_target_at(&tabs_snap, 7).is_some(), |this| {
            this.on_action(move |_: &SelectTab7, _, cx| {
                if let Some(target) = tab_target_at(&tabs_for_select_7.read(cx).snapshot(), 7) {
                    tabs_for_select_7.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                }
            })
        })
        .when(tab_target_at(&tabs_snap, 8).is_some(), |this| {
            this.on_action(move |_: &SelectTab8, _, cx| {
                if let Some(target) = tab_target_at(&tabs_for_select_8.read(cx).snapshot(), 8) {
                    tabs_for_select_8.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                }
            })
        })
        .when(tab_target_at(&tabs_snap, 9).is_some(), |this| {
            this.on_action(move |_: &SelectTab9, _, cx| {
                if let Some(target) = tab_target_at(&tabs_for_select_9.read(cx).snapshot(), 9) {
                    tabs_for_select_9.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                }
            })
        })
        .child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(title_bar)
                .child(
                    div().flex_1().min_h_0().child(
                        h_resizable("main-layout")
                            .with_state(&view.resize_state)
                            .child(
                                resizable_panel()
                                    .size(px(sidebar_initial))
                                    .size_range(px(180.)..px(361.))
                                    .child(sidebar_col),
                            )
                            .child(
                                resizable_panel()
                                    .size_range(px(280.)..Pixels::MAX)
                                    .child(main_content),
                            ),
                    ),
                )
                .child(status_bar),
        )
}
