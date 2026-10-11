use gpui::*;
use gpui_component::input::InputState;
use gpui_component::{Root, init};
use rust_i18n::t;
use tracing::{debug, warn};

use crate::controllers::settings::SettingsController;
use crate::controllers::tabs::{TabTarget, TabsController, TabsSnapshot};
use crate::credentials::{CredentialStore, KeyringCredentialStore};
use crate::data::constants::{DETAIL_TAB_TITLE_MAX_CHARS, KEYRING_API_KEY, KEYRING_SERVICE};
use crate::data::enums::CatalogPresentation;
use crate::data::ui_state::{UiState, WindowBoundsPref};
use crate::services::{LibraryService, LoginService, LoginTokens, collections::CollectionsService};
use crate::ui::actions::*;
use crate::ui::views::root_view::LibraryRootView;
use crate::ui::views::settings_window_view::SettingsWindowView;
use crate::util::init::init_globals;
use crate::util::sort::{SortDirection, SortMethod};
use crate::util::text::truncate_with_ellipsis;

/// Snapshot of catalog view state needed to render checkmarks in the native
/// menu bar's View menu (presentation mode, sort field/direction, and
/// grouping).
///
/// Rebuilt and passed to [`build_menus`] every time the catalog view state
/// changes, so the OS menu's checkmarks stay in sync with the
/// toolbar/keyboard-driven selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewMenuState {
    pub presentation:   CatalogPresentation,
    pub sort:           SortMethod,
    pub sort_direction: SortDirection,
    pub grouped:        bool,
}

/// Holds the factory closure used to create a [`LibraryService`] on demand.
///
/// `None` tokens mean the user is not authenticated; the factory should return
/// a service that reflects the unauthenticated state without crashing.
///
/// Set this global before calling [`setup`].
pub struct ServiceFactory(pub  Box<dyn Fn(Option<LoginTokens>) -> Box<dyn LibraryService>
                                       + Send
                                       + Sync
                                       + 'static>);

impl Global for ServiceFactory {}

/// Holds the factory closure used to create a [`CollectionsService`] on demand.
///
/// `None` tokens mean the user is not authenticated; the factory should return
/// a service that reflects the unauthenticated state without crashing.
///
/// Set this global before calling [`setup`].
pub struct CollectionsServiceFactory(pub  Box<dyn Fn(Option<LoginTokens>)
                                                     -> Box<dyn CollectionsService>
                                                  + Send
                                                  + Sync
                                                  + 'static>);

impl Global for CollectionsServiceFactory {}

/// Holds the factory closure used to create a [`LoginService`] on demand.
///
/// Used by [`SettingsController`] to authenticate the user from the Account
/// tab.
pub struct LoginServiceFactory(pub Box<dyn Fn() -> Box<dyn LoginService> + Send + Sync + 'static>);

impl Global for LoginServiceFactory {}

/// Handle to the library window and its root view, set once the window opens.
///
/// Lets app-level action fallbacks in [`setup`] (registered via `cx.on_action`,
/// which fire regardless of which element currently has keyboard focus) reach
/// the library window's state even when no element in its focus/dispatch path
/// owns the relevant `on_action` handler — see the
/// `ShowSettings`/`ShowActivity`/ `ShowAlertHistory`/`AddCollection` fallbacks
/// below.
struct LibraryWindowHandle {
    view:   Entity<LibraryRootView>,
    window: WindowHandle<Root>,
}

impl Global for LibraryWindowHandle {}

/// Opens the library window in the unauthenticated state.
///
/// The window opens immediately. If `startup_api_key` is `Some`, the root view
/// kicks off a background re-authentication and transitions to authenticated on
/// success.
///
/// # Panics
///
/// Panics if the window cannot be opened or if `ServiceFactory` has not been
/// set.
#[allow(clippy::expect_used)]
pub fn open_library_window(startup_api_key: Option<String>, cx: &mut App) {
    let service = (cx.global::<ServiceFactory>().0)(None);
    let collections_service = (cx.global::<CollectionsServiceFactory>().0)(None);
    let mut library_view = None;
    let window_bounds = restore_library_window_bounds(cx);
    debug!(?window_bounds, "opening library window");
    let window = cx
        .open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(t!("sidebar.app_name").to_string().into()),
                    appears_transparent: true,
                    // Vertically centers the traffic lights within
                    // `title_bar_view::render_title_bar`'s 44px-tall custom
                    // row — macOS's own default position assumes the
                    // standard ~28px system title bar, which sits the
                    // buttons noticeably above center in our taller row.
                    traffic_light_position: Some(point(px(12.0), px(14.0))),
                }),
                window_bounds,
                ..Default::default()
            },
            |window, cx| {
                window.on_window_should_close(cx, move |window, _cx| {
                    save_library_window_bounds(window);
                    true
                });
                let view = cx.new(|cx| {
                    LibraryRootView::new(window, cx, service, collections_service, startup_api_key)
                });
                library_view = Some(view.clone());
                cx.new(|cx| Root::new(view, window, cx).bordered(false))
            },
        )
        .expect("failed to open library window");
    if let Some(view) = library_view {
        cx.set_global(LibraryWindowHandle { view, window });
    }
}

