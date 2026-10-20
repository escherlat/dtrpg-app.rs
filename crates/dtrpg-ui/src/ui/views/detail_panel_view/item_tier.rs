use super::*;

/// Renders the item tier for a multi-item entry: a persistent, selectable
/// item list and an item metadata area that updates in place on selection.
///
/// See `catalog-entry-detail-view`'s persistent-item-list and
/// update-in-place requirements.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_item_tier(item: &LibraryItem, storage_root_path: &Path,
                               entity: Entity<LibraryController>, tabs: &Entity<TabsController>,
                               colors: &ColorTokens, window: &mut Window, cx: &mut App)
                               -> impl IntoElement + 'static {
    let entry_id = Arc::clone(&item.id);
    let selected_ix = entity.read(cx).selected_item_file(&entry_id);
    let entry_dir = resolved_item_dir(&StorageConfig::load(), item);

    let table = item_list_table(tabs, &entity, &entry_id, entry_dir, window, cx);
    let table_for_prepaint = table.clone();

    // `DataTable` virtualizes its rows and sizes itself to its container
    // (`.size_full()` internally) — unlike the old stateless `Table`, it
    // renders nothing if given an unbounded height, which is what the
    // surrounding naturally-scrolling detail-tab content provides. Give it
    // an explicit height sized to the row count (capped so a very large
    // bundle scrolls internally instead of pushing the rest of the tab off
    // screen).
    let row_height = Size::default().table_row_height();
    let visible_rows = item.files.len().min(8) as f32;
    let table_height = row_height * (visible_rows + 1.0);
    let item_list =
        div().h(table_height)
             .child(DataTable::new(&table).bordered(false)
                                          .scrollbar_visible(true, false))
             // Captures the table's rendered width so the Name column can
             // fill whatever space Type/Size/Status don't use — see
             // `ItemListDelegate::table_width`. Mirrors
             // `LibraryController::set_entry_bounds`'s guarded-update
             // pattern so an unchanged width doesn't re-notify every frame.
             .on_prepaint(move |bounds, _window, cx| {
                 table_for_prepaint.update(cx, |state, cx| {
                                       if state.delegate().table_width != Some(bounds.size.width) {
                                           state.delegate_mut().table_width =
                                               Some(bounds.size.width);
                                           cx.notify();
                                       }
                                   });
             });

    let selected_file = selected_ix.and_then(|ix| item.files.get(ix).map(|file| (ix, file)));

    div().flex()
         .flex_col()
         .gap(px(12.0))
         .child(div().text_sm()
                     .font_weight(gpui::FontWeight::SEMIBOLD)
                     .text_color(colors.text_primary)
                     .child(t!("detail.items_heading").to_string()))
         .child(div().text_sm()
                     .text_color(colors.text_tertiary)
                     .child(t!("detail.item_prompt_select").to_string())
                     .into_any_element())
         .child(item_list)
         .child(match selected_file {
                    Some((ix, file)) => render_item_metadata(item,
                                                             file,
                                                             ix,
                                                             storage_root_path,
                                                             entity.clone(),
                                                             colors,
                                                             cx).into_any_element(),
                    None => div().into_any_element(),
                })
}

/// Renders the metadata area for a single selected item within a multi-item
/// entry: name, type, format, file size, the entry's download state
/// (individual files share the entry's on-disk download state today; see
/// `define-rust-catalog-entry-detail-view`'s open questions for future
/// per-file tracking), and a per-file "Other details" disclosure (file id,
/// download location).
///
/// `row_ix` is the file's position within the entry's `files` list — used to
/// key element ids and disclosure state, since the API has been observed to
/// reuse the same download id across genuinely distinct files within a
/// bundle (see `LibraryItem::dedupe_files`).
pub(super) fn render_item_metadata(item: &LibraryItem, file: &LibraryItemFile, row_ix: usize,
                                   storage_root_path: &Path, entity: Entity<LibraryController>,
                                   colors: &ColorTokens, cx: &App)
                                   -> impl IntoElement + 'static {
    let label_font_family = cx.global::<LibriTheme>().fonts.label_font.clone();
    let value_font_family = cx.global::<LibriTheme>().fonts.value_font.clone();
    let name_value = copyable_value(SharedString::from(format!("file-name-{row_ix}")),
                                    file.name.to_string(),
                                    &value_font_family);

    let file_size_value = {
        let entry_dir =
            dtrpg_core::data::storage::publisher_dir(storage_root_path, &item.publisher);
        let on_disk = crate::util::file_size::on_disk_file_size(&entry_dir, file.name.as_ref());
        crate::util::file_size::prefer_on_disk_size(format!("{:.1} {}",
                                                            file.size_mb,
                                                            t!("size.mb")),
                                                    on_disk)
    };

    let metadata = DescriptionList::vertical()
        .columns(2)
        .bordered(false)
        .child(DescriptionItem::new(styled_label(t!("detail.item_list_column_name").to_string(),
                                                 &label_font_family))
                   .value(name_value)
                   .span(2))
        .child(DescriptionItem::new(styled_label(t!("detail.field_format").to_string(),
                                                 &label_font_family))
                   .value(styled_value(file.format.to_string(), &value_font_family)))
        .child(DescriptionItem::new(styled_label(t!("detail.field_file_size").to_string(),
                                                 &label_font_family))
                   .value(styled_value(file_size_value, &value_font_family)))
        // .child(DescriptionItem::new(t!("detail.field_status").to_string())
        //            .value(if item.status == ItemStatus::Downloaded {
        //                t!("detail.status_on_device").to_string()
        //            } else {
        //                t!("detail.status_in_cloud").to_string()
        //            })
        //            .span(2))
    ;

    div().flex()
         .flex_col()
         .gap(px(12.0))
         .child(metadata)
         .child(render_file_other_details(FileOtherDetailsContext { entry_id: &item.id,
                                                                    publisher: &item.publisher,
                                                                    row_ix,
                                                                    is_downloaded:
                                                                        file.downloaded,
                                                                    storage_root_path },
                                          file,
                                          entity,
                                          colors,
                                          cx))
}

