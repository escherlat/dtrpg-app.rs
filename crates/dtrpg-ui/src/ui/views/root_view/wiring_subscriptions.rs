//! Subscribes the root view to every controller/event stream it reacts to
//! (notifications, native menu sync, cross-controller propagation, auth
//! transitions) and kicks off background startup auth — everything wired
//! once in [`LibraryRootView::new`] that isn't a text input (see
//! `wiring_inputs` for those).

use super::*;

/// The entities [`wire_controller_subscriptions`] subscribes to. Bundled
/// into one struct — rather than five parameters — per this crate's
/// function-argument-count convention.
pub(super) struct RootViewEntities {
    pub(super) controller: Entity<LibraryController>,
    pub(super) tabs:       Entity<TabsController>,
    pub(super) activity:   Entity<ActivityController>,
    pub(super) settings:   Entity<SettingsController>,
    pub(super) auth_state: Entity<AuthStateController>,
}

pub(super) fn wire_controller_subscriptions(window: &mut gpui::Window,
                                            cx: &mut Context<LibraryRootView>,
                                            entities: &RootViewEntities,
                                            startup_api_key: Option<String>) {
    let RootViewEntities { controller,
                           tabs,
                           activity,
                           settings,
                           auth_state, } = entities;

    cx.subscribe_in(controller,
                    window,
                    |_this, _ctrl, event: &CollectionCreateFailed, window, cx| {
                        window.push_notification(error_notification(event.message.clone()), cx);
                    })
      .detach();

    cx.subscribe_in(controller,
                    window,
                    |_this, _ctrl, event: &CollectionMemberAddFailed, window, cx| {
                        window.push_notification(error_notification(event.message.clone()), cx);
                    })
      .detach();

    cx.subscribe_in(controller,
                    window,
                    |_this, _ctrl, event: &CollectionMemberRemoveFailed, window, cx| {
                        window.push_notification(error_notification(event.message.clone()), cx);
                    })
      .detach();

    cx.subscribe_in(controller,
                    window,
                    |_this, _ctrl, event: &CollectionMemberAlreadyPresent, window, cx| {
                        window.push_notification(warning_notification(event.message.clone()), cx);
                    })
      .detach();

    cx.subscribe_in(controller,
                    window,
                    |_this, _ctrl, event: &ThumbnailRefreshStarted, window, cx| {
                        let msg = t!("notification.refresh_thumbnails_started",
                                     count = event.count).to_string();
                        window.push_notification(Notification::info(msg), cx);
                    })
      .detach();

    cx.subscribe_in(controller,
                    window,
                    |_this, _ctrl, event: &ThumbnailRefreshCompleted, window, cx| {
                        if event.failed == 0 {
                            let msg = t!("notification.refresh_thumbnails_completed",
                                         succeeded = event.succeeded).to_string();
                            window.push_notification(Notification::success(msg), cx);
                        }
                        else {
                            let msg = t!("notification.refresh_thumbnails_completed_with_failures",
                                         succeeded = event.succeeded,
                                         failed = event.failed).to_string();
                            window.push_notification(warning_notification(msg), cx);
                        }
                    })
      .detach();

    cx.subscribe_in(controller,
                    window,
                    |_this, _ctrl, _event: &ThumbnailRefreshNoOp, window, cx| {
                        let msg = t!("notification.refresh_thumbnails_noop").to_string();
                        window.push_notification(warning_notification(msg), cx);
                    })
      .detach();

    let controller_for_disk_space = controller.clone();
    cx.subscribe_in(controller,
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

    cx.subscribe(controller, |this, ctrl, _event: &LibraryChanged, cx| {
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
                                           group_by:       ctrl.group_by, };
          if this.last_menu_state != Some(menu_state) {
              cx.set_menus(build_menus(&menu_state, &this.tabs.read(cx).snapshot()));
              this.last_menu_state = Some(menu_state);
          }
          cx.notify();
      })
      .detach();

    cx.subscribe(tabs, |this, ctrl, _event: &TabsChanged, cx| {
          // Keep the native Window menu's tab-selection items (labels and
          // enabled state) in sync with the open tab set. Unlike
          // `LibraryChanged`, `TabsChanged` only fires on an actual
          // open/close/activate, so no de-duplication guard is needed here.
          let menu_state = this.last_menu_state.unwrap_or_default();
          cx.set_menus(build_menus(&menu_state, &ctrl.read(cx).snapshot()));
          cx.notify();
      })
      .detach();

    cx.subscribe(activity, |_this, _ctrl, _event: &ActivityChanged, cx| {
          cx.notify();
      })
      .detach();

    cx.subscribe_in(activity,
                    window,
                    |_this, _ctrl, event: &DownloadComplete, window, cx| {
                        let msg = format!("{}", event.title);
                        window.push_notification(Notification::success(msg), cx);
                    })
      .detach();

    cx.subscribe_in(activity,
                    window,
                    |_this, _ctrl, event: &DownloadError, window, cx| {
                        let msg = format!("{}: {}", event.title, event.message);
                        window.push_notification(error_notification(msg), cx);
                    })
      .detach();

    // Settings now lives in its own window (see `settings_window_view`), so
    // `SettingsChanged` no longer affects this window's focus tree — just
    // repaint to pick up any state (e.g. auth) the main window also reads.
    cx.subscribe(settings, |_this, _ctrl, _event: &SettingsChanged, cx| {
          cx.notify();
      })
      .detach();

    // Propagate a `max_concurrent_downloads` change from the Storage
    // settings page to the controller that actually enforces it, so it
    // takes effect immediately rather than only on next app launch.
    let controller_for_settings = controller.clone();
    let settings_for_concurrency = settings.clone();
    cx.subscribe(settings,
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
    cx.subscribe(settings,
                 move |_this, _ctrl, _event: &SettingsChanged, cx| {
                     let days = settings_for_window.read(cx)
                                                   .snapshot()
                                                   .recently_updated_window_days;
                     controller_for_window.update(cx, |ctrl, cx| {
                                              ctrl.set_recently_updated_window_days(days, cx);
                                          });
                 })
      .detach();

    cx.subscribe(auth_state, |_this, _ctrl, _event: &AuthStateChanged, cx| {
          cx.notify();
      })
      .detach();

    // Handle logout: delete the API key and transition to unauthenticated.
    // Tokens are in-memory only and need no explicit deletion.
    let auth_state_for_logout = auth_state.clone();
    cx.subscribe(settings,
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
    cx.subscribe_in(settings,
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
                    settings,
                    window,
                    move |_this, _settings, _event: &StartupAuthBegun, window, cx| {
                        auth_state_for_begun.update(cx, |ctrl, cx| ctrl.set_auth_pending(true, cx));
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
    cx.subscribe_in(settings,
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
    cx.subscribe(settings,
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
}