/// Resolves the library window's persisted position/size into a
/// `WindowBounds`, or `None` (falling back to the OS default placement) if
/// nothing was ever saved, or the saved bounds no longer intersect any
/// currently connected display.
fn restore_library_window_bounds(cx: &App) -> Option<WindowBounds> {
    let saved = UiState::load().library_window_bounds()?;
    let bounds = Bounds { origin: Point { x: px(saved.x),
                                          y: px(saved.y), },
                          size:   Size { width:  px(saved.width),
                                         height: px(saved.height), }, };
    let display_bounds: Vec<_> = cx.displays().iter().map(|d| d.bounds()).collect();
    let fits_a_display = display_bounds.iter().any(|db| bounds.intersects(db));
    debug!(?saved,
           ?bounds,
           ?display_bounds,
           fits_a_display,
           "restoring library window bounds");
    fits_a_display.then_some(WindowBounds::Windowed(bounds))
}

/// Saves the library window's current position/size to `UiState`.
///
/// Called both on every live resize/move (via `LibraryRootView`'s
/// `observe_window_bounds` subscription) and once more just before the
/// window closes (mirroring `open_settings_window`'s existing
/// `on_window_should_close` pattern), so the saved bounds stay current even
/// if the close hook doesn't fire for a given quit path.
pub fn save_library_window_bounds(window: &Window) {
    let bounds = window.bounds();
    debug!(?bounds, "saving library window bounds");
    UiState::load().save_library_window_bounds(WindowBoundsPref { x:      bounds.origin.x.as_f32(),
                                                                  y:      bounds.origin.y.as_f32(),
                                                                  width:  bounds.size
                                                                                .width
                                                                                .as_f32(),
                                                                  height: bounds.size
                                                                                .height
                                                                                .as_f32(), });
}

