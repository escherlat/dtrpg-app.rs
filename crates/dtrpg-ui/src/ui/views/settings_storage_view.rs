//! Downloads settings section: download root path display, folder picker, and
//! reveal action.

use std::path::PathBuf;

use dtrpg_core::data::storage::placement::{PlacementCondition, PlacementRule};
use dtrpg_core::data::storage::roots::StorageRoot;
use dtrpg_core::data::storage::validate_writable;
use dtrpg_core::platform::{Platform, PlatformIntegration, PlatformKind};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Element, Entity, IntoElement, ParentElement, SharedString, Styled, div, px,
};
use gpui_kit::component::IconName;
use gpui_kit::component::Selectable;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState, NumberInput};
use gpui_kit::component::switch::Switch;
use rust_i18n::t;

use crate::controllers::settings::{RuleEditorType, SettingsController};
use crate::data::theme::ColorTokens;
use crate::ui::widgets::selectable_text;

/// Lower bound for the concurrency field: 0 would mean thumbnails and
/// downloads never start.
pub(crate) const MIN_CONCURRENT_DOWNLOADS: usize = 1;
/// Upper bound for the concurrency field. There is no bandwidth throttling
/// (see the change's design non-goals), so this caps how aggressively a
/// misconfigured value could saturate the connection.
pub(crate) const MAX_CONCURRENT_DOWNLOADS: usize = 5;

