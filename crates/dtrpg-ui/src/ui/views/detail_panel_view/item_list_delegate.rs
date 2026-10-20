use super::*;

/// Returns the column definitions used for the multi-item entry's item list.
///
/// Name's `width(240.)` is only the fallback default before the table's
/// rendered width is known; once known, [`ItemListDelegate::column`]
/// recomputes it each render to fill whatever space Type/Size/Status don't
/// use, so Name absorbs panel resizes while the other three stay narrow and
/// right-aligned as a group. All four are still user-resizable — see
/// `detail-item-list-column-resize`.
pub(crate) fn item_list_columns() -> Vec<Column> {
    vec![Column::new("name", t!("detail.item_list_column_name")).width(240.)
                                                                .min_width(120.)
                                                                .resizable(true),
         Column::new("type", t!("detail.item_list_column_type")).width(90.)
                                                                .min_width(60.)
                                                                .resizable(true),
         Column::new("size", t!("detail.item_list_column_size")).width(90.)
                                                                .min_width(60.)
                                                                .resizable(true),
         Column::new("info", t!("detail.item_list_column_info")).width(100.)
                                                                .min_width(60.)
                                                                .resizable(true),
         Column::new("status", t!("detail.item_list_column_status")).width(90.)
                                                                    .min_width(60.)
                                                                    .resizable(true),]
}

/// `TableDelegate` for a multi-item entry's item list. Backed by
/// `LibraryController`, reading `item.files` live via `item_by_id` rather
/// than cloning the file list, mirroring `CatalogListDelegate`'s
/// read-through-controller pattern.
pub(crate) struct ItemListDelegate {
    pub(crate) controller:  Entity<LibraryController>,
    pub(crate) entry_id:    Arc<str>,
    pub(crate) columns:     Vec<Column>,
    /// User-adjusted column widths. `None` means use the static default from
    /// `item_list_columns()`.
    pub(crate) user_widths: Vec<Option<Pixels>>,
    /// The entry's on-disk download directory, used to resolve each file's
    /// on-disk size suffix in the Size column (see `render_metadata_table`'s
    /// equivalent computation).
    pub(crate) entry_dir:   PathBuf,
    /// Current rendered width of the item list table, captured by
    /// `render_item_tier`'s `on_prepaint` hook. Drives the Name column's
    /// fill-remaining-width sizing in [`Self::column`] — `gpui-kit`'s
    /// `Column` has no built-in flex/stretch behavior, so Name's width is
    /// recomputed each time this changes rather than left fixed.
    pub(crate) table_width: Option<Pixels>,
}

impl TableDelegate for ItemListDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, cx: &App) -> usize {
        self.controller
            .read(cx)
            .item_by_id(&self.entry_id)
            .map_or(0, |item| item.files.len())
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        let mut col = self.columns[col_ix].clone();
        if let Some(Some(w)) = self.user_widths.get(col_ix) {
            col = col.width(w.as_f32());
        }
        else if col_ix == 0
                  && let Some(table_width) = self.table_width
        {
            // Name has no user override yet: fill whatever width
            // Type/Size/Status don't use, so those three stay
            // narrow and pushed to the right instead of leaving
            // blank trailing space when the panel is wider than the
            // four columns' fixed defaults.
            let others: f32 = self.columns[1..].iter().map(|c| c.width.as_f32()).sum();
            let name_width = (table_width.as_f32() - others).max(col.min_width.as_f32());
            col = col.width(name_width);
        }
        col
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        let Some(item) = self.controller.read(cx).item_by_id(&self.entry_id).cloned()
        else {
            return div().into_any_element();
        };
        let Some(file) = item.files.get(row_ix)
        else {
            return div().into_any_element();
        };
        let colors = cx.global::<LibriTheme>().colors.clone();
        let value_font_family = cx.global::<LibriTheme>().fonts.value_font.clone();

