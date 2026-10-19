//! Native application menu construction.

use super::*;

pub fn build_menus(state: &ViewMenuState, tabs: &TabsSnapshot) -> Vec<Menu> {
    // Column-header clicks produce `SortMethod::Custom { col_key }` rather than
    // the named variants the menu offers; map each back to the menu item it
    // corresponds to so the checkmark still tracks column-driven sorts.
    let normalized_sort = match state.sort {
        SortMethod::Custom { col_key: "publisher", } => Some(SortMethod::Publisher),
        SortMethod::Custom { col_key: "added" } => Some(SortMethod::DateAdded),
        SortMethod::Custom { col_key: "pages" } => Some(SortMethod::PageCount),
        SortMethod::Custom { .. } => None,
        method => Some(method),
    };
    let sort_checked = |target: SortMethod| normalized_sort == Some(target);

    // `cmd-0` always targets the Catalog tab (`open_tabs[0]`); `cmd-<n>` for
    // `n` in `1..=9` targets the nth open *detail* tab (`open_tabs[n]`),
    // since Catalog occupies index `0` and is never re-targeted by `cmd-1..9`.
    // Returns (label, enabled, checked) — checked reflects whether the tab at
    // this position is the currently active tab.
    let tab_label = |position: usize| -> (SharedString, bool, bool) {
        match tabs.open_tabs.get(position) {
            Some(target @ TabTarget::Catalog) => {
                (t!("tabs.catalog_tab").to_string().into(), true, *target == tabs.active)
            }
            Some(target @ TabTarget::Detail(id)) => {
                let title = tabs.titles
                                .get(id)
                                .cloned()
                                .unwrap_or_else(|| t!("tabs.detail_tab_fallback").to_string());
                (truncate_with_ellipsis(&title, DETAIL_TAB_TITLE_MAX_CHARS).into(),
                 true,
                 *target == tabs.active)
            }
            None => (t!("menu.window_select_tab_empty").to_string().into(), false, false),
        }
    };

    vec![
        Menu::new(t!("sidebar.app_name").to_string()).items([
            MenuItem::action(t!("menu.app_about").to_string(), About),
            MenuItem::separator(),
            MenuItem::action(t!("menu.app_settings").to_string(), ShowSettings),
            MenuItem::separator(),
            MenuItem::os_submenu(
                t!("menu.app_services").to_string(),
                SystemMenuType::Services,
            ),
            MenuItem::separator(),
            MenuItem::action(t!("menu.app_hide").to_string(), HideApplication),
            MenuItem::action(t!("menu.app_hide_others").to_string(), HideOthers),
            MenuItem::action(t!("menu.app_show_all").to_string(), ShowAll),
            MenuItem::separator(),
            MenuItem::action(t!("menu.app_quit").to_string(), Quit),
        ]),
        Menu::new(t!("menu.catalog_title").to_string()).items([
            MenuItem::action(t!("menu.catalog_add_collection").to_string(), AddCollection),
            MenuItem::separator(),
            MenuItem::action(t!("menu.catalog_reload").to_string(), ReloadCatalog),
            MenuItem::action(
                t!("menu.catalog_refresh_thumbnails").to_string(),
                RefreshThumbnails,
            ),
            MenuItem::action(
                t!("menu.catalog_check_availability").to_string(),
                CheckItemAvailability,
            ),
        ]),
        Menu::new(t!("menu.edit_title").to_string()).items([
            MenuItem::os_action(t!("menu.edit_undo").to_string(), Undo, OsAction::Undo),
            MenuItem::os_action(t!("menu.edit_redo").to_string(), Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action(t!("menu.edit_cut").to_string(), Cut, OsAction::Cut),
            MenuItem::os_action(t!("menu.edit_copy").to_string(), Copy, OsAction::Copy),
            MenuItem::os_action(t!("menu.edit_paste").to_string(), Paste, OsAction::Paste),
            MenuItem::os_action(
                t!("menu.edit_select_all").to_string(),
                SelectAll,
                OsAction::SelectAll,
            ),
        ]),
        Menu::new(t!("menu.view_title").to_string()).items([
            MenuItem::action(t!("menu.view_full_screen").to_string(), ToggleFullscreen),
            MenuItem::separator(),
            MenuItem::submenu(
                Menu::new(t!("menu.view_mode_title").to_string()).items([
                    MenuItem::action(t!("menu.view_as_list").to_string(), ViewAsList)
                        .checked(state.presentation == CatalogPresentation::List),
                    MenuItem::action(t!("menu.view_as_thumbs").to_string(), ViewAsThumbs)
                        .checked(state.presentation == CatalogPresentation::Thumbs),
                    MenuItem::action(t!("menu.view_as_grid").to_string(), ViewAsGrid)
                        .checked(state.presentation == CatalogPresentation::Grid),
                ]),
            ),
            MenuItem::submenu(
                Menu::new(t!("menu.sort_title").to_string()).items([
                    MenuItem::action(t!("menu.sort_by_title").to_string(), SortByTitle)
                        .checked(sort_checked(SortMethod::Title)),
                    MenuItem::action(t!("menu.sort_by_publisher").to_string(), SortByPublisher)
                        .checked(sort_checked(SortMethod::Publisher)),
                    MenuItem::action(t!("menu.sort_by_date_added").to_string(), SortByDateAdded)
                        .checked(sort_checked(SortMethod::DateAdded)),
                    MenuItem::action(t!("menu.sort_by_pages").to_string(), SortByPages)
                        .checked(sort_checked(SortMethod::PageCount)),
                    MenuItem::separator(),
                    MenuItem::action(t!("menu.sort_ascending").to_string(), SortAscending)
                        .checked(state.sort_direction == SortDirection::Ascending),
                    MenuItem::action(t!("menu.sort_descending").to_string(), SortDescending)
                        .checked(state.sort_direction == SortDirection::Descending),
                    MenuItem::separator(),
                    MenuItem::action(
                        t!("menu.toggle_group_by_publisher").to_string(),
                        ToggleGroupByPublisher,
                    )
                    .checked(state.group_by == GroupBy::Publisher),
                    MenuItem::action(t!("menu.toggle_group_by_tag").to_string(), ToggleGroupByTag)
                        .checked(state.group_by == GroupBy::Tag),
                ]),
            ),
            MenuItem::separator(),
            MenuItem::action(t!("menu.focus_search").to_string(), FocusSearch),
            MenuItem::submenu(
                Menu::new(t!("menu.window_select_tab_title").to_string()).items([
                    {
                        let (label, enabled, checked) = tab_label(0);
                        MenuItem::action(label, SelectTab0)
                            .disabled(!enabled)
                            .checked(checked)
                    },
                    {
                        let (label, enabled, checked) = tab_label(1);
                        MenuItem::action(label, SelectTab1)
                            .disabled(!enabled)
                            .checked(checked)
                    },
                    {
                        let (label, enabled, checked) = tab_label(2);
                        MenuItem::action(label, SelectTab2)
                            .disabled(!enabled)
                            .checked(checked)
                    },
                    {
                        let (label, enabled, checked) = tab_label(3);
                        MenuItem::action(label, SelectTab3)
                            .disabled(!enabled)
                            .checked(checked)
                    },
                    {
                        let (label, enabled, checked) = tab_label(4);
                        MenuItem::action(label, SelectTab4)
                            .disabled(!enabled)
                            .checked(checked)
                    },
                    {
                        let (label, enabled, checked) = tab_label(5);
                        MenuItem::action(label, SelectTab5)
                            .disabled(!enabled)
                            .checked(checked)
                    },
                    {
                        let (label, enabled, checked) = tab_label(6);
                        MenuItem::action(label, SelectTab6)
                            .disabled(!enabled)
                            .checked(checked)
                    },
                    {
                        let (label, enabled, checked) = tab_label(7);
                        MenuItem::action(label, SelectTab7)
                            .disabled(!enabled)
                            .checked(checked)
                    },
                    {
                        let (label, enabled, checked) = tab_label(8);
                        MenuItem::action(label, SelectTab8)
                            .disabled(!enabled)
                            .checked(checked)
                    },
                    {
                        let (label, enabled, checked) = tab_label(9);
                        MenuItem::action(label, SelectTab9)
                            .disabled(!enabled)
                            .checked(checked)
                    },
                ]),
            ),
        ]),
        Menu::new(t!("menu.window_title").to_string()).items([
            MenuItem::action(t!("menu.window_minimize").to_string(), Minimize),
            MenuItem::action(t!("menu.window_zoom").to_string(), Zoom),
            MenuItem::separator(),
            MenuItem::action(t!("menu.window_show_activity").to_string(), ShowActivity),
            MenuItem::action(
                t!("menu.window_show_alert_history").to_string(),
                ShowAlertHistory,
            ),
        ]),
        Menu::new(t!("menu.help_title").to_string()).items([
            MenuItem::action(t!("menu.app_about").to_string(), About),
            MenuItem::separator(),
            MenuItem::action(t!("menu.help_report").to_string(), ReportFeatureBug),
        ]),
    ]
}
