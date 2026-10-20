use super::*;

// ── CatalogListDelegate
// ───────────────────────────────────────────────────────

/// `TableDelegate` for the ungrouped list view. Backed by `LibraryController`.
pub(super) struct CatalogListDelegate {
    pub(super) controller:   Entity<LibraryController>,
    /// Used to open/focus a multi-item entry's expanded detail tab when the
    /// row's inline open action hits
    /// `OpenError::MultipleFilesRequireSelection`.
    pub(super) tabs:         Entity<TabsController>,
    pub(super) storage_root: PathBuf,
    pub(super) cols:         ListColumnSet,
}

impl TableDelegate for CatalogListDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.cols.len()
    }

    fn rows_count(&self, cx: &App) -> usize {
        self.controller.read(cx).visible_items_count()
    }

    fn loading(&self, cx: &App) -> bool {
        self.controller.read(cx).is_loading()
    }

    fn column(&self, col_ix: usize, cx: &App) -> Column {
        // Only the sort is needed. `snapshot()` copies the whole catalog, and
        // the table asks for a column once for every cell it draws.
        let controller = self.controller.read(cx);
        let (sort, sort_direction) = (controller.sort, controller.sort_direction);
        let sorted_column = column_sorted_by(sort).and_then(|c| self.cols.position_of(c));
        let col = self.cols.column(col_ix);
        if sorted_column == Some(col_ix) {
            let sort = match sort_direction {
                SortDirection::Ascending => ColumnSort::Ascending,
                SortDirection::Descending => ColumnSort::Descending,
            };
            col.sort(sort)
        }
        else if self.cols.kind(col_ix).and_then(sort_method_for).is_some() {
            col.sort(ColumnSort::Default)
        }
        else {
            col
        }
    }

    fn perform_sort(&mut self, col_ix: usize, sort: ColumnSort, _window: &mut Window,
                    cx: &mut Context<TableState<Self>>) {
        let Some(method) = self.cols.kind(col_ix).and_then(sort_method_for)
        else {
            return;
        };
        let (method, direction) = match sort {
            ColumnSort::Ascending => (method, SortDirection::Ascending),
            ColumnSort::Descending => (method, SortDirection::Descending),
            ColumnSort::Default => (SortMethod::Title, SortDirection::Ascending),
        };
        self.controller.update(cx, |ctrl, cx| {
                           ctrl.set_sort(method, cx);
                           ctrl.set_sort_direction(direction, cx);
                       });
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, window: &mut Window,
                    cx: &mut Context<TableState<Self>>)
                    -> PopupMenu {
        let items = self.controller
                        .read(cx)
                        .visible_items_slice(row_ix..row_ix + 1);
        let Some(item) = items.into_iter().next()
        else {
            return menu;
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
        append_common_menu_items(menu, &item, &self.controller, window, cx)
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        let items = self.controller
                        .read(cx)
                        .visible_items_slice(row_ix..row_ix + 1);
        let Some(item) = items.into_iter().next()
        else {
            return div().into_any_element();
        };
        let colors = cx.global::<LibriTheme>().colors.clone();
        let column_width = self.column(col_ix, cx).width;
        let ctrl_ref = self.controller.read(cx);
        let is_checking = ctrl_ref.is_checking(&item.id);
        let activity = ctrl_ref.item_activity(&item.id);
        let selection_mode = ctrl_ref.bulk_selection_mode();
        let selected = ctrl_ref.is_bulk_selected(&item.id);
        let Some(column) = self.cols.kind(col_ix)
        else {
            return div().into_any_element();
        };
        let tags = ctrl_ref.tag_library();
        render_list_item_cell(&item,
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
                              selection_mode,
                              selected)
    }

    fn render_th(&mut self, col_ix: usize, _window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        // `gpui-kit`'s default `render_th` renders the column name in a
        // plain non-flex `div`, so the text sits at the top of the
        // header cell instead of vertically centered like every
        // `render_td` cell. Match the cell style here.
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

// ── GroupedCatalogListDelegate
// ────────────────────────────────────────────

/// A row in the flattened, publisher-grouped list: either a group header or
/// a data row. Grouping is flattened into a single row list so the grouped
/// list view can reuse `DataTable`'s virtualization instead of hand-rolling
/// (and fully materializing) its own rows — see `CatalogView::grouped_items`.
pub(super) enum GroupedRow {
    Header {
        /// The publisher or tag the items below are grouped under.
        title: Arc<str>,
        count: usize,
        /// What the group is a group of.
        kind:  GroupBy,
    },
    Item(Box<LibraryItem>),
}