/// Renders the Storage settings section.
///
/// Displays the current `storage_root_path`, inline icon buttons for "Change…"
/// and "Show in Finder/Explorer/Files", an optional warning row when
/// `storage_path_exists` is `false`, and an editable field for the shared
/// thumbnail/download concurrency limit.
#[allow(clippy::too_many_arguments)]
pub fn render_storage_section(storage_root_path: PathBuf, storage_path_exists: bool,
                              entity: Entity<SettingsController>, colors: &ColorTokens,
                              storage_path_input: Option<Entity<InputState>>,
                              max_concurrent_downloads: usize,
                              max_concurrent_downloads_input: Option<Entity<InputState>>,
                              create_collections: bool, storage_roots: Vec<StorageRoot>,
                              storage_rules: Vec<PlacementRule>,
                              publisher_rule_input: Option<Entity<InputState>>,
                              collection_rule_input: Option<Entity<InputState>>,
                              size_rule_input: Option<Entity<InputState>>,
                              rule_target_root_id: u32, publisher_names: Vec<String>,
                              rule_editor_type: RuleEditorType)
                              -> impl IntoElement + 'static + use<> {
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;
    let border = colors.border;
    let surface_alt = colors.surface_alt;
    let warning_bg = colors.warning_bg;
    let warning_text = colors.warning_text;

    let path_display = storage_root_path.to_string_lossy().into_owned();
    let entity_change = entity.clone();
    let entity_reveal = entity.clone();

    let reveal_label = platform_reveal_label().into_owned();

    let mut section = div()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .p(px(24.0))
        // ── Section header ────────────────────────────────────────────────
        .child(
            div()
                .text_sm()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(text_primary)
                .child(t!("settings.storage_title")),
        )
        // ── Path row with inline action buttons ───────────────────────────
        .child(
            div().flex().flex_col().gap(px(6.0)).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    // Path input field
                    .child({
                        let path_el: AnyElement = if let Some(input_state) = storage_path_input {
                            Input::new(&input_state)
                                .appearance(true)
                                .into_element()
                                .into_any()
                        } else {
                            div()
                                .flex_1()
                                .min_w_0()
                                .h(px(34.0))
                                .px(px(12.0))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(border)
                                .bg(surface_alt)
                                .flex()
                                .items_center()
                                .child(
                                    selectable_text("settings-storage-path", path_display)
                                        .text_sm()
                                        .text_color(text_secondary)
                                        .truncate(),
                                )
                                .into_any()
                        };
                        path_el
                    })
                    // "Change…" icon button
                    .child(
                        Button::new("change-storage")
                            .ghost()
                            .outline()
                            .icon(IconName::Folder)
                            .tooltip(t!("settings.storage_change_tooltip").to_string())
                            .on_click(move |_event, _window, cx| {
                                let entity = entity_change.clone();
                                cx.spawn(async move |cx| {
                                    // Present the picker asynchronously rather than blocking
                                    // here. `rfd::FileDialog::pick_folder` runs AppKit's
                                    // nested modal loop on the main thread while this handler
                                    // still holds gpui's `App` borrow (the handler runs
                                    // inside `dispatch_event`). That modal loop drains the
                                    // main dispatch queue, so a foreground task resuming
                                    // mid-picker — a catalog page continuation, or the
                                    // periodic catalog-refresh timer — re-enters
                                    // `AppCell::borrow_mut` on the still-borrowed `App` and
                                    // aborts with "already borrowed". `AsyncFileDialog`
                                    // presents the panel as a sheet, so no gpui thread is
                                    // ever blocked. See issue #257.
                                    let Some(handle) =
                                        rfd::AsyncFileDialog::new().pick_folder().await
                                    else {
                                        return;
                                    };
                                    let path = handle.path().to_path_buf();
                                    match validate_writable(&path) {
                                        Ok(()) => {
                                            entity.update(cx, |ctrl, cx| {
                                                if let Err(e) = ctrl.apply_storage_path(path, cx) {
                                                    tracing::warn!("storage path rejected: {e}");
                                                }
                                            });
                                        }
                                        Err(e) => tracing::warn!("storage path not writable: {e}"),
                                    }
                                })
                                .detach();
                            }),
                    )
                    // "Show in Finder/Explorer/Files" icon button
                    .child(
                        Button::new("reveal-storage")
                            .ghost()
                            .outline()
                            .icon(IconName::FolderOpen)
                            .tooltip(reveal_label.clone())
                            .on_click(move |_event, _window, cx| {
                                entity_reveal.read(cx).reveal_storage_location();
                            }),
                    ),
            ),
        );

    // ── Missing-path warning ──────────────────────────────────────────────
    if !storage_path_exists {
        section = section.child(div().rounded(px(6.0))
                                     .px(px(10.0))
                                     .py(px(6.0))
                                     .bg(warning_bg)
                                     .flex()
                                     .items_center()
                                     .gap(px(6.0))
                                     .child(div().text_sm().text_color(warning_text).child("⚠"))
                                     .child(div().text_xs()
                                                 .text_color(warning_text)
                                                 .child(t!("settings.storage_missing"))));
    }

    section
           // ── Divider ───────────────────────────────────────────────────────
           .child(div().h(px(1.0)).bg(border))
           // ── Note ─────────────────────────────────────────────────────────
           .child(div().text_xs()
                       .text_color(warning_text)
                       .child(format!("\u{26A0} {}", t!("settings.storage_note"))))
           // ── Divider ───────────────────────────────────────────────────────
           .child(div().h(px(1.0)).bg(border))
           // ── Concurrency field ─────────────────────────────────────────────
           .child(render_concurrency_row(max_concurrent_downloads,
                                         max_concurrent_downloads_input,
                                         colors))
           // ── Divider ───────────────────────────────────────────────────────
           .child(div().h(px(1.0)).bg(border))
           // ── Create collections toggle ───────────────────────────────────
           .child(render_create_collections_toggle(create_collections, entity.clone(), colors))
           // ── Additional roots ───────────────────────────────────────────
           .child(div().h(px(1.0)).bg(border))
           .child(render_storage_roots(storage_roots.clone(), entity.clone(), colors))
           .child(div().h(px(1.0)).bg(border))
           .child(render_storage_rules(storage_rules,
                                       entity,
                                       colors,
                                       storage_roots,
                                       publisher_rule_input,
                                       collection_rule_input,
                                       size_rule_input,
                                       rule_target_root_id,
                                       publisher_names,
                                       rule_editor_type))
}

