//! Root view: composes sidebar, toolbar, catalog, and detail panel.
//!
//! Construction wiring is split out by concern into `wiring_inputs` (text
//! inputs and font pickers) and `wiring_subscriptions` (controller/event
//! subscriptions); window-opening actions live in `actions`; the render tree
//! lives in `render`. Each does `use super::*;` to reach this file's struct,
//! imports, and helpers.

use std::cell::Cell;
use std::rc::Rc;

use dtrpg_core::{
    credentials::{CredentialStore, KeyringCredentialStore},
    data::{
        auth_state::AuthState,
        constants::{KEYRING_API_KEY, KEYRING_SERVICE},
        enums::{CatalogPresentation, GroupBy, SelectionField},
        ui_state::UiState,
    },
    services::{LibraryService, collections::CollectionsService},
    util::sort::{SortDirection, SortMethod},
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext, ClipboardItem, Context, Entity, FocusHandle, Focusable,
    InteractiveElement, IntoElement, ParentElement, Pixels, Render, SharedString, Styled,
    WindowHandle, div, px,
};
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::{
    DialogAction, DialogClose, DialogFooter, DialogHeader, DialogTitle,
};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::{Notification, NotificationType};
use gpui_kit::component::resizable::{ResizableState, h_resizable, resizable_panel};
use gpui_kit::component::searchable_list::SearchableListDelegate as _;
use gpui_kit::component::select::{SelectEvent, SelectState};
use gpui_kit::component::{IconName, Sizable as _};
use rust_i18n::t;

use self::wiring_subscriptions::RootViewEntities;
use crate::ui::actions::{
    About, AddCollection, CheckItemAvailability, FocusSearch, RefreshThumbnails, ReloadCatalog,
    ReportFeatureBug, SelectTab0, SelectTab1, SelectTab2, SelectTab3, SelectTab4, SelectTab5,
    SelectTab6, SelectTab7, SelectTab8, SelectTab9, ShowActivity, ShowAlertHistory, ShowSettings,
    SortAscending, SortByDateAdded, SortByPages, SortByPublisher, SortByTitle, SortDescending,
    ToggleGroupByPublisher, ToggleGroupByTag, ViewAsGrid, ViewAsList, ViewAsThumbs,
};
use crate::ui::app::{
    CollectionsServiceFactory, LoginServiceFactory, ServiceFactory, ViewMenuState, build_menus,
    open_about_window, open_settings_window, save_library_window_bounds,
};
use crate::ui::views::report_feature_bug_window_view::open_report_feature_bug_window;
use crate::ui::views::{
    bulk_action_bar_view::render_bulk_action_bar,
    catalog_view::CatalogView,
    detail_panel_view::render_detail_tab_content,
    notification_banner_view::render_notification_banner,
    sidebar_view::render_sidebar,
    status_bar_view::{ActivityBarData, StatusBarSnapshot, render_status_bar},
    tab_strip_view::render_tab_strip,
    title_bar_view::render_title_bar,
    toolbar_view::render_toolbar,
};
use crate::{
    controllers::{
        activity::ActivityController,
        auth_state::AuthStateController,
        library::LibraryController,
        settings::SettingsController,
        tabs::{TabTarget, TabsController, TabsSnapshot},
    },
    data::{
        events::{
            ActivityChanged, AuthStateChanged, CacheCleared, CollectionCreateFailed,
            CollectionMemberAddFailed, CollectionMemberAlreadyPresent,
            CollectionMemberRemoveFailed, DownloadComplete, DownloadError, LibraryChanged,
            LogoutRequested, LowDiskSpaceWarning, SettingsChanged, SignInSucceeded,
            StartupAuthBegun, StartupAuthFailed, TabsChanged, ThumbnailRefreshCompleted,
            ThumbnailRefreshNoOp, ThumbnailRefreshStarted,
        },
        theme::LibriTheme,
    },
};

/// Type-tag used to identify the startup-auth toast notification.
struct AuthPendingNotif;