/// Builds the shared toggle-state key for a single file's "Other details"
/// disclosure within a multi-item entry's item tier.
pub(super) fn file_other_details_key(entry_id: &str, row_ix: usize) -> Arc<str> {
    Arc::from(format!("{entry_id}:{row_ix}"))
}

/// Grouped context for [`render_file_other_details`], kept below Rust's
/// preferred argument count.
pub(super) struct FileOtherDetailsContext<'a> {
    entry_id:          &'a str,
    publisher:         &'a str,
    row_ix:            usize,
    is_downloaded:     bool,
    storage_root_path: &'a Path,
}

/// Renders a single file's "Other details" disclosure section: a
/// clickable header that toggles a collapsed-by-default panel showing the
/// file's id and its on-disk download location.
///
/// The download location is the entry's shared item folder — per-file paths
/// aren't tracked separately (see `LibraryItemFile`), so every file in a
/// bundle reports the same location.
pub(super) fn render_file_other_details(ctx: FileOtherDetailsContext<'_>,
                                        file: &LibraryItemFile,
                                        entity: Entity<LibraryController>, colors: &ColorTokens,
                                        cx: &App)
                                        -> impl IntoElement + 'static {
    let FileOtherDetailsContext { entry_id,
                                  publisher,
                                  row_ix,
                                  is_downloaded,
                                  storage_root_path, } = ctx;
    let toggle_key = file_other_details_key(entry_id, row_ix);
    let open = entity.read(cx).is_file_other_details_open(&toggle_key);
    let label_font_family = cx.global::<LibriTheme>().fonts.label_font.clone();
    let value_font_family = cx.global::<LibriTheme>().fonts.value_font.clone();

    let toggle_entity = entity;
    let toggle_key_for_click = Arc::clone(&toggle_key);

    let header = div().id(SharedString::from(format!("file-other-details-header-{toggle_key}")))
                      .flex()
                      .items_center()
                      .gap(px(6.0))
                      .cursor_pointer()
                      .text_sm()
                      .font_weight(gpui::FontWeight::SEMIBOLD)
                      .text_color(colors.text_primary)
                      .on_click(move |_, _, cx| {
                          toggle_entity.update(cx, |ctrl, cx| {
                              ctrl.toggle_file_other_details(Arc::clone(&toggle_key_for_click), cx);
                          });
                      })
                      .child(Icon::new(if open {
                                           IconName::ChevronUp
                                       }
                                       else {
                                           IconName::ChevronDown
                                       }).text_color(colors.text_secondary))
                      .child(t!("detail.other_details_heading").to_string());

    let path_value = if is_downloaded {
        dtrpg_core::data::storage::publisher_dir(storage_root_path, publisher).display()
                                                                              .to_string()
    }
    else {
        value_or_dash("")
    };

    let content = DescriptionList::vertical()
        .columns(2)
        .bordered(false)
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_file_id").to_string(),
                &label_font_family,
            ))
            .value(copyable_value(
                SharedString::from(format!("file-id-{toggle_key}")),
                file.id.to_string(),
                &value_font_family,
            ))
            .span(2),
        )
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_download_location").to_string(),
                &label_font_family,
            ))
            .value(copyable_value(
                SharedString::from(format!("file-path-{toggle_key}")),
                path_value,
                &value_font_family,
            ))
            .span(2),
        );

    Collapsible::new().gap(px(8.0))
                      .open(open)
                      .child(header)
                      .content(content)
}
