//! Toolbar view: section title, search, sort dropdown, group toggle, layout
//! switcher.

use gpui::prelude::*;
use gpui::{Entity, IntoElement, MouseButton, ParentElement, Styled, div, px};
use gpui_component::Icon;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::tab::{Tab, TabBar};
use gpui_component::tooltip::Tooltip;
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::data::{enums::CatalogPresentation, theme::ColorTokens};
use crate::util::filter::*;
use crate::util::pluralize::pluralize;
use crate::util::sort::*;

/// Human-readable title for a sidebar filter, used by both the catalog tab
/// header and the status bar's active-tab summary.
pub(crate) fn section_title_for(filter: &SidebarFilter) -> String {
    match filter {
        SidebarFilter::AllTitles => t!("sidebar.all_titles").to_string(),
        SidebarFilter::RecentlyUpdated => t!("sidebar.recently_updated").to_string(),
        SidebarFilter::OnDevice => t!("sidebar.on_this_device").to_string(),
        SidebarFilter::InCloud => t!("sidebar.in_the_cloud").to_string(),
        SidebarFilter::Publisher(name) => {
            t!("toolbar.filter_title_publisher", name = name).to_string()
        }
        SidebarFilter::Collection(_, name) => {
            t!("toolbar.filter_title_collection", name = name).to_string()
        }
    }
}

/// Builds the count text shown beneath the section title.
///
/// `filter_count` is the number of catalog items matching `filter` alone
/// (ignoring search); `matched_count` additionally applies the search query;
/// `total_count` is the whole-catalog count. Publisher filters keep the legacy
/// "publisher item(s), total item(s)" wording; `AllTitles` shows the plain
/// catalog total; every other filter (smart sections, collections) shows
/// `filter_count` since the filter itself already narrows the result set below
/// the catalog total.
fn count_label_for(filter: &SidebarFilter, filter_count: usize, matched_count: usize,
                   total_count: usize, has_search: bool)
                   -> String {
    if matches!(filter, SidebarFilter::Publisher(_)) {
        let base = format!("{}, {}",
                           pluralize(filter_count,
                                     "count.publisher_item",
                                     "count.publisher_items"),
                           pluralize(total_count, "count.total_item", "count.total_items"),);
        return if has_search {
            format!("{base}{}", t!("toolbar.filtered_suffix", n = matched_count))
        }
        else {
            base
        };
    }

    let base_count = if matches!(filter, SidebarFilter::AllTitles) {
        total_count
    }
    else {
        filter_count
    };
    let base = pluralize(base_count, "count.item", "count.items");
    if has_search {
        format!("{base}{}", t!("toolbar.filtered_suffix", n = matched_count))
    }
    else {
        base
    }
}

/// Renders the toolbar row above the catalog.
#[allow(clippy::too_many_arguments)]
pub fn render_toolbar(filter: &SidebarFilter, filter_count: usize, matched_count: usize,
                      total_count: usize, search_query: &str, search_input: Entity<InputState>,
                      sort: SortMethod, sort_direction: SortDirection, grouped: bool,
                      presentation: CatalogPresentation, entity: Entity<LibraryController>,
                      colors: &ColorTokens)
                      -> impl IntoElement + 'static + use<> {
    let surface = colors.surface;
    let border = colors.border;
    let text_primary = colors.text_primary;
    let text_tertiary = colors.text_tertiary;

    let title = section_title_for(filter);
    let has_search = !search_query.is_empty();
    let count_label = count_label_for(filter, filter_count, matched_count, total_count, has_search);

    div().h(px(57.0))
         .flex_none()
         .flex()
         .items_center()
         .gap(px(16.0))
         .px(px(18.0))
         .border_b_1()
         .border_color(border)
         .bg(surface)
         // ── Title + count ─────────────────────────────────────────────────
         .child(div().flex()
                     .flex_col()
                     .gap(px(1.0))
                     .flex_shrink_0()
                     .max_w(px(560.0))
                     .child(div().text_color(text_primary)
                                 .font_weight(gpui::FontWeight::SEMIBOLD)
                                 .text_lg()
                                 .truncate()
                                 .child(title))
                     .child(div().text_xs()
                                 .text_color(text_tertiary)
                                 .whitespace_nowrap()
                                 .child(count_label)))
         // ── Spacer / drag region ──────────────────────────────────────────
         .child(div().id("toolbar-drag-region")
                     .flex_1()
                     .h_full()
                     .on_mouse_down(MouseButton::Left, |_, window, _| {
                         window.start_window_move();
                     }))
         // ── Controls ──────────────────────────────────────────────────────
         .child(div().flex()
                     .items_center()
                     .gap(px(10.0))
                     .child(Input::new(&search_input).w(px(188.0)).cleanable(true))
                     .child(render_sort_selector(sort, sort_direction, grouped, entity.clone()))
                     .child(render_layout_switcher(presentation, entity)))
}

