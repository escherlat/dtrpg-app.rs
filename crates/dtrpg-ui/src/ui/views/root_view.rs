//! Root view: composes sidebar, toolbar, catalog, and detail panel.

use std::cell::Cell;
use std::rc::Rc;

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

use crate::ui::actions::{
    About, AddCollection, CheckItemAvailability, FocusSearch, RefreshThumbnails, ReloadCatalog,
    ReportFeatureBug, SelectTab0, SelectTab1, SelectTab2, SelectTab3, SelectTab4, SelectTab5,
    SelectTab6, SelectTab7, SelectTab8, SelectTab9, ShowActivity, ShowAlertHistory, ShowSettings,
    SortAscending, SortByDateAdded, SortByPages, SortByPublisher, SortByTitle, SortDescending,
    ToggleGroupByPublisher, ViewAsGrid, ViewAsList, ViewAsThumbs,
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
    credentials::{CredentialStore, KeyringCredentialStore},
    data::{
        auth_state::AuthState,
        constants::{KEYRING_API_KEY, KEYRING_SERVICE},
        enums::{CatalogPresentation, SelectionField},
        events::{
            ActivityChanged, AuthStateChanged, CacheCleared, CollectionCreateFailed,
            CollectionMemberAddFailed, CollectionMemberAlreadyPresent,
            CollectionMemberRemoveFailed, DownloadComplete, DownloadError, LibraryChanged,
            LogoutRequested, LowDiskSpaceWarning, SettingsChanged, SignInSucceeded,
            StartupAuthBegun, StartupAuthFailed, TabsChanged, ThumbnailRefreshCompleted,
            ThumbnailRefreshNoOp, ThumbnailRefreshStarted,
        },
        theme::LibriTheme,
        ui_state::UiState,
    },
    services::{LibraryService, collections::CollectionsService},
    util::sort::{SortDirection, SortMethod},
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

        let email_initial = settings.read(cx).email_draft().to_owned();
        let email_input = cx.new(|cx| {
                                let mut state = InputState::new(window, cx)
                .placeholder(t!("settings.email_placeholder").to_string());
                                if !email_initial.is_empty() {
                                    state = state.default_value(&email_initial);
                                }
                                state
                            });
        let settings_for_email = settings.clone();
        cx.subscribe(&email_input,
                     move |_this, input_entity, event: &InputEvent, cx| {
                         if matches!(event, InputEvent::Change) {
                             let value = input_entity.read(cx).value().to_string();
                             settings_for_email.update(cx, |ctrl, cx| {
                                                   ctrl.set_email_draft(value, cx)
                                               });
                         }
                     })
          .detach();
        settings.update(cx, |ctrl, _cx| ctrl.set_email_input(email_input));

        let password_input = cx.new(|cx| {
                                   InputState::new(window, cx)
                .placeholder(t!("settings.password_placeholder").to_string())
                .masked(true)
                               });
        let settings_for_password = settings.clone();
        cx.subscribe(&password_input,
                     move |_this, input_entity, event: &InputEvent, cx| {
                         if matches!(event, InputEvent::Change) {
                             let value = input_entity.read(cx).value().to_string();
                             settings_for_password.update(cx, |ctrl, cx| {
                                                      ctrl.set_password_draft(value, cx)
                                                  });
                         }
                     })
          .detach();
        settings.update(cx, |ctrl, _cx| ctrl.set_password_input(password_input));

        let storage_path_placeholder = {
            use crate::data::storage::StorageConfig;
            StorageConfig::load().root_path()
                                 .to_string_lossy()
                                 .into_owned()
        };
        let storage_path_input =
            cx.new(|cx| InputState::new(window, cx).default_value(&storage_path_placeholder));
        let settings_for_storage = settings.clone();
        cx.subscribe(&storage_path_input,
                     move |_this, input_entity, event: &InputEvent, cx| {
                         if matches!(event, InputEvent::Change) {
                             let value = input_entity.read(cx).value().to_string();
                             settings_for_storage.update(cx, |ctrl, cx| {
                                                     ctrl.set_storage_path_draft(value, cx)
                                                 });
                         }
                     })
          .detach();
        settings.update(cx, |ctrl, _cx| {
                    ctrl.set_storage_path_input(storage_path_input)
                });

        let recently_updated_window_days_initial = {
            use crate::data::storage::StorageConfig;
            StorageConfig::load().recently_updated_window_days()
        };
        let recently_updated_window_input = cx.new(|cx| {
                                                  InputState::new(window, cx)
                .default_value(recently_updated_window_days_initial.to_string())
                .min(f64::from(
                    crate::data::constants::RECENTLY_UPDATED_WINDOW_MIN_DAYS,
                ))
                .max(f64::from(
                    crate::data::constants::RECENTLY_UPDATED_WINDOW_MAX_DAYS,
                ))
                                              });
        let settings_for_window = settings.clone();
        cx.subscribe_in(&recently_updated_window_input,
                        window,
                        move |_this, input_entity, event: &InputEvent, window, cx| {
                            match event {
                                InputEvent::Change => {
                                    let value = input_entity.read(cx).value().to_string();
                                    if let Ok(days) = value.parse::<u32>() {
                                        settings_for_window.update(cx, |ctrl, cx| {
                                            ctrl.set_recently_updated_window_days(days, cx);
                                        });
                                    }
                                }
                                InputEvent::Blur => {
                                    // `NumberInput`'s own min/max clamp only
                                    // corrects the
                                    // displayed text in some focus-loss paths;
                                    // explicitly
                                    // re-syncing the field to the persisted
                                    // (already-clamped)
                                    // value on every blur guarantees the
                                    // display never shows an
                                    // out-of-range number regardless of that
                                    // widget-internal
                                    // timing.
                                    let clamped = settings_for_window.read(cx)
                                                                     .snapshot()
                                                                     .recently_updated_window_days;
                                    let current = input_entity.read(cx).value().to_string();
                                    if current != clamped.to_string() {
                                        input_entity.update(cx, |state, cx| {
                                                        state.set_value(clamped.to_string(),
                                                                        window,
                                                                        cx);
                                                    });
                                    }
                                }
                                _ => {}
                            }
                        })
          .detach();
        settings.update(cx, |ctrl, _cx| {
                    ctrl.set_recently_updated_window_input(recently_updated_window_input)
                });

        let max_concurrent_downloads_initial = {
            use crate::data::storage::StorageConfig;
            StorageConfig::load().max_concurrent_downloads()
        };
        let max_concurrent_downloads_input = cx.new(|cx| {
                                                   use crate::ui::views::settings_storage_view::{
                                                       MAX_CONCURRENT_DOWNLOADS,
                                                       MIN_CONCURRENT_DOWNLOADS,
                                                   };
                                                   InputState::new(window, cx)
                .default_value(max_concurrent_downloads_initial.to_string())
                .min(MIN_CONCURRENT_DOWNLOADS as f64)
                .max(MAX_CONCURRENT_DOWNLOADS as f64)
                                               });
        let settings_for_concurrency_input = settings.clone();
        cx.subscribe(&max_concurrent_downloads_input,
                     move |_this, input_entity, event: &InputEvent, cx| {
                         if matches!(event, InputEvent::Change) {
                             let value = input_entity.read(cx).value().to_string();
                             if let Ok(n) = value.parse::<usize>() {
                                 settings_for_concurrency_input.update(cx, |ctrl, cx| {
                                     ctrl.set_max_concurrent_downloads(n, cx);
                                 });
                             }
                         }
                     })
          .detach();
        settings.update(cx, |ctrl, _cx| {
                    ctrl.set_max_concurrent_downloads_input(max_concurrent_downloads_input)
                });

        // ── Appearance page font pickers ─────────────────────────────────────
        //
        // Built once here (like the inputs above) rather than in
        // `SettingsWindowView`, so the picker state — and any installed-font
        // enumeration cost — survives the settings window closing and
        // reopening within the session. Any font actually installed on the
        // user's system is selectable, not a curated list — see
        // `settings_appearance_view`.
        let mut installed_fonts = cx.text_system().all_font_names();
        installed_fonts.sort();
        installed_fonts.dedup();

        let current_fonts = cx.global::<LibriTheme>().fonts.clone();

        let body_selected = installed_fonts.position(&current_fonts.body_font.to_string());
        let body_font_select = cx.new(|cx| {
                                     SelectState::new(installed_fonts.clone(),
                                                      body_selected,
                                                      window,
                                                      cx).searchable(true)
                                 });
        let controller_for_body_font = controller.clone();
        cx.subscribe(&body_font_select,
                     move |_this, _state, event: &SelectEvent<Vec<String>>, cx| {
                         let SelectEvent::Confirm(Some(name)) = event
                         else {
                             return;
                         };
                         controller_for_body_font.update(cx, |ctrl, cx| {
                             ctrl.set_body_font(SharedString::from(name.clone()), cx);
                         });
                     })
          .detach();
        settings.update(cx, |ctrl, _cx| ctrl.set_body_font_select(body_font_select));

        let value_selected = installed_fonts.position(&current_fonts.value_font.to_string());
        let value_font_select = cx.new(|cx| {
                                      SelectState::new(installed_fonts.clone(),
                                                       value_selected,
                                                       window,
                                                       cx).searchable(true)
                                  });
        let controller_for_value_font = controller.clone();
        cx.subscribe(&value_font_select,
                     move |_this, _state, event: &SelectEvent<Vec<String>>, cx| {
                         let SelectEvent::Confirm(Some(name)) = event
                         else {
                             return;
                         };
                         controller_for_value_font.update(cx, |ctrl, cx| {
                             ctrl.set_value_font(SharedString::from(name.clone()), cx);
                         });
                     })
          .detach();
        settings.update(cx, |ctrl, _cx| {
                    ctrl.set_value_font_select(value_font_select)
                });

        let label_selected = installed_fonts.position(&current_fonts.label_font.to_string());
        let label_font_select = cx.new(|cx| {
                                      SelectState::new(installed_fonts.clone(),
                                                       label_selected,
                                                       window,
                                                       cx).searchable(true)
                                  });
        let controller_for_label_font = controller.clone();
        cx.subscribe(&label_font_select,
                     move |_this, _state, event: &SelectEvent<Vec<String>>, cx| {
                         let SelectEvent::Confirm(Some(name)) = event
                         else {
                             return;
                         };
                         controller_for_label_font.update(cx, |ctrl, cx| {
                             ctrl.set_label_font(SharedString::from(name.clone()), cx);
                         });
                     })
          .detach();
        settings.update(cx, |ctrl, _cx| {
                    ctrl.set_label_font_select(label_font_select)
                });

        let mono_selected = installed_fonts.position(&current_fonts.mono_font.to_string());
        let mono_font_select =
            cx.new(|cx| {
                  SelectState::new(installed_fonts, mono_selected, window, cx).searchable(true)
              });
        let controller_for_mono_font = controller.clone();
        cx.subscribe(&mono_font_select,
                     move |_this, _state, event: &SelectEvent<Vec<String>>, cx| {
                         let SelectEvent::Confirm(Some(name)) = event
                         else {
                             return;
                         };
                         controller_for_mono_font.update(cx, |ctrl, cx| {
                             ctrl.set_mono_font(SharedString::from(name.clone()), cx);
                         });
                     })
          .detach();
        settings.update(cx, |ctrl, _cx| ctrl.set_mono_font_select(mono_font_select));

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

        let search_input =
            cx.new(|cx| {
                  InputState::new(window, cx).placeholder(t!("search.placeholder").to_string())
              });

        let controller_for_search = controller.clone();
        cx.subscribe(&search_input,
                     move |_this, input_entity, event: &InputEvent, cx| {
                         if matches!(event, InputEvent::Change) {
                             let value = input_entity.read(cx).value().to_string();
                             controller_for_search.update(cx, |ctrl, cx| {
                                                      ctrl.set_search_query(value, cx);
                                                  });
                         }
                     })
          .detach();

        let bulk_pattern_input = cx.new(|cx| {
                                       InputState::new(window, cx)
                .placeholder(t!("toolbar.select_matches_placeholder").to_string())
                                   });
        let bulk_pattern_field = Rc::new(Cell::new(SelectionField::Title));

        let collection_name_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("collections.name_placeholder").to_string())
        });
        let file_opener_extension_input = cx.new(|cx| {
                                                InputState::new(window, cx)
                .placeholder(t!("settings.file_opener_extension_placeholder").to_string())
                                            });

        let publisher_search_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("search.publishers_placeholder").to_string())
        });
        let controller_for_publisher_search = controller.clone();
        cx.subscribe(&publisher_search_input,
                     move |_this, input_entity, event: &InputEvent, cx| {
                         if matches!(event, InputEvent::Change) {
                             let value = input_entity.read(cx).value().to_string();
                             controller_for_publisher_search.update(cx, |ctrl, cx| {
                                 ctrl.set_publisher_search_query(value, cx);
                             });
                         }
                     })
          .detach();

        let collection_search_input = cx.new(|cx| {
                                            InputState::new(window, cx)
                .placeholder(t!("search.collections_placeholder").to_string())
                                        });
        let controller_for_collection_search = controller.clone();
        cx.subscribe(&collection_search_input,
                     move |_this, input_entity, event: &InputEvent, cx| {
                         if matches!(event, InputEvent::Change) {
                             let value = input_entity.read(cx).value().to_string();
                             controller_for_collection_search.update(cx, |ctrl, cx| {
                                 ctrl.set_collection_search_query(value, cx);
                             });
                         }
                     })
          .detach();

        // Commit the pending "add file opener" row on Enter; on blur, commit if
        // the user typed something, otherwise discard the pending row
        // (matches clicking elsewhere in most inline-add UIs — no
        // separate confirm step needed).
        let settings_for_ext = settings.clone();
        cx.subscribe(&file_opener_extension_input,
                     move |_this, input_entity, event: &InputEvent, cx| match event {
                         InputEvent::PressEnter { .. } => {
                             let value = input_entity.read(cx).value().to_string();
                             settings_for_ext.update(cx, |ctrl, cx| {
                                                 ctrl.commit_pending_file_opener(&value, cx)
                                             });
                         }
                         InputEvent::Blur => {
                             let value = input_entity.read(cx).value().to_string();
                             settings_for_ext.update(cx, |ctrl, cx| {
                                                 if value.trim().is_empty() {
                                                     ctrl.cancel_pending_file_opener(cx);
                                                 }
                                                 else {
                                                     ctrl.commit_pending_file_opener(&value, cx);
                                                 }
                                             });
                         }
                         _ => {}
                     })
          .detach();

        cx.subscribe_in(&controller,
                        window,
                        |_this, _ctrl, event: &CollectionCreateFailed, window, cx| {
                            window.push_notification(error_notification(event.message.clone()), cx);
                        })
          .detach();

        cx.subscribe_in(&controller,
                        window,
                        |_this, _ctrl, event: &CollectionMemberAddFailed, window, cx| {
                            window.push_notification(error_notification(event.message.clone()), cx);
                        })
          .detach();

        cx.subscribe_in(&controller,
                        window,
                        |_this, _ctrl, event: &CollectionMemberRemoveFailed, window, cx| {
                            window.push_notification(error_notification(event.message.clone()), cx);
                        })
          .detach();

        cx.subscribe_in(&controller,
                        window,
                        |_this, _ctrl, event: &CollectionMemberAlreadyPresent, window, cx| {
                            window.push_notification(warning_notification(event.message.clone()),
                                                     cx);
                        })
          .detach();

        cx.subscribe_in(&controller,
                        window,
                        |_this, _ctrl, event: &ThumbnailRefreshStarted, window, cx| {
                            let msg = t!("notification.refresh_thumbnails_started",
                                         count = event.count).to_string();
                            window.push_notification(Notification::info(msg), cx);
                        })
          .detach();

        cx.subscribe_in(&controller,
                        window,
                        |_this, _ctrl, event: &ThumbnailRefreshCompleted, window, cx| {
                            if event.failed == 0 {
                                let msg = t!("notification.refresh_thumbnails_completed",
                                             succeeded = event.succeeded).to_string();
                                window.push_notification(Notification::success(msg), cx);
                            }
                            else {
                                let msg =
                                    t!("notification.refresh_thumbnails_completed_with_failures",
                                       succeeded = event.succeeded,
                                       failed = event.failed).to_string();
                                window.push_notification(warning_notification(msg), cx);
                            }
                        })
          .detach();

        cx.subscribe_in(&controller,
                        window,
                        |_this, _ctrl, _event: &ThumbnailRefreshNoOp, window, cx| {
                            let msg = t!("notification.refresh_thumbnails_noop").to_string();
                            window.push_notification(warning_notification(msg), cx);
                        })
          .detach();

        let controller_for_disk_space = controller.clone();
        cx.subscribe_in(&controller,
                        window,
                        move |_this, _ctrl, event: &LowDiskSpaceWarning, window, cx| {
                            let needed = format!("{:.0}", event.needed_mb);
                            let free = format!("{:.0}", event.free_mb);
                            let confirm_entity = controller_for_disk_space.clone();
                            let cancel_entity = controller_for_disk_space.clone();
                            window.open_alert_dialog(cx, move |alert, _, _| {
                                      let confirm_entity = confirm_entity.clone();
                                      let cancel_entity = cancel_entity.clone();
                                      alert
                        .confirm()
                        .title(t!("catalog.low_disk_space_confirm_title").to_string())
                        .description(
                            t!(
                                "catalog.low_disk_space_confirm_description",
                                needed = needed.clone(),
                                free = free.clone()
                            )
                            .to_string(),
                        )
                        .on_ok(move |_, _window, cx| {
                            confirm_entity.update(cx, |ctrl, cx| ctrl.confirm_pending_download(cx));
                            true
                        })
                        .on_cancel(move |_, _window, cx| {
                            cancel_entity.update(cx, |ctrl, cx| ctrl.cancel_pending_download(cx));
                            true
                        })
                                  });
                        })
          .detach();

        cx.subscribe(&controller, |this, ctrl, _event: &LibraryChanged, cx| {
              // Keep the native View menu's checkmarks (presentation, sort,
              // grouping) in sync with the
              // toolbar/keyboard-driven selection. `LibraryChanged`
              // fires far more often than the checkmark-relevant state actually
              // changes (e.g. once per thumbnail during a catalog load), and
              // `cx.set_menus` replaces the whole native menu bar — on macOS
              // that tears down any menu currently tracking a
              // click, which read as the app menu flickering or
              // closing itself right after opening. Only
              // rebuild when the state that drives the checkmarks has changed.
              let ctrl = ctrl.read(cx);
              let menu_state = ViewMenuState { presentation:   ctrl.presentation,
                                               sort:           ctrl.sort,
                                               sort_direction: ctrl.sort_direction,
                                               grouped:        ctrl.grouped, };
              if this.last_menu_state != Some(menu_state) {
                  cx.set_menus(build_menus(&menu_state, &this.tabs.read(cx).snapshot()));
                  this.last_menu_state = Some(menu_state);
              }
              cx.notify();
          })
          .detach();

        cx.subscribe(&tabs, |this, ctrl, _event: &TabsChanged, cx| {
              // Keep the native Window menu's tab-selection items (labels and
              // enabled state) in sync with the open tab set. Unlike
              // `LibraryChanged`, `TabsChanged` only fires on an actual
              // open/close/activate, so no de-duplication guard is needed here.
              let menu_state = this.last_menu_state.unwrap_or_default();
              cx.set_menus(build_menus(&menu_state, &ctrl.read(cx).snapshot()));
              cx.notify();
          })
          .detach();

        cx.subscribe(&activity, |_this, _ctrl, _event: &ActivityChanged, cx| {
              cx.notify();
          })
          .detach();

        cx.subscribe_in(&activity,
                        window,
                        |_this, _ctrl, event: &DownloadComplete, window, cx| {
                            let msg = format!("{}", event.title);
                            window.push_notification(Notification::success(msg), cx);
                        })
          .detach();

        cx.subscribe_in(&activity,
                        window,
                        |_this, _ctrl, event: &DownloadError, window, cx| {
                            let msg = format!("{}: {}", event.title, event.message);
                            window.push_notification(error_notification(msg), cx);
                        })
          .detach();

        // Settings now lives in its own window (see `settings_window_view`), so
        // `SettingsChanged` no longer affects this window's focus tree — just
        // repaint to pick up any state (e.g. auth) the main window also reads.
        cx.subscribe(&settings, |_this, _ctrl, _event: &SettingsChanged, cx| {
              cx.notify();
          })
          .detach();

        // Propagate a `max_concurrent_downloads` change from the Storage
        // settings page to the controller that actually enforces it, so it
        // takes effect immediately rather than only on next app launch.
        let controller_for_settings = controller.clone();
        let settings_for_concurrency = settings.clone();
        cx.subscribe(&settings,
                     move |_this, _ctrl, _event: &SettingsChanged, cx| {
                         let n = settings_for_concurrency.read(cx)
                                                         .snapshot()
                                                         .max_concurrent_downloads;
                         controller_for_settings.update(cx, |ctrl, cx| {
                                                    ctrl.set_max_concurrent_downloads(n, cx);
                                                });
                     })
          .detach();

        // Propagate a `recently_updated_window_days` change from the Storage
        // settings page to the controller that actually applies it, so it
        // takes effect immediately rather than only on next app launch.
        let controller_for_window = controller.clone();
        let settings_for_window = settings.clone();
        cx.subscribe(&settings,
                     move |_this, _ctrl, _event: &SettingsChanged, cx| {
                         let days = settings_for_window.read(cx)
                                                       .snapshot()
                                                       .recently_updated_window_days;
                         controller_for_window.update(cx, |ctrl, cx| {
                                                  ctrl.set_recently_updated_window_days(days, cx);
                                              });
                     })
          .detach();

        cx.subscribe(&auth_state,
                     |_this, _ctrl, _event: &AuthStateChanged, cx| {
                         cx.notify();
                     })
          .detach();

        // Handle logout: delete the API key and transition to unauthenticated.
        // Tokens are in-memory only and need no explicit deletion.
        let auth_state_for_logout = auth_state.clone();
        cx.subscribe(&settings,
                     move |_this, _ctrl, _event: &LogoutRequested, cx| {
                         let store = KeyringCredentialStore::new(KEYRING_SERVICE, KEYRING_API_KEY);
                         if let Err(e) = store.delete() {
                             tracing::warn!("credential delete (api-key): {e}");
                         }
                         auth_state_for_logout.update(cx, |ctrl, cx| {
                                                  ctrl.set_state(AuthState::Unauthenticated, cx)
                                              });
                     })
          .detach();

        // Handle sign-in: replace both services, mark authenticated, dismiss
        // any auth toast.
        let auth_state_for_signin = auth_state.clone();
        let controller_for_signin = controller.clone();
        cx.subscribe_in(&settings,
                        window,
                        move |_this, _settings, event: &SignInSucceeded, window, cx| {
                            let tokens = event.0.clone();
                            let service =
                                cx.global::<ServiceFactory>().0.as_ref()(Some(tokens.clone()));
                            let collections_service =
                                cx.global::<CollectionsServiceFactory>().0.as_ref()(Some(tokens));
                            controller_for_signin.update(cx, |ctrl, cx| {
                                                     ctrl.replace_service(service,
                                                                          collections_service,
                                                                          cx);
                                                 });
                            auth_state_for_signin.update(cx, |ctrl, cx| {
                                                     ctrl.set_state(AuthState::Authenticated, cx)
                                                 });
                            window.remove_notification::<AuthPendingNotif>(cx);
                        })
          .detach();

        // Handle startup auth beginning: suppress the "Not signed in" banner
        // and show a toast.
        let auth_state_for_begun = auth_state.clone();
        cx.subscribe_in(
                        &settings,
                        window,
                        move |_this, _settings, _event: &StartupAuthBegun, window, cx| {
                            auth_state_for_begun.update(cx, |ctrl, cx| {
                                                    ctrl.set_auth_pending(true, cx)
                                                });
                            window.push_notification(
                    Notification::new()
                        .message(t!("notification.signing_in").to_string())
                        .autohide(false)
                        .id::<AuthPendingNotif>(),
                    cx,
                );
                        },
        )
          .detach();

        // Handle startup auth failure: clear pending state and dismiss the
        // toast.
        let auth_state_for_failed = auth_state.clone();
        cx.subscribe_in(&settings,
                        window,
                        move |_this, _settings, _event: &StartupAuthFailed, window, cx| {
                            auth_state_for_failed.update(cx, |ctrl, cx| {
                                                     ctrl.set_auth_pending(false, cx)
                                                 });
                            window.remove_notification::<AuthPendingNotif>(cx);
                        })
          .detach();

        // Handle cache clear: drop the in-memory catalog/collections and force
        // a live re-fetch, so cleared content disappears immediately
        // instead of lingering.
        let controller_for_cache_cleared = controller.clone();
        cx.subscribe(&settings,
                     move |_this, _settings, _event: &CacheCleared, cx| {
                         let controller = controller_for_cache_cleared.clone();
                         cx.defer(move |cx| {
                               controller.update(cx, |ctrl, cx| ctrl.clear_and_reload(cx));
                           });
                     })
          .detach();

        // Start background auth after all subscriptions are wired.
        if let Some(key) = startup_api_key {
            settings.update(cx, |ctrl, cx| ctrl.startup_auth(key, cx));
        }

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
               search_input,
               collection_name_input,
               file_opener_extension_input,
               publisher_search_input,
               collection_search_input,
               bulk_pattern_input,
               bulk_pattern_field,
               last_menu_state: None }
    }

    /// Opens the Settings window, or brings an already-open one to front.
    ///
    /// Extracted so both the `ShowSettings` action bound on this view's root
    /// element and the app-level fallback in `ui::app::setup` (which fires
    /// when no element in the window's dispatch path currently owns focus)
    /// share one implementation.
    pub(crate) fn show_settings(&mut self, cx: &mut Context<Self>) {
        let already_open = self.settings_window
                               .map(|handle| {
                                   handle.update(cx, |_, window, _| window.activate_window())
                                         .is_ok()
                               })
                               .unwrap_or(false);
        if !already_open {
            self.settings.update(cx, |ctrl, cx| ctrl.open(cx));
            let handle = open_settings_window(self.settings.clone(),
                                              self.controller.clone(),
                                              self.file_opener_extension_input.clone(),
                                              cx);
            self.settings_window = Some(handle);
        }
    }

    /// Opens the "Report a Feature/Bug" window, or brings an already-open
    /// one to front.
    pub(crate) fn show_report_feature_bug(&mut self, cx: &mut Context<Self>) {
        let already_open = self.report_feature_bug_window
                               .map(|handle| {
                                   handle.update(cx, |_, window, _| window.activate_window())
                                         .is_ok()
                               })
                               .unwrap_or(false);
        if !already_open {
            let handle = open_report_feature_bug_window(cx);
            self.report_feature_bug_window = Some(handle);
        }
    }

    /// Opens the About window, or brings an already-open one to front.
    pub(crate) fn show_about(&mut self, cx: &mut Context<Self>) {
        let already_open = self.about_window
                               .map(|handle| {
                                   handle.update(cx, |_, window, _| window.activate_window())
                                         .is_ok()
                               })
                               .unwrap_or(false);
        if !already_open {
            let handle = open_about_window(cx);
            self.about_window = Some(handle);
        }
    }

    /// Opens Settings on the Account tab with the email input focused.
    ///
    /// Used by the "Login to DriveThruRPG" notification banner action — see
    /// `SettingsController::request_email_focus_on_account_tab`.
    pub(crate) fn show_settings_focused_on_email(&mut self, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |ctrl, cx| ctrl.request_email_focus_on_account_tab(cx));
        self.show_settings(cx);
    }

    /// Toggles the Activity panel. See [`Self::show_settings`] for why this
    /// is a method rather than inline closure logic.
    pub(crate) fn show_activity(&mut self, cx: &mut Context<Self>) {
        self.activity.update(cx, |a, cx| a.toggle_panel(cx));
    }

    /// Toggles the Alert History panel. See [`Self::show_settings`] for why
    /// this is a method rather than inline closure logic.
    pub(crate) fn show_alert_history(&mut self, cx: &mut Context<Self>) {
        self.activity.update(cx, |a, cx| a.toggle_alert_panel(cx));
    }

    /// Opens the "Add Collection" dialog. See [`Self::show_settings`] for why
    /// this is a method rather than inline closure logic.
    pub(crate) fn show_add_collection_dialog(&mut self, window: &mut gpui::Window,
                                             cx: &mut Context<Self>) {
        let ctrl = self.controller.clone();
        let input = self.collection_name_input.clone();
        window.open_dialog(cx, move |dialog, _window, _cx| {
                  let ctrl = ctrl.clone();
                  let input = input.clone();
                  dialog
                .close_button(false)
                .overlay_closable(true)
                .w(px(320.))
                // Visible Cancel/Create buttons are rendered via `.footer(...)`
                // below (wrapped in `DialogClose`/`DialogAction`, which dispatch
                // the same `CancelDialog`/`ConfirmDialog` actions Escape/Enter
                // use), so the callbacks registered here run regardless of
                // whether the user clicks a button or uses the keyboard.
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
                    let input = input.clone();
                    move |content, _, _| {
                        content
                            .child(DialogHeader::new().px_4().pt_4().child(
                                DialogTitle::new().child(t!("collections.add_dialog_title")),
                            ))
                            .child(div().px_4().py_2().child(Input::new(&input)))
                    }
                })
                .footer(
                    DialogFooter::new()
                        .px_4()
                        .pb_4()
                        .child(
                            DialogClose::new().child(
                                Button::new("cancel-collection")
                                    .label(t!("collections.add_dialog_cancel")),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("confirm-collection")
                                    .primary()
                                    .label(t!("collections.add_dialog_confirm")),
                            ),
                        ),
                )
              });
    }
}