/// Renders saved placement rules in evaluation order. Rule creation is kept
/// in the core configuration API; this view makes persisted rules visible and
/// safely supports removing or reordering them.
#[allow(clippy::too_many_arguments)] // Render state is split into independent settings controls.
fn render_storage_rules(rules: Vec<PlacementRule>, entity: Entity<SettingsController>,
                        colors: &ColorTokens, roots: Vec<StorageRoot>,
                        publisher_input: Option<Entity<InputState>>,
                        collection_input: Option<Entity<InputState>>,
                        size_input: Option<Entity<InputState>>, target_root_id: u32,
                        publisher_names: Vec<String>, rule_editor_type: RuleEditorType)
                        -> impl IntoElement + 'static + use<> {
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;
    let border = colors.border;
    let selector = [(RuleEditorType::Publisher, "Publisher"),
                    (RuleEditorType::Collection, "Collection"),
                    (RuleEditorType::Size, "Size")];
    let type_selector = div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .children(selector.into_iter().map(|(kind, label)| {
            let entity = entity.clone();
            Button::new(SharedString::from(format!("rule-type-{label}")))
                .ghost()
                .selected(kind == rule_editor_type)
                .child(label)
                .on_click(move |_, _, cx| {
                    entity.update(cx, |ctrl, cx| ctrl.set_rule_editor_type(kind, cx));
                })
        }));
    let active_input = match rule_editor_type {
        RuleEditorType::Publisher => publisher_input.clone(),
        RuleEditorType::Collection => collection_input.clone(),
        RuleEditorType::Size => size_input.clone(),
    };
    let input = active_input.clone();
    let add_entity = entity.clone();
    let add_button = Button::new("add-placement-rule").ghost()
                                                      .outline()
                                                      .child(t!("settings.storage_add_rule"))
                                                      .on_click(move |_, _, cx| {
                                                          let Some(input) = input.clone()
                                                          else {
                                                              return;
                                                          };
                                                          let value =
                                                              input.read(cx).value().to_string();
                                                          add_entity.update(cx, |ctrl, cx| {
                                                                        match rule_editor_type {
                RuleEditorType::Publisher => ctrl.add_publisher_rule(value, cx),
                RuleEditorType::Collection => ctrl.add_collection_rule(value, cx),
                RuleEditorType::Size => ctrl.add_size_rule(value, cx),
            }
                                                                    });
                                                      });
    let suggestion_list = if rule_editor_type == RuleEditorType::Publisher {
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(publisher_names.into_iter().take(8).map(|name| {
                let input = publisher_input.clone();
                Button::new(SharedString::from(format!("publisher-suggestion-{name}")))
                    .ghost()
                    .child(name.clone())
                    .on_click(move |_, window, cx| {
                        if let Some(input) = input.clone() {
                            input.update(cx, |state, cx| {
                                state.set_value(name.clone(), window, cx);
                            });
                        }
                    })
            }))
            .into_any_element()
    }
    else {
        div().into_any_element()
    };
    let add_row = div().flex()
                       .items_start()
                       .gap(px(6.0))
                       .child(type_selector)
                       .child(div().flex_1()
                                   .flex_col()
                                   .gap(px(3.0))
                                   .child(active_input.map_or_else(|| {
                                                           div().text_xs()
                                                                .text_color(text_secondary)
                                                                .child("Value")
                                                                .into_any_element()
                                                       },
                                                       |input| {
                                                           Input::new(&input).appearance(true)
                                                                             .into_any_element()
                                                       }))
                                   .child(suggestion_list))
                       .child(add_button);

    let target_row =
        div().flex()
             .items_center()
             .gap(px(4.0))
             .child(div().text_xs().text_color(text_secondary).child("Target:"))
             .children(roots.iter().map(|root| {
                 let id = root.id;
                 let entity = entity.clone();
                 Button::new(SharedString::from(format!("rule-target-{id}")))
                .ghost()
                .selected(id == target_root_id)
                .child(root.name.clone())
                .on_click(move |_, _, cx| {
                    entity.update(cx, |ctrl, cx| ctrl.set_rule_target_root(id, cx));
                })
             }));

    div().flex()
         .flex_col()
         .gap(px(8.0))
         .child(div().text_sm()
                     .font_weight(gpui::FontWeight::SEMIBOLD)
                     .text_color(text_primary)
                     .child(t!("settings.storage_rules_title")))
         .child(target_row)
         .child(add_row)
         .children(rules.into_iter().enumerate().map(|(index, rule)| {
             let entity_up = entity.clone();
             let entity_down = entity.clone();
             let entity_remove = entity.clone();
             div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .p(px(6.0))
                .border_1()
                .border_color(border)
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(text_secondary)
                        .child(format_rule(&rule)),
                )
                .child(
                    Button::new(SharedString::from(format!("rule-up-{index}")))
                        .ghost()
                        .icon(IconName::ChevronUp)
                        .on_click(move |_, _, cx| {
                            entity_up.update(cx, |ctrl, cx| {
                                ctrl.move_storage_rule(index, false, cx);
                            });
                        }),
                )
                .child(
                    Button::new(SharedString::from(format!("rule-down-{index}")))
                        .ghost()
                        .icon(IconName::ChevronDown)
                        .on_click(move |_, _, cx| {
                            entity_down.update(cx, |ctrl, cx| {
                                ctrl.move_storage_rule(index, true, cx);
                            });
                        }),
                )
                .child(
                    Button::new(SharedString::from(format!("rule-remove-{index}")))
                        .ghost()
                        .icon(IconName::Close)
                        .on_click(move |_, _, cx| {
                            entity_remove.update(cx, |ctrl, cx| {
                                ctrl.remove_storage_rule(index, cx);
                            });
                        }),
                )
         }))
}

