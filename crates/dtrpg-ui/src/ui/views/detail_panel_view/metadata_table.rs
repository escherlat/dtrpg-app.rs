use super::*;

/// Renders a text value with an appear-on-hover copy button, in the app's
/// dedicated value font.
///
/// `field_id` must be unique within the surrounding view — it doubles as the
/// hover group name and the copy button's element id.
pub(super) fn copyable_value(field_id: SharedString, value: impl Into<SharedString>,
                             value_font_family: &str)
                             -> AnyElement {
    let value: SharedString = value.into();

    div()
        .id(SharedString::from(format!("{field_id}-row")))
        .group(field_id.clone())
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(
            selectable_text(
                SharedString::from(format!("{field_id}-text")),
                value.clone(),
            )
            .font_family(value_font_family.to_string()),
        )
        .child(
            div()
                .invisible()
                .group_hover(field_id.clone(), |d| d.visible())
                .child(
                    Clipboard::new(SharedString::from(format!("{field_id}-copy")))
                        .value(value)
                        .tooltip(t!("detail.copy_tooltip").to_string()),
                ),
        )
        .into_any_element()
}

/// Renders a `DescriptionList` label in the app's dedicated label font and in
/// small caps, distinguishing field names from their values (rendered in the
/// dedicated value font — see [`styled_value`]).
pub(super) fn styled_label(label: impl Into<SharedString>, label_font_family: &str) -> AnyElement {
    div().font_family(label_font_family.to_string())
         .child(small_caps_text(label))
         .into_any_element()
}

/// Renders a `DescriptionList` value in the app's dedicated value font, for
/// plain text values that don't need [`copyable_value`]'s copy button.
pub(super) fn styled_value(value: impl Into<SharedString>, value_font_family: &str) -> AnyElement {
    div().font_family(value_font_family.to_string())
         .child(value.into())
         .into_any_element()
}

pub(super) fn platform_reveal_label() -> std::borrow::Cow<'static, str> {
    match Platform.kind() {
        PlatformKind::MacOs => t!("detail.show_in_finder"),
        PlatformKind::Windows => t!("detail.show_in_explorer"),
        PlatformKind::Linux => t!("detail.show_in_files"),
    }
}

/// Renders a small status icon next to the item title: a checkmark when
/// downloaded, a cloud glyph otherwise. Replaces the old text-only "Status" row
/// in the metadata table.
/// Renders the detail tab's title-row status glyph. While `is_verifying` is
/// true (a file-presence check per `verify-downloaded-status-against-disk`
/// is in flight), shows a distinct pending glyph and tooltip rather than a
/// downloaded/cloud state that may be stale.
pub(super) fn render_status_icon(is_downloaded: bool, is_offloaded: bool, is_verifying: bool,
                                 color: gpui::Hsla)
                                 -> impl IntoElement + 'static {
    let (glyph, tooltip_text) = if is_verifying {
        ("\u{22ef}", t!("detail.tooltip_verifying_download").to_string())
    }
    else if is_offloaded {
        ("\u{2601}", t!("detail.tooltip_offloaded_download").to_string())
    }
    else if is_downloaded {
        ("\u{2713}", t!("detail.status_on_device").to_string())
    }
    else {
        ("\u{2601}", t!("detail.status_in_cloud").to_string())
    };

    div().id("detail-status-icon")
         .text_sm()
         .text_color(color)
         .tooltip(move |window, cx| Tooltip::new(tooltip_text.clone()).build(window, cx))
         .child(glyph)
}

/// Renders a metadata value, falling back to an em dash when the underlying
/// data is empty (e.g. the API did not report a game system/line for this
/// item).
pub(super) fn value_or_dash(value: &str) -> String {
    if value.trim().is_empty() {
        "\u{2014}".to_string()
    }
    else {
        value.to_string()
    }
}

