use super::*;

// ── GroupedCatalogListDelegate
// ────────────────────────────────────────────

/// `TableDelegate` for the grouped list view. Backed by a flattened row list
/// built from `group_by_publisher`, rebuilt only when the controller's
/// visible items change (see `CatalogView::grouped_items`).
pub(super) struct GroupedCatalogListDelegate {
    pub(super) controller:   Entity<LibraryController>,
    /// Used to open/focus a multi-item entry's expanded detail tab when the
    /// row's inline open action hits
    /// `OpenError::MultipleFilesRequireSelection`.
    pub(super) tabs:         Entity<TabsController>,
    pub(super) storage_root: PathBuf,
    pub(super) cols:         ListColumnSet,
    pub(super) rows:         Vec<GroupedRow>,
}

impl GroupedCatalogListDelegate {
    pub(super) fn row_at(&self, row_ix: usize) -> Option<&GroupedRow> {
        self.rows.get(row_ix)
    }
}

impl TableDelegate for GroupedCatalogListDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.cols.len()
    }

    fn loading(&self, cx: &App) -> bool {
        self.controller.read(cx).is_loading()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        self.cols.column(col_ix)
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, window: &mut Window,
                    cx: &mut Context<TableState<Self>>)
                    -> PopupMenu {
        let item = match self.row_at(row_ix) {
            Some(GroupedRow::Header { kind, .. }) if *kind != GroupBy::Publisher => return menu,
            Some(GroupedRow::Header { title, .. }) => {
                let publisher = title.to_string();
                let entity = self.controller.clone();
                return menu.item(
                    PopupMenuItem::new(t!("catalog.publisher_download_all")).on_click(
                        move |_, _, cx| {
                            entity.update(cx, |ctrl, cx| {
                                ctrl.request_download_all_for_publisher(&publisher, cx);
                            });
                        },
                    ),
                );
            }
            Some(GroupedRow::Item(item)) => item,
            None => return menu,
        };
        let id = Arc::clone(&item.id);
        let status = item.status;
        let title = item.title.to_string();
        let entity = self.controller.clone();
        let menu = match status {
            ItemStatus::Downloaded => {
                let item_path =
                    dtrpg_core::data::storage::publisher_dir(&self.storage_root, &item.publisher);
                let entity_remove = entity.clone();
                let remove_id = Arc::clone(&id);
                let item_path_reveal = item_path.clone();
                let item_for_open = item.clone();
                let tabs_for_open = self.tabs.clone();
                let controller_for_open = entity.clone();
                menu.item(PopupMenuItem::new(t!("catalog.action_open")).on_click(
                    move |_, _, cx| {
                        open_item_or_focus_detail_tab(
                            &item_path,
                            &item_for_open,
                            &tabs_for_open,
                            &controller_for_open,
                            cx,
                        );
                    },
                ))
                .item(
                    PopupMenuItem::new(platform_reveal_label()).on_click(move |_, _, _| {
                        if !item_path_reveal.exists() {
                            tracing::warn!(
                                path = %item_path_reveal.display(),
                                "reveal: file not found"
                            );
                            return;
                        }
                        if let Err(e) = reveal_in_file_manager(&item_path_reveal) {
                            tracing::warn!("reveal_in_file_manager failed: {e}");
                        }
                    }),
                )
                .item({
                    let title_for_remove = title.clone();
                    PopupMenuItem::new(t!("catalog.action_remove_download")).on_click(
                        move |_, window, cx| {
                            let entity_remove = entity_remove.clone();
                            let remove_id = Arc::clone(&remove_id);
                            let title_for_remove = title_for_remove.clone();
                            window.open_alert_dialog(cx, move |alert, _, _| {
                                let entity_remove = entity_remove.clone();
                                let remove_id = Arc::clone(&remove_id);
                                alert
                                    .confirm()
                                    .title(
                                        t!(
                                            "catalog.remove_download_confirm_title",
                                            title = title_for_remove.clone()
                                        )
                                        .to_string(),
                                    )
                                    .description(
                                        t!("catalog.remove_download_confirm_description")
                                            .to_string(),
                                    )
                                    .on_ok(move |_, _window, cx| {
                                        entity_remove.update(cx, |ctrl, cx| {
                                            ctrl.remove_download(&remove_id, cx)
                                        });
                                        true
                                    })
                            });
                        },
                    )
                })
            }
            ItemStatus::Cloud => menu.item(
                PopupMenuItem::new(t!("catalog.action_download")).on_click(move |_, _, cx| {
                    entity.update(cx, |ctrl, cx| ctrl.request_download(&id, title.clone(), cx));
                }),
            ),
        };
        append_common_menu_items(menu, item, &self.controller, window, cx)
    }

    fn render_tr(&mut self, row_ix: usize, _window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> gpui::Stateful<gpui::Div> {
        let id = ("grouped-row", row_ix);
        match self.row_at(row_ix) {
            Some(GroupedRow::Header { .. }) => {
                let colors = cx.global::<LibriTheme>().colors.clone();
                div().id(id).bg(colors.hover)
            }
            _ => div().id(id),
        }
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        let colors = cx.global::<LibriTheme>().colors.clone();
        match self.row_at(row_ix) {
            Some(GroupedRow::Header { title, count, .. }) if col_ix == 0 => {
                div().h_full()
                     .flex()
                     .items_center()
                     .gap(px(8.0))
                     .child(div().text_sm()
                                 .font_weight(FontWeight::SEMIBOLD)
                                 .text_color(colors.text_primary)
                                 .child(title.to_string()))
                     .child(div().text_xs()
                                 .text_color(colors.text_tertiary)
                                 .child(count.to_string()))
                     .into_any_element()
            }
            Some(GroupedRow::Header { .. }) => div().h_full().into_any_element(),
            Some(GroupedRow::Item(item)) => {
                let Some(column) = self.cols.kind(col_ix)
                else {
                    return div().into_any_element();
                };
                let column_width = self.column(col_ix, cx).width;
                let is_checking = self.controller.read(cx).is_checking(&item.id);
                let activity = self.controller.read(cx).item_activity(&item.id);
                let tags = self.controller.read(cx).tag_library();
                render_list_item_cell(item,
                                      column,
                                      tags.tag_names_of(&item.id),
                                      &colors,
                                      &self.storage_root,
                                      &self.controller,
                                      &self.tabs,
                                      column_width,
                                      window,
                                      is_checking,
                                      activity,
                                      false,
                                      false)
            }
            None => div().into_any_element(),
        }
    }

    fn render_th(&mut self, col_ix: usize, _window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        let name = self.column(col_ix, cx).name;
        let label_font_family = cx.global::<LibriTheme>().fonts.label_font.clone();
        let controller = self.controller.clone();
        let chosen = controller.read(cx).list_columns.clone();
        div().id(("list-heading", col_ix))
             .h_full()
             .flex()
             .items_center()
             .text_sm()
             .font_family(label_font_family.to_string())
             .font_weight(FontWeight::MEDIUM)
             .child(name)
             .context_menu(move |menu, _, _| column_chooser_menu(menu, &chosen, &controller))
    }
}
