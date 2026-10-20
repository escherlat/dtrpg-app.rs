//! Builds and wires the text inputs and font pickers created once in
//! [`LibraryRootView::new`]: the Settings window's form fields (which live
//! for the app session, not just while the settings window is open — see
//! `wire_settings_inputs`) and the main window's own search/bulk/collection
//! inputs (`build_misc_inputs`).

use super::*;

/// Builds every input and font-picker entity the Settings window's pages
/// read, subscribes each to push its value into the `settings` controller,
/// and attaches it via the matching `SettingsController::set_*` call — so
/// these survive the settings window closing and reopening within the
/// session (see the font-picker section below for why that matters there
/// too).
pub(super) fn wire_settings_inputs(window: &mut gpui::Window, cx: &mut Context<LibraryRootView>,
                                   settings: &Entity<SettingsController>,
                                   controller: &Entity<LibraryController>) {
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
                         settings_for_email.update(cx, |ctrl, cx| ctrl.set_email_draft(value, cx));
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
        use dtrpg_core::data::storage::StorageConfig;
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
        use dtrpg_core::data::storage::StorageConfig;
        StorageConfig::load().recently_updated_window_days()
    };
    let recently_updated_window_input = cx.new(|cx| {
                                              InputState::new(window, cx)
            .default_value(recently_updated_window_days_initial.to_string())
            .min(f64::from(
                dtrpg_core::data::constants::RECENTLY_UPDATED_WINDOW_MIN_DAYS,
            ))
            .max(f64::from(
                dtrpg_core::data::constants::RECENTLY_UPDATED_WINDOW_MAX_DAYS,
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
        use dtrpg_core::data::storage::StorageConfig;
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

    let publisher_rule_input = cx.new(|cx| InputState::new(window, cx).default_value(""));
    let collection_rule_input = cx.new(|cx| InputState::new(window, cx).default_value(""));
    let size_rule_input = cx.new(|cx| InputState::new(window, cx).default_value(""));
    settings.update(cx, |ctrl, _cx| {
                ctrl.set_publisher_rule_input(publisher_rule_input);
                ctrl.set_collection_rule_input(collection_rule_input);
                ctrl.set_size_rule_input(size_rule_input);
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
    let body_font_select =
        cx.new(|cx| {
              SelectState::new(installed_fonts.clone(), body_selected, window, cx).searchable(true)
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
    let value_font_select =
        cx.new(|cx| {
              SelectState::new(installed_fonts.clone(), value_selected, window, cx).searchable(true)
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
    let label_font_select =
        cx.new(|cx| {
              SelectState::new(installed_fonts.clone(), label_selected, window, cx).searchable(true)
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
        cx.new(|cx| SelectState::new(installed_fonts, mono_selected, window, cx).searchable(true));
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
}

/// Inputs owned by the main window itself (not the Settings window) that
/// need to survive into [`LibraryRootView`]'s fields: search, bulk-selection,
/// and the sidebar/dialog inline inputs.
pub(super) struct MiscInputs {
    pub(super) search_input:                Entity<InputState>,
    pub(super) bulk_pattern_input:          Entity<InputState>,
    pub(super) bulk_pattern_field:          Rc<Cell<SelectionField>>,
    pub(super) collection_name_input:       Entity<InputState>,
    pub(super) file_opener_extension_input: Entity<InputState>,
    pub(super) publisher_search_input:      Entity<InputState>,
    pub(super) collection_search_input:     Entity<InputState>,
}

pub(super) fn build_misc_inputs(window: &mut gpui::Window, cx: &mut Context<LibraryRootView>,
                                controller: &Entity<LibraryController>,
                                settings: &Entity<SettingsController>)
                                -> MiscInputs {
    let search_input =
        cx.new(|cx| InputState::new(window, cx).placeholder(t!("search.placeholder").to_string()));

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

    let collection_name_input =
        cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("collections.name_placeholder").to_string())
        });
    let file_opener_extension_input = cx.new(|cx| {
                                            InputState::new(window, cx)
            .placeholder(t!("settings.file_opener_extension_placeholder").to_string())
                                        });

    let publisher_search_input =
        cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("search.publishers_placeholder").to_string())
        });
    let controller_for_publisher_search = controller.clone();
    cx.subscribe(&publisher_search_input,
                 move |_this, input_entity, event: &InputEvent, cx| {
                     if matches!(event, InputEvent::Change) {
                         let value = input_entity.read(cx).value().to_string();
                         controller_for_publisher_search.update(cx, |ctrl, cx| {
                                                            ctrl.set_publisher_search_query(value,
                                                                                            cx);
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

    MiscInputs { search_input,
                 bulk_pattern_input,
                 bulk_pattern_field,
                 collection_name_input,
                 file_opener_extension_input,
                 publisher_search_input,
                 collection_search_input }
}