        match col_ix {
            // Name column.
            0 => div().h_full()
                      .flex()
                      .items_center()
                      .gap(px(4.0))
                      .text_sm()
                      .font_family(value_font_family.to_string())
                      .text_color(colors.text_primary)
                      .child(div().flex_1()
                                  .min_w_0()
                                  .truncate()
                                  .child(file.name.to_string()))
                      .into_any_element(),
            1 => div().h_full()
                      .flex()
                      .items_center()
                      .text_sm()
                      .font_family(value_font_family.to_string())
                      .text_color(colors.text_secondary)
                      .truncate()
                      .child(file.format.to_string())
                      .into_any_element(),
            2 => {
                let catalog_str = format!("{:.1} {}", file.size_mb, t!("size.mb"));
                let on_disk =
                    crate::util::file_size::on_disk_file_size(&self.entry_dir, file.name.as_ref());
                div().h_full()
                     .flex()
                     .items_center()
                     .text_sm()
                     .font_family(value_font_family.to_string())
                     .text_color(colors.text_secondary)
                     .child(crate::util::file_size::prefer_on_disk_size(catalog_str, on_disk))
                     .into_any_element()
            }
            3 => {
                let entry_id = Arc::clone(&self.entry_id);
                let entry_dir = self.entry_dir.clone();
                let info = self.controller.update(cx, |ctrl, _cx| {
                                              ctrl.file_info(&entry_id, row_ix, &entry_dir)
                                          });
                let text = info.display().unwrap_or_else(|| "\u{2014}".to_string());
                div().h_full()
                     .flex()
                     .items_center()
                     .text_sm()
                     .font_family(value_font_family.to_string())
                     .text_color(colors.text_secondary)
                     .truncate()
                     .child(text)
                     .into_any_element()
            }
            4 => {
                // Status: downloaded checkmark, in-progress indicator, or a
                // download action for this specific item — independent of
                // sibling rows and of the entry-level download button (see
                // `queue-per-item-downloads`). Cancelling an in-progress
                // download is done from the activity panel rather than a
                // second cancel control here.
                let is_downloading = self.controller
                                         .read(cx)
                                         .is_file_queued_or_active(&self.entry_id, row_ix as u32);
                let is_verifying = self.controller
                                       .read(cx)
                                       .is_verifying_downloads(&self.entry_id);

                if is_verifying {
                    div().h_full()
                         .flex()
                         .items_center()
                         .id(SharedString::from(format!("item-row-{row_ix}-verifying")))
                         .tooltip(move |window, cx| {
                             Tooltip::new(t!("detail.tooltip_verifying_download").to_string())
                                .build(window, cx)
                         })
                         .child(Spinner::new().with_size(Size::XSmall)
                                              .color(colors.text_tertiary))
                         .into_any_element()
                }
                else if file.downloaded {
                    div().h_full()
                         .flex()
                         .items_center()
                         .text_sm()
                         .text_color(colors.text_secondary)
                         .id(SharedString::from(format!("item-row-{row_ix}-status")))
                         .tooltip(move |window, cx| {
                             Tooltip::new(t!("detail.item_status_local").to_string()).build(window,
                                                                                            cx)
                         })
                         // Checkmark for "on this device", matching
                         // `render_status_icon`'s glyph convention rather than a
                         // separate `IconName` so both status indicators read the
                         // same way.
                         .child("\u{2713}")
                         .into_any_element()
                }
                else if is_downloading {
                    div().h_full()
                         .flex()
                         .items_center()
                         .text_sm()
                         .text_color(colors.text_tertiary)
                         .child(t!("detail.item_status_downloading").to_string())
                         .into_any_element()
                }
                else {
                    let controller = self.controller.clone();
                    let entry_id = Arc::clone(&self.entry_id);
                    let title = self.controller
                                    .read(cx)
                                    .item_by_id(&self.entry_id)
                                    .map(|item| item.title.to_string())
                                    .unwrap_or_default();
                    div().h_full()
                         .flex()
                         .items_center()
                         .text_sm()
                         .text_color(colors.text_secondary)
                         .id(SharedString::from(format!("item-row-{row_ix}-download")))
                         .cursor_pointer()
                         .tooltip(move |window, cx| {
                             Tooltip::new(t!("detail.item_download_button").to_string())
                                .build(window, cx)
                         })
                         // Cloud glyph for "in the cloud, not yet on this device" —
                         // an arrow-down here previously read as an already-completed
                         // download indicator rather than a pending one.
                         .child("\u{2601}")
                         // Stop propagation so clicking download doesn't also fire the
                         // row's own click (which selects this row) — same class of bug
                         // fixed in the activity panel's cancel button.
                         .on_click(move |_, _, cx| {
                             cx.stop_propagation();
                             controller.update(cx, |ctrl, cx| {
                                           ctrl.request_item_download(&entry_id,
                                                                      row_ix as u32,
                                                                      title.clone(),
                                                                      cx);
                                       });
                         })
                         .into_any_element()
                }
            }
            _ => div().into_any_element(),
        }
    }

    fn render_th(&mut self, col_ix: usize, _window: &mut Window,
                 cx: &mut Context<TableState<Self>>)
                 -> impl IntoElement {
        let name = self.column(col_ix, cx).name;
        let label_font_family = cx.global::<LibriTheme>().fonts.label_font.clone();
        div().h_full()
             .flex()
             .items_center()
             .text_sm()
             .font_family(label_font_family.to_string())
             .font_weight(gpui::FontWeight::MEDIUM)
             .child(name)
    }
}