/// Builds an error toast with a copy-to-clipboard action for the message
/// text, so the user can grab the full error (URLs, status codes, decode
/// errors) for a bug report without needing to open the alert history panel.
fn error_notification(message: impl Into<String>) -> Notification {
    let message = message.into();
    Notification::new().message(message.clone())
                       .with_type(NotificationType::Error)
                       .autohide(false)
                       .action(move |_notification, _window, _cx| {
                           let message = message.clone();
                           Button::new("copy-error-message")
                .icon(IconName::Copy)
                .ghost()
                .xsmall()
                .tooltip(t!("notification.copy_tooltip"))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(message.clone()));
                })
                       })
}

/// Builds a low-severity, auto-hiding toast for expected/non-fatal outcomes
/// (e.g. adding an item that is already a member of the target collection),
/// as opposed to [`error_notification`]'s persistent error styling.
fn warning_notification(message: impl Into<String>) -> Notification {
    Notification::new().message(message.into())
                       .with_type(NotificationType::Warning)
}

/// Resolves a Window-menu/`cmd-<n>` tab position to the `TabTarget` currently
/// open there, if any.
///
/// Position `0` always resolves to `open_tabs[0]` (the Catalog tab). Positions
/// `1..=9` resolve to `open_tabs[position]` — the 1st through 9th *detail*
/// tab, since Catalog occupies index `0` and is never re-targeted by
/// `cmd-1`..`cmd-9`.
fn tab_target_at(snapshot: &TabsSnapshot, position: usize) -> Option<TabTarget> {
    snapshot.open_tabs.get(position).cloned()
}

/// Top-level GPUI view for the Libri library window.
pub struct LibraryRootView {
    controller:                  Entity<LibraryController>,
    settings:                    Entity<SettingsController>,
    activity:                    Entity<ActivityController>,
    auth_state:                  Entity<AuthStateController>,
    catalog_view:                Entity<CatalogView>,
    /// Open/active tab state for the main-window tab strip.
    tabs:                        Entity<TabsController>,
    /// Resizable state for the two-column main layout (sidebar / catalog).
    resize_state:                Entity<ResizableState>,
    /// Restored or default sidebar panel initial width.
    sidebar_width:               f32,
    /// Default focus handle for the root div, ensuring menu-triggered actions
    /// always have a dispatch path even before any child element grabs focus.
    root_focus:                  FocusHandle,
    /// Handle to the currently-open settings window, if any. Used by the
    /// `ShowSettings` action to bring an already-open window to front instead
    /// of opening a duplicate; `None` when no settings window is open.
    settings_window:             Option<WindowHandle<Root>>,
    /// Handle to the currently-open "Report a Feature/Bug" window, if any.
    /// Used by the `ReportFeatureBug` action to bring an already-open window
    /// to front instead of opening a duplicate; `None` when no report window
    /// is open.
    report_feature_bug_window:   Option<WindowHandle<Root>>,
    /// Handle to the currently-open About window, if any. Used by the
    /// `About` action to bring an already-open window to front instead of
    /// opening a duplicate; `None` when no About window is open.
    about_window:                Option<WindowHandle<Root>>,
    /// Editable search input wired to the library controller's search query.
    search_input:                Entity<InputState>,
    /// Draft name input for the "Create Collection" dialog.
    collection_name_input:       Entity<InputState>,
    /// Inline search input for the sidebar's Publishers section.
    publisher_search_input:      Entity<InputState>,
    /// Inline search input for the sidebar's Collections section.
    collection_search_input:     Entity<InputState>,
    /// Draft extension input for the in-progress "add file opener" row in the
    /// File Openers settings list (inline, not a modal — see the subscription
    /// wired up in `new()` for the Enter/Blur commit behavior).
    file_opener_extension_input: Entity<InputState>,
    /// Pattern text input for the bulk-selection bar's "Select Matches"
    /// control.
    bulk_pattern_input:          Entity<InputState>,
    /// Field the bulk-selection pattern match is applied against, selected
    /// via the bulk-action bar's field dropdown.
    bulk_pattern_field:          Rc<Cell<SelectionField>>,
    /// Last `ViewMenuState` passed to `cx.set_menus`.
    ///
    /// `cx.set_menus` replaces the whole native menu bar and, on macOS, tears
    /// down any menu currently tracking a click — rebuilding it on every
    /// `LibraryChanged` (which also fires per-thumbnail during a catalog
    /// load) closes an open menu out from under the user and reads as
    /// flicker. Skip the rebuild when the checkmark-relevant state hasn't
    /// actually changed.
    last_menu_state:             Option<ViewMenuState>,
}