/// Opens the settings window as a separate, non-modal window.
///
/// Shares `settings` and `library` with the caller's `LibraryRootView` rather
/// than constructing new controllers, so drafts/tab state (and, for
/// `library`, the Appearance page's font/theme setters) persist across
/// close/reopen. Wires the window's close event to `SettingsController::close`
/// so the panel-visibility state stays in sync even when the user closes the
/// window via its native close control rather than Escape.
///
/// # Panics
///
/// Panics if the window cannot be opened.
#[allow(clippy::expect_used)]
pub fn open_settings_window(settings: Entity<SettingsController>,
                            library: Entity<crate::controllers::library::LibraryController>,
                            file_opener_extension_input: Entity<InputState>, cx: &mut App)
                            -> WindowHandle<Root> {
    let settings_for_close = settings.clone();
    let window_size = Size { width:  px(720.),
                             height: px(520.), };
    cx.open_window(WindowOptions { // `appears_transparent: true` hides the native opaque title
                                   // bar and title text (same
                                   // pattern as the main library window's
                                   // `title_bar_view.rs`) while keeping the macOS traffic-light
                                   // window controls, which `titlebar: None` removes entirely on
                                   // this platform (there is no dedicated close-only chrome
                                   // option
                                   // in gpui). `is_minimizable: false` and `is_resizable: false`
                                   // disable the minimize and zoom buttons, leaving only close —
                                   // resizing is no longer needed now that the panel content
                                   // scrolls (see `settings_view::render_settings_panel`).
                                   titlebar: Some(TitlebarOptions { appears_transparent:
                                                                        true,
                                                                    // Vertically centers the
                                                                    // traffic lights within
                                                                    // `settings_view.rs`'s
                                                                    // 28px-tall `settings-drag-region`
                                                                    // —
                                                                    // see the main window's
                                                                    // equivalent comment in
                                                                    // `open_library_window`
                                                                    // for the (28 - 16) / 2
                                                                    // math.
                                                                    traffic_light_position:
                                                                        Some(point(px(12.0),
                                                                                   px(6.0))),
                                                                    ..Default::default() }),
                                   window_bounds: Some(WindowBounds::centered(window_size, cx)),
                                   is_resizable: false,
                                   is_minimizable: false,
                                   ..Default::default() },
                   move |window, cx| {
                       window.on_window_should_close(cx, move |_window, cx| {
                                 settings_for_close.update(cx, |ctrl, cx| ctrl.close(cx));
                                 true
                             });
                       let view = cx.new(|cx| {
                                        SettingsWindowView::new(window,
                                                                cx,
                                                                settings,
                                                                library,
                                                                file_opener_extension_input)
                                    });
                       cx.new(|cx| Root::new(view, window, cx).bordered(false))
                   })
      .expect("failed to open settings window")
}