pub(super) fn render_metadata_table(item: &LibraryItem, storage_root_path: &Path,
                                    label_font_family: &str, value_font_family: &str)
                                    -> impl IntoElement + 'static + use<> {
    let file_size_label = if item.files.len() > 1 {
        t!("detail.field_total_file_size").to_string()
    }
    else {
        t!("detail.field_file_size").to_string()
    };

    // Three display modes depending on how many of the entry's files are
    // downloaded: none → catalog (cloud) size only; all → on-disk (local)
    // size only, since it's the exact figure once every file is present;
    // some → both, so the user can see progress against the total.
    let file_size_value = {
        let catalog_mb: f64 = if item.files.is_empty() {
            item.size_mb
        }
        else {
            item.files.iter().map(|f| f.size_mb).sum()
        };
        let catalog_str = format!("{:.0} {}", catalog_mb, t!("size.mb"));

        if item.files.is_empty() {
            catalog_str
        }
        else {
            let entry_dir =
                dtrpg_core::data::storage::publisher_dir(storage_root_path, &item.publisher);
            let downloaded_count = item.files.iter().filter(|f| f.downloaded).count();
            let resolved: Vec<u64> =
                item.files
                    .iter()
                    .filter(|f| f.downloaded)
                    .filter_map(|f| {
                        crate::util::file_size::on_disk_file_size(&entry_dir, f.name.as_ref())
                    })
                    .collect();

            if downloaded_count == 0 {
                catalog_str
            }
            else if downloaded_count == item.files.len() && resolved.len() == item.files.len() {
                crate::util::file_size::format_bytes(resolved.iter().sum())
            }
            else {
                let on_disk_bytes = if resolved.is_empty() {
                    None
                }
                else {
                    Some(resolved.iter().sum())
                };
                crate::util::file_size::with_on_disk_suffix(catalog_str, on_disk_bytes)
            }
        }
    };

    let mut list = DescriptionList::vertical()
        .columns(2)
        .bordered(false)
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_system").to_string(),
                label_font_family,
            ))
            .value(Text::from(
                selectable_text("detail-field-system", value_or_dash(&item.line))
                    .font_family(value_font_family.to_string()),
            )),
        )
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_released").to_string(),
                label_font_family,
            ))
            .value(Text::from(
                selectable_text("detail-field-released", item.year.to_string())
                    .font_family(value_font_family.to_string()),
            )),
        )
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_format").to_string(),
                label_font_family,
            ))
            .value(Text::from(
                selectable_text("detail-field-format", item.format.to_string())
                    .font_family(value_font_family.to_string()),
            )),
        )
        .child(
            DescriptionItem::new(styled_label(file_size_label, label_font_family)).value(
                Text::from(
                    selectable_text("detail-field-file-size", file_size_value)
                        .font_family(value_font_family.to_string()),
                ),
            ),
        )
        .child(
            DescriptionItem::new(styled_label(
                t!("detail.field_category").to_string(),
                label_font_family,
            ))
            .value(Text::from(
                selectable_text("detail-field-kind", item.kind.to_string())
                    .font_family(value_font_family.to_string()),
            ))
            .span(2),
        );

    // The DriveThruRPG order-product API does not always report a page count;
    // omit the row entirely rather than showing a misleading "0".
    if item.pages > 0 {
        list = list.child(
            DescriptionItem::new(styled_label(
                t!("detail.field_pages").to_string(),
                label_font_family,
            ))
            .value(Text::from(
                selectable_text("detail-field-pages", item.pages.to_string())
                    .font_family(value_font_family.to_string()),
            ))
            .span(2),
        );
    }

    if let Some(ts) = item.date_added {
        let value = render_relative_date_value(&item.id, "added", ts, value_font_family);
        list = list.child(DescriptionItem::new(styled_label(t!("detail.field_added").to_string(),
                                                            label_font_family)).value(value)
                                                                               .span(2));
    }

    if let Some(ts) = item.date_updated {
        let value = render_relative_date_value(&item.id, "updated", ts, value_font_family);
        list =
            list.child(DescriptionItem::new(styled_label(t!("detail.field_updated").to_string(),
                                                         label_font_family)).value(value)
                                                                            .span(2));
    }

    list
}