fn format_rule(rule: &PlacementRule) -> String {
    match &rule.condition {
        PlacementCondition::Publisher(name) => {
            format!("Publisher: {name} → root {}", rule.target_root_id)
        }
        PlacementCondition::Collection(id) => {
            format!("Collection: {id} → root {}", rule.target_root_id)
        }
        PlacementCondition::SizeAtLeastMb(size) => {
            format!("Size ≥ {size:.1} MB → root {}", rule.target_root_id)
        }
    }
}

/// Renders the configured roots and lets the user add or remove named roots.
fn render_storage_roots(roots: Vec<StorageRoot>, entity: Entity<SettingsController>,
                        colors: &ColorTokens)
                        -> impl IntoElement + 'static + use<> {
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;
    let border = colors.border;
    let surface_alt = colors.surface_alt;
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(text_primary)
                        .child(t!("settings.storage_roots_title")),
                )
                .child(
                    Button::new("add-storage-root")
                        .ghost()
                        .outline()
                        .icon(IconName::Folder)
                        .tooltip(t!("settings.storage_add_root_tooltip").to_string())
                        .on_click({
                            let entity = entity.clone();
                            move |_, _, cx| {
                                let entity = entity.clone();
                                cx.spawn(async move |cx| {
                                    let Some(handle) =
                                        rfd::AsyncFileDialog::new().pick_folder().await
                                    else {
                                        return;
                                    };
                                    let path = handle.path().to_path_buf();
                                    let name = path
                                        .file_name()
                                        .and_then(|name| name.to_str())
                                        .filter(|name| !name.is_empty())
                                        .unwrap_or("Storage")
                                        .to_owned();
                                    if let Err(error) = entity.update(cx, |ctrl, cx| {
                                        ctrl.add_storage_root(name, path, cx)
                                    }) {
                                        tracing::warn!(%error, "storage root rejected");
                                    }
                                })
                                .detach();
                            }
                        }),
                ),
        )
        .children(roots.into_iter().map(|root| {
            let root_id = root.id;
            let can_remove = root_id != dtrpg_core::data::storage::DEFAULT_ROOT_ID;
            let entity = entity.clone();
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .p(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(border)
                .bg(surface_alt)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(div().text_sm().text_color(text_primary).child(root.name))
                        .child(
                            div()
                                .text_xs()
                                .text_color(text_secondary)
                                .child(root.path.to_string_lossy().into_owned())
                                .truncate(),
                        ),
                )
                .when(can_remove, |row| {
                    row.child(
                        Button::new(SharedString::from(format!("remove-storage-root-{root_id}")))
                            .ghost()
                            .icon(IconName::Close)
                            .tooltip(t!("settings.storage_remove_root_tooltip").to_string())
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |ctrl, cx| {
                                    ctrl.remove_storage_root(root_id, cx);
                                });
                            }),
                    )
                })
        }))
}

