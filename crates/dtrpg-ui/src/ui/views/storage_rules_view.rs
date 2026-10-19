//! Placement-rule settings controls.

use super::*;

/// Renders saved placement rules in evaluation order. Rule creation is kept
/// in the core configuration API; this view makes persisted rules visible and
/// safely supports removing or reordering them.
#[allow(clippy::too_many_arguments)] // Render state is split into independent settings controls.
pub(super) fn render_storage_rules(rules: Vec<PlacementRule>,
                                   entity: Entity<SettingsController>, colors: &ColorTokens,
                                   roots: Vec<StorageRoot>,
                                   publisher_input: Option<Entity<InputState>>,
                                   collection_input: Option<Entity<InputState>>,
                                   size_input: Option<Entity<InputState>>, target_root_id: u32,
                                   publisher_names: Vec<String>,
                                   rule_editor_type: RuleEditorType)
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