// ── Sort selector
// ─────────────────────────────────────────────────────────────

fn render_sort_selector(current: SortMethod, direction: SortDirection, grouped: bool,
                        entity: Entity<LibraryController>)
                        -> impl IntoElement + 'static {
    let label = match current {
        SortMethod::Title => t!("toolbar.sort_title"),
        SortMethod::Publisher => t!("toolbar.sort_publisher"),
        SortMethod::DateAdded => t!("toolbar.sort_date_added"),
        SortMethod::PageCount => t!("toolbar.sort_pages"),
        SortMethod::Custom { .. } => t!("toolbar.sort_custom"),
    };

    let is_custom = matches!(current, SortMethod::Custom { .. });

    Button::new("sort-selector").icon(Icon::empty().path("icons/arrow-down-up.svg"))
                                .label(label)
                                .ghost()
                                .dropdown_caret(true)
                                .tooltip(t!("toolbar.sort_by"))
                                .dropdown_menu(move |menu, _, _| {
                                    let e = entity.clone();
                                    let e2 = entity.clone();
                                    let e3 = entity.clone();
                                    let e4 = entity.clone();
                                    let e5 = entity.clone();
                                    let e6 = entity.clone();
                                    let e7 = entity.clone();
                                    let mut m = menu
                .item(
                    PopupMenuItem::new(t!("toolbar.sort_title"))
                        .checked(current == SortMethod::Title)
                        .on_click(move |_, _, cx| {
                            e.update(cx, |ctrl, cx| ctrl.set_sort(SortMethod::Title, cx));
                        }),
                )
                .item(
                    PopupMenuItem::new(t!("toolbar.sort_publisher"))
                        .checked(current == SortMethod::Publisher)
                        .on_click(move |_, _, cx| {
                            e2.update(cx, |ctrl, cx| ctrl.set_sort(SortMethod::Publisher, cx));
                        }),
                )
                .item(
                    PopupMenuItem::new(t!("toolbar.sort_date_added"))
                        .checked(current == SortMethod::DateAdded)
                        .on_click(move |_, _, cx| {
                            e3.update(cx, |ctrl, cx| ctrl.set_sort(SortMethod::DateAdded, cx));
                        }),
                )
                .item(
                    PopupMenuItem::new(t!("toolbar.sort_pages"))
                        .checked(current == SortMethod::PageCount)
                        .on_click(move |_, _, cx| {
                            e4.update(cx, |ctrl, cx| ctrl.set_sort(SortMethod::PageCount, cx));
                        }),
                );
                                    if is_custom {
                                        m = m.item(
                    PopupMenuItem::new(t!("toolbar.sort_custom"))
                        .checked(true)
                        .disabled(true),
                );
                                    }
                                    m.separator()
                .item(
                    PopupMenuItem::new(t!("toolbar.sort_ascending"))
                        .checked(direction == SortDirection::Ascending)
                        .on_click(move |_, _, cx| {
                            e5.update(cx, |ctrl, cx| {
                                ctrl.set_sort_direction(SortDirection::Ascending, cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new(t!("toolbar.sort_descending"))
                        .checked(direction == SortDirection::Descending)
                        .on_click(move |_, _, cx| {
                            e6.update(cx, |ctrl, cx| {
                                ctrl.set_sort_direction(SortDirection::Descending, cx);
                            });
                        }),
                )
                .separator()
                .item(
                    PopupMenuItem::new(t!("toolbar.group_by_publisher"))
                        .checked(grouped)
                        .on_click(move |_, _, cx| {
                            e7.update(cx, |ctrl, cx| ctrl.set_grouped(!grouped, cx));
                        }),
                )
                                })
}

// ── Layout switcher
// ───────────────────────────────────────────────────────────

fn render_layout_switcher(current: CatalogPresentation, entity: Entity<LibraryController>)
                          -> impl IntoElement + 'static {
    let selected = match current {
        CatalogPresentation::List => 0usize,
        CatalogPresentation::Thumbs => 1,
        CatalogPresentation::Grid => 2,
    };
    TabBar::new("layout-switcher")
        .segmented()
        .selected_index(selected)
        .child(
            Tab::new()
                .icon(Icon::empty().path("icons/list.svg"))
                .tooltip(|window, cx| {
                    Tooltip::new(t!("toolbar.view_list").to_string()).build(window, cx)
                }),
        )
        .child(
            Tab::new()
                .icon(Icon::empty().path("icons/gallery-thumbnails.svg"))
                .tooltip(|window, cx| {
                    Tooltip::new(t!("toolbar.view_thumbs").to_string()).build(window, cx)
                }),
        )
        .child(
            Tab::new()
                .icon(Icon::empty().path("icons/layout-grid.svg"))
                .tooltip(|window, cx| {
                    Tooltip::new(t!("toolbar.view_grid").to_string()).build(window, cx)
                }),
        )
        .on_click(move |ix, _, cx| {
            let mode = match ix {
                0 => CatalogPresentation::List,
                1 => CatalogPresentation::Thumbs,
                _ => CatalogPresentation::Grid,
            };
            entity.update(cx, |ctrl, cx| ctrl.set_presentation(mode, cx));
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::filter::SidebarFilter;

    #[test]
    fn publisher_filter_includes_name() {
        let title = section_title_for(&SidebarFilter::Publisher("Kobold Press".into()));
        assert!(title.contains("Kobold Press"),
                "publisher name must appear in label");
    }

    #[test]
    fn collection_filter_includes_name() {
        let title = section_title_for(&SidebarFilter::Collection(42, "My Shelf".into()));
        assert!(title.contains("My Shelf"),
                "collection name must appear in label");
    }

    #[test]
    fn collection_filter_count_reflects_filter_count_not_total() {
        // Regression: selecting a collection must show the collection's own
        // item count, not the whole-catalog total (which stays fixed across
        // filter changes and made the count text look unresponsive).
        let filter = SidebarFilter::Collection(42, "My Shelf".into());
        let label = count_label_for(&filter, 3, 3, 500, false);
        assert!(label.contains('3'),
                "expected collection filter count (3) in label, got: {label}");
        assert!(!label.contains("500"),
                "collection label must not show the whole-catalog total, got: {label}");
    }

    #[test]
    fn smart_section_filter_count_reflects_filter_count_not_total() {
        let filter = SidebarFilter::OnDevice;
        let label = count_label_for(&filter, 7, 7, 500, false);
        assert!(label.contains('7'));
        assert!(!label.contains("500"));
    }

    #[test]
    fn all_titles_filter_count_shows_total() {
        let label = count_label_for(&SidebarFilter::AllTitles, 500, 500, 500, false);
        assert!(label.contains("500"));
    }

    #[test]
    fn collection_filter_with_search_shows_matched_count() {
        let filter = SidebarFilter::Collection(42, "My Shelf".into());
        let label = count_label_for(&filter, 10, 2, 500, true);
        assert!(label.contains("2 filtered"));
    }

    #[test]
    fn non_dynamic_filters_return_nonempty_string() {
        for filter in [SidebarFilter::AllTitles,
                       SidebarFilter::RecentlyUpdated,
                       SidebarFilter::OnDevice,
                       SidebarFilter::InCloud]
        {
            assert!(!section_title_for(&filter).is_empty());
        }
    }
}