/// Renders the "Create collections" toggle row: a label/note pair and a
/// switch, matching the layout style used for the concurrency stepper above.
fn render_create_collections_toggle(create_collections: bool,
                                    entity: Entity<SettingsController>, colors: &ColorTokens)
                                    -> impl IntoElement + 'static + use<> {
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_sm()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(text_primary)
                        .child(t!("settings.create_collections_title")),
                )
                .child(
                    Switch::new("create-collections")
                        .checked(create_collections)
                        .tooltip(t!("settings.create_collections_tooltip").to_string())
                        .on_click(move |checked, _window, cx| {
                            let checked = *checked;
                            entity.update(cx, |ctrl, cx| {
                                ctrl.set_create_collections(checked, cx);
                            });
                        }),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(text_secondary)
                .child(create_collections_note()),
        )
}

/// Returns the "Create collections" note text, appending the Windows
/// Developer Mode caveat only when running on Windows — it does not apply
/// to macOS or Linux, where symlink creation needs no elevated privilege.
fn create_collections_note() -> String {
    if Platform.kind() == PlatformKind::Windows {
        format!("{} {}",
                t!("settings.create_collections_note"),
                t!("settings.create_collections_note_windows"))
    }
    else {
        t!("settings.create_collections_note").to_string()
    }
}

/// Renders the "Max concurrent downloads" row: a label/note pair and an
/// editable [`NumberInput`] (bounded `MIN_CONCURRENT_DOWNLOADS`-
/// `MAX_CONCURRENT_DOWNLOADS` via `InputState::min`/`max`, set when the field
/// was created) with built-in +/- stepper buttons.
///
/// Falls back to a plain, non-editable value when `input` is `None` (before
/// the root view attaches the shared input state), matching the pattern used
/// for `recently_updated_window_input` in `settings_advanced_view`.
fn render_concurrency_row(max_concurrent_downloads: usize, input: Option<Entity<InputState>>,
                          colors: &ColorTokens)
                          -> impl IntoElement + 'static + use<> {
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;

    let field: AnyElement = if let Some(input_state) = input {
        NumberInput::new(&input_state).into_any_element()
    }
    else {
        div().text_sm()
             .text_color(text_primary)
             .child(max_concurrent_downloads.to_string())
             .into_any_element()
    };

    div().flex()
         .flex_col()
         .gap(px(6.0))
         .child(div().text_sm()
                     .font_weight(gpui::FontWeight::SEMIBOLD)
                     .text_color(text_primary)
                     .child(t!("settings.max_concurrent_downloads_title")))
         .child(div().w(px(140.0)).child(field))
         .child(div().text_xs()
                     .text_color(text_secondary)
                     .child(t!("settings.max_concurrent_downloads_note")))
}

// ── Helpers
// ───────────────────────────────────────────────────────────────────

fn platform_reveal_label() -> std::borrow::Cow<'static, str> {
    match Platform.kind() {
        PlatformKind::MacOs => t!("detail.show_in_finder"),
        PlatformKind::Windows => t!("detail.show_in_explorer"),
        PlatformKind::Linux => t!("detail.show_in_files"),
    }
}