/// Initializes the GPUI application and routes to the login or library window.
///
/// Checks the platform keyring for a stored API key. Opens the library window
/// when credentials are found; falls back to the login window otherwise.
pub fn setup(cx: &mut App) {
    init(cx);
    init_globals(cx);

    // Apply the persisted (or default) body font and sync gpui-component's
    // theme colors with the active Libri theme; otherwise buttons, inputs,
    // popovers, scrollbars, the sidebar, and the catalog table all render
    // with gpui-component's default light colors regardless of which Libri
    // theme is active.
    let initial_theme = cx.global::<crate::data::theme::LibriTheme>().clone();
    cx.update_global::<gpui_component::Theme, _>(|theme, _cx| {
          theme.font_family = initial_theme.fonts.body_font.clone();
          crate::data::theme::apply_theme_colors(theme, &initial_theme.colors);
      });

    // Key bindings
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None),
                  KeyBinding::new("cmd-,", ShowSettings, None),
                  KeyBinding::new("cmd-h", HideApplication, None),
                  KeyBinding::new("alt-cmd-h", HideOthers, None),
                  KeyBinding::new("cmd-m", Minimize, None),
                  KeyBinding::new("ctrl-cmd-f", ToggleFullscreen, None),
                  KeyBinding::new("cmd-shift-n", AddCollection, None),
                  KeyBinding::new("cmd-r", ReloadCatalog, None),
                  KeyBinding::new("cmd-shift-r", RefreshThumbnails, None),
                  KeyBinding::new("cmd-0", SelectTab0, None),
                  KeyBinding::new("cmd-1", SelectTab1, None),
                  KeyBinding::new("cmd-2", SelectTab2, None),
                  KeyBinding::new("cmd-3", SelectTab3, None),
                  KeyBinding::new("cmd-4", SelectTab4, None),
                  KeyBinding::new("cmd-5", SelectTab5, None),
                  KeyBinding::new("cmd-6", SelectTab6, None),
                  KeyBinding::new("cmd-7", SelectTab7, None),
                  KeyBinding::new("cmd-8", SelectTab8, None),
                  KeyBinding::new("cmd-9", SelectTab9, None)]);

    // App-level action handlers
    cx.on_action::<Quit>(|_, cx| cx.quit());
    cx.on_action::<HideApplication>(|_, cx| cx.hide());
    cx.on_action::<HideOthers>(|_, cx| cx.hide_other_apps());
    // The real handler lives on `LibraryRootView` (opens the About dialog).
    // This is a harmless fallback in case the action fires before any
    // window has focus.
    cx.on_action::<About>(|_, _cx| {});
    // Same fallback shape, for the "Report a Feature/Bug" dialog — see
    // `LibraryRootView`'s `ReportFeatureBug` handler.
    cx.on_action::<ReportFeatureBug>(|_, _cx| {});
    cx.on_action::<Minimize>(|_, cx| {
          if let Some(win) = cx.active_window() {
              win.update(cx, |_, window, _| window.minimize_window()).ok();
          }
      });
    cx.on_action::<Zoom>(|_, cx| {
          if let Some(win) = cx.active_window() {
              win.update(cx, |_, window, _| window.zoom_window()).ok();
          }
      });
    cx.on_action::<ToggleFullscreen>(|_, cx| {
          if let Some(win) = cx.active_window() {
              win.update(cx, |_, window, _| window.toggle_fullscreen())
                 .ok();
          }
      });
    // The following fallbacks always keep Settings/Activity/Alert
    // History/Add Collection enabled in the native menu, regardless of which
    // element in the library window currently owns keyboard focus (native
    // menu-item enablement is resolved by walking up from the focused
    // dispatch node — see `LibraryWindowHandle`'s doc comment). The element-
    // level `on_action` handlers on `LibraryRootView`'s root div remain the
    // primary path and run first; these only fire when nothing in the
    // window's dispatch path claims the action.
    cx.on_action::<ShowSettings>(|_, cx| {
          let Some(view) = cx.try_global::<LibraryWindowHandle>()
                             .map(|h| h.view.clone())
          else {
              return;
          };
          view.update(cx, |view, cx| view.show_settings(cx));
      });
    cx.on_action::<ShowActivity>(|_, cx| {
          let Some(view) = cx.try_global::<LibraryWindowHandle>()
                             .map(|h| h.view.clone())
          else {
              return;
          };
          view.update(cx, |view, cx| view.show_activity(cx));
      });
    cx.on_action::<ShowAlertHistory>(|_, cx| {
          let Some(view) = cx.try_global::<LibraryWindowHandle>()
                             .map(|h| h.view.clone())
          else {
              return;
          };
          view.update(cx, |view, cx| view.show_alert_history(cx));
      });
    cx.on_action::<AddCollection>(|_, cx| {
          let Some((view, window)) = cx.try_global::<LibraryWindowHandle>()
                                       .map(|h| (h.view.clone(), h.window))
          else {
              return;
          };
          window.update(cx, |_, window, cx| {
                    view.update(cx, |view, cx| view.show_add_collection_dialog(window, cx));
                })
                .ok();
      });

    // Menu bar
    cx.set_menus(build_menus(&ViewMenuState::default(), &TabsController::new().snapshot()));

    let startup_api_key = match KeyringCredentialStore::new(KEYRING_SERVICE, KEYRING_API_KEY).load()
    {
        Ok(Some(cred)) => Some(cred.secret),
        Ok(None) => None,
        Err(e) => {
            warn!("keyring check failed: {e}");
            None
        }
    };

    open_library_window(startup_api_key, cx);
    cx.activate(true);
}

/// Builds the full native menu bar, applying checkmarks to the View menu's
/// Presentation and Sort submenus (and the Group toggle) based on `state`, and
/// building the Window menu's ten tab-selection items from `tabs`.
///
/// `cx.set_menus` replaces the entire menu bar on every call, so this must
/// reconstruct the whole bar rather than just the affected submenus. Called
/// once at startup with the default state, and again by `LibraryRootView`
/// whenever the catalog's presentation/sort/grouping changes or the open tab
/// set changes, so the OS menu's checkmarks and tab-selection items track the
/// toolbar/tab-strip's current state.
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
                    .checked(state.grouped),
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