impl LibraryRootView {
    /// Constructs the root view and wires up the controller subscriptions.
    ///
    /// If `startup_api_key` is `Some`, a background re-authentication is
    /// started immediately. The window always opens in the unauthenticated
    /// state and transitions once auth completes.
    pub fn new(window: &mut gpui::Window, cx: &mut Context<Self>,
               service: Box<dyn LibraryService>,
               collections_service: Box<dyn CollectionsService>, startup_api_key: Option<String>)
               -> Self {
        let activity = cx.new(|_| ActivityController::new());
        let tabs = cx.new(|_| TabsController::new());
        let controller =
            cx.new(|cx| LibraryController::new(service, collections_service, activity.clone(), cx));
        let login_service = cx.global::<LoginServiceFactory>().0();
        let settings = cx.new(|cx| SettingsController::new(login_service, cx));

        wiring_inputs::wire_settings_inputs(window, cx, &settings, &controller);

        let catalog_view = cx.new(|cx| {
                                 CatalogView::new(window,
                                                  cx,
                                                  controller.clone(),
                                                  settings.clone(),
                                                  tabs.clone())
                             });
        let auth_state = cx.new(|_| AuthStateController::new(AuthState::Unauthenticated));
        let root_focus = cx.focus_handle();
        root_focus.focus(window, cx);

        let misc = wiring_inputs::build_misc_inputs(window, cx, &controller, &settings);

        let entities = RootViewEntities { controller: controller.clone(),
                                          tabs:       tabs.clone(),
                                          activity:   activity.clone(),
                                          settings:   settings.clone(),
                                          auth_state: auth_state.clone(), };
        wiring_subscriptions::wire_controller_subscriptions(window, cx, &entities, startup_api_key);

        let ui_state = UiState::load();
        let sidebar_width = ui_state.sidebar_width().unwrap_or(250.0);
        let resize_state = cx.new(|_| ResizableState::default());

        cx.subscribe(&resize_state, move |_this, state, _event, cx| {
              let sizes = state.read(cx).sizes().clone();
              if sizes.len() >= 2 {
                  let sidebar = sizes[0].as_f32();
                  UiState::load().save_sidebar_width(sidebar);
              }
          })
          .detach();

        // Save the window's position/size on every live resize/move, not
        // just at close time — some quit paths (e.g. Cmd+Q) don't reliably
        // fire `on_window_should_close`, which left the saved bounds stale.
        cx.observe_window_bounds(window, |_this, window, _cx| {
              save_library_window_bounds(window);
          })
          .detach();

        Self { controller,
               settings,
               activity,
               auth_state,
               catalog_view,
               tabs,
               resize_state,
               sidebar_width,
               root_focus,
               settings_window: None,
               report_feature_bug_window: None,
               about_window: None,
               search_input: misc.search_input,
               collection_name_input: misc.collection_name_input,
               file_opener_extension_input: misc.file_opener_extension_input,
               publisher_search_input: misc.publisher_search_input,
               collection_search_input: misc.collection_search_input,
               bulk_pattern_input: misc.bulk_pattern_input,
               bulk_pattern_field: misc.bulk_pattern_field,
               last_menu_state: None }
    }
}

impl Focusable for LibraryRootView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.root_focus.clone()
    }
}

impl Render for LibraryRootView {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        render::render_root_view(self, window, cx)
    }
}

mod actions;
mod render;
mod wiring_inputs;
mod wiring_subscriptions;
