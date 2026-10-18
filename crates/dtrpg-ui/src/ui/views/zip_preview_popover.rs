//! Zip content preview popover: anchored, hover-to-preview / click-to-pin
//! popover listing a Zip file row's internal entries within the expanded
//! detail tab's item tier (see `zip-content-preview`).
//!
//! Mirrors `item_popover_view`'s anchored/deferred pattern rather than
//! `gpui-kit::Popover`, since it needs hover-to-open and click-to-pin
//! as distinct behaviors tied to controller state.

use std::path::Path;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{
    AnyElement, Entity, Hsla, IntoElement, Pixels, Point, Styled, anchored, deferred, div, px,
};
use gpui_kit::component::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::data::theme::ColorTokens;
use crate::util::file_size::format_bytes;
use crate::util::zip_preview::{ZipEntry, list_entries};

const POPOVER_WIDTH: f32 = 280.0;
const POPOVER_MARGIN: f32 = 8.0;
const POPOVER_LIST_MAX_HEIGHT: f32 = 220.0;

/// Renders the Zip content preview popover anchored at `position`, listing
/// `path`'s internal entries, or an inline "preview unavailable" state if
/// the archive can't be read.
pub fn render_zip_preview_popover(entry_id: Arc<str>, path: &Path, position: Point<Pixels>,
                                  entity: Entity<LibraryController>, colors: &ColorTokens)
                                  -> AnyElement {
    let surface = colors.surface;
    let border = colors.border;
    let text_primary = colors.text_primary;
    let text_secondary = colors.text_secondary;

    let body = match list_entries(path) {
        Ok(entries) => render_entry_list(&entries, text_primary, text_secondary),
        Err(_) => div().text_sm()
                       .text_color(text_secondary)
                       .child(t!("detail.zip_preview_unavailable").to_string())
                       .into_any_element(),
    };

    let content =
        div().id("zip-preview-popover")
             .occlude()
             .w(px(POPOVER_WIDTH))
             .bg(surface)
             .border_1()
             .border_color(border)
             .rounded(px(8.0))
             .shadow_lg()
             .p(px(12.0))
             .flex()
             .flex_col()
             .gap(px(8.0))
             .child(div().flex()
                         .items_center()
                         .justify_between()
                         .gap(px(8.0))
                         .child(div().text_sm()
                                     .font_weight(gpui::FontWeight::SEMIBOLD)
                                     .text_color(text_primary)
                                     .child(t!("detail.zip_preview_heading").to_string()))
                         .child(Button::new("zip-preview-close").ghost()
                                                                .compact()
                                                                .icon(IconName::Close)
                                                                .on_click(move |_, _, cx| {
                                                                    entity.update(cx, |ctrl, cx| {
                                                          ctrl.close_zip_preview(&entry_id, cx);
                                                      });
                                                                })))
             .child(body);

    deferred(anchored().snap_to_window_with_margin(px(POPOVER_MARGIN))
                       .position(position)
                       .child(content)).with_priority(1)
                                       .into_any_element()
}

/// Renders the scrollable list of a Zip archive's internal entries (name and
/// size per row), capped at a fixed max height so large archives scroll
/// instead of growing the popover unbounded.
fn render_entry_list(entries: &[ZipEntry], text_primary: Hsla, text_secondary: Hsla) -> AnyElement {
    if entries.is_empty() {
        return div().text_sm()
                    .text_color(text_secondary)
                    .child(t!("detail.zip_preview_empty").to_string())
                    .into_any_element();
    }

    // `overflow_y_scrollbar()` wraps its element in `Scrollable`, whose
    // `render` only copies the `size` style field onto its own outer
    // wrapper — `max_h` sets `max_size` and would be silently dropped if
    // placed on the scrollable div itself. The height cap lives on this
    // plain outer div instead; the inner div just fills it via
    // `size_full()`, which `Scrollable` already applies on its own.
    div().max_h(px(POPOVER_LIST_MAX_HEIGHT))
         .child(div().flex()
                     .flex_col()
                     .size_full()
                     .overflow_y_scrollbar()
                     .gap(px(4.0))
                     .children(entries.iter().map(|entry| {
                                                 div().flex()
                              .items_center()
                              .justify_between()
                              .gap(px(8.0))
                              .child(div().flex_1()
                                          .min_w_0()
                                          .text_sm()
                                          .text_color(text_primary)
                                          .truncate()
                                          .child(entry.name.clone()))
                              .child(div().flex_none()
                                          .text_xs()
                                          .text_color(text_secondary)
                                          .child(format_bytes(entry.size_bytes)))
                                             })))
         .into_any_element()
}
