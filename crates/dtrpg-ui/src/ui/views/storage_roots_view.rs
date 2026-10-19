//! Storage-root settings controls.

use super::*;

/// Renders the configured roots and lets the user add or remove named roots.
pub(super) fn render_storage_roots(roots: Vec<StorageRoot>, entity: Entity<SettingsController>,
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
                        .icon(IconName::Plus)
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