impl Focusable for LibraryRootView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.root_focus.clone()
    }
}

impl Render for LibraryRootView {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        // `Root` (gpui-kit, wrapping this view — see `gpui_kit::open_window`
        // in `ui/app/mod.rs`) renders its own dialog/sheet/notification
        // layers automatically via its own `Render` impl; this view does
        // not need to compose them.

        // Adjusting the shared rem size scales every `rems(...)`-based size
        // utility (`.text_sm()`, `.text_xs()`, etc.) in this window, like
        // zooming a page — see `LibraryController::set_ui_text_size`.
        window.set_rem_size(cx.global::<LibriTheme>().fonts.ui_text_size);

        let lib_entity = self.controller.clone();
        let settings_entity = self.settings.clone();
        let activity_entity = self.activity.clone();
        let auth_entity = self.auth_state.clone();

        let snap = self.controller.read(cx).snapshot();
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
             grouped,
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
                            snap.grouped,
                            snap.presentation,
                            snap.bulk_selection_mode,
                            snap.bulk_selected_ids,
                            snap.bulk_selection_count,
                            snap.items);

        let settings_snap = self.settings.read(cx).snapshot();
        let activity_snap = self.activity.read(cx).snapshot();
        let notices: Vec<_> = self.auth_state
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
                                                          self.bulk_pattern_input.clone(),
                                                          self.bulk_pattern_field.clone(),
                                                          colors)
                               });

        let publisher_search =
            crate::ui::views::sidebar_view::SidebarSectionSearch { open:
                                                                       snap.publisher_search_open,
                                                                   query:
                                                                       snap.publisher_search_query,
                                                                   input:
                                                                       self.publisher_search_input
                                                                           .clone(), };
        let collection_search =
            crate::ui::views::sidebar_view::SidebarSectionSearch { open:
                                                                       snap.collection_search_open,
                                                                   query:
                                                                       snap.collection_search_query,
                                                                   input:
                                                                       self.collection_search_input
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
                                     self.tabs.clone(),
                                     self.collection_name_input.clone(),
                                     publisher_search,
                                     collection_search);
        let toolbar = render_toolbar(&filter,
                                     snap.filter_count,
                                     matched_count,
                                     total_count,
                                     &snap.search_query,
                                     self.search_input.clone(),
                                     sort,
                                     sort_direction,
                                     grouped,
                                     presentation,
                                     bulk_selection_mode,
                                     lib_entity.clone(),
                                     colors);
        let title_bar = render_title_bar(&settings_snap.auth, settings_entity.clone(), colors, cx);
        let tab_strip = render_tab_strip(self.tabs.clone(), cx);
        let banner = render_notification_banner(notices, auth_entity, cx.entity(), colors);

        let tabs_snap = self.tabs.read(cx).snapshot();
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
        let alert_snap = self.activity.read(cx).alert_snapshot();
        let current_locale =
            crate::i18n::Locale::from_code(&rust_i18n::locale()).unwrap_or(crate::i18n::Locale::En);
        let status_bar = render_status_bar(StatusBarSnapshot { total_count,
                                                               total_mb,
                                                               active_tab_label,
                                                               active_tab_count,
                                                               theme_key: theme.key,
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
                                       .child(self.catalog_view.clone())
                                       .into_any_element(),
            TabTarget::Detail(id) => {
                let controller_ref = self.controller.read(cx);
                match controller_ref.item_by_id(id) {
                    Some(item) => {
                        let item = item.clone();
                        let cache = cx.global::<crate::ui::library::cover::CoverCache>();
                        let detail_key = crate::ui::library::cover::detail_cache_key(&item.id);
                        let cover_image = cache.get(&detail_key).or_else(|| cache.get(&item.id));
                        render_detail_tab_content(&item,
                                                  settings_snap.storage_root_path.clone(),
                                                  lib_entity.clone(),
                                                  &self.tabs,
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
        let controller_for_reload = self.controller.clone();
        let controller_for_refresh_thumbnails = self.controller.clone();
        let controller_for_check_availability = self.controller.clone();
        let controller_for_view_list = self.controller.clone();
        let controller_for_view_thumbs = self.controller.clone();
        let controller_for_view_grid = self.controller.clone();
        let controller_for_sort_title = self.controller.clone();
        let controller_for_sort_publisher = self.controller.clone();
        let controller_for_sort_date_added = self.controller.clone();
        let controller_for_sort_pages = self.controller.clone();
        let controller_for_sort_asc = self.controller.clone();
        let controller_for_sort_desc = self.controller.clone();
        let controller_for_group_toggle = self.controller.clone();
        let search_input_for_focus = self.search_input.clone();
        let tabs_for_select_0 = self.tabs.clone();
        let tabs_for_select_1 = self.tabs.clone();
        let tabs_for_select_2 = self.tabs.clone();
        let tabs_for_select_3 = self.tabs.clone();
        let tabs_for_select_4 = self.tabs.clone();
        let tabs_for_select_5 = self.tabs.clone();
        let tabs_for_select_6 = self.tabs.clone();
        let tabs_for_select_7 = self.tabs.clone();
        let tabs_for_select_8 = self.tabs.clone();
        let tabs_for_select_9 = self.tabs.clone();
        let sidebar_initial = self.sidebar_width;

        div()
            .size_full()
            .bg(surface)
            .text_color(text_primary)
            .relative()
            .track_focus(&self.root_focus)
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
                    .update(cx, |ctrl, cx| ctrl.request_check_batch(cx));
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
                    let grouped = ctrl.snapshot().grouped;
                    ctrl.set_grouped(!grouped, cx);
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
                                .with_state(&self.resize_state)
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
}
