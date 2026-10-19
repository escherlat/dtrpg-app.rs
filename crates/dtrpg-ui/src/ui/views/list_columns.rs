//! The columns of the catalog's list layout: which are shown, how wide the
//! user has dragged them, how each sorts, and the menu that shows and hides
//! them.

use std::collections::HashMap;

use dtrpg_core::data::enums::ListColumn;
use dtrpg_core::util::sort::SortMethod;
use gpui::{Entity, Pixels};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::table::Column;
use rust_i18n::t;

use crate::controllers::library::LibraryController;

/// The text in a column's heading.
fn heading(column: ListColumn) -> String {
    match column {
        ListColumn::Title => t!("catalog.col_title").to_string(),
        ListColumn::Publisher => t!("catalog.col_publisher").to_string(),
        ListColumn::System => t!("catalog.col_system").to_string(),
        ListColumn::Pages => t!("catalog.col_pages").to_string(),
        ListColumn::Size => t!("catalog.col_size").to_string(),
        ListColumn::Added => t!("catalog.col_added").to_string(),
        ListColumn::Tags => t!("catalog.col_tags").to_string(),
        ListColumn::Status | ListColumn::Open => String::new(),
    }
}

/// The table's definition of `column`, at its usual width.
fn table_column(column: ListColumn) -> Column {
    let defined = Column::new(column.key(), heading(column));
    match column {
        ListColumn::Title => defined.width(300.).min_width(150.).resizable(true),
        ListColumn::Publisher => defined.width(130.).resizable(true),
        ListColumn::System => defined.width(110.).resizable(true),
        ListColumn::Pages | ListColumn::Size => defined.width(60.).resizable(true),
        ListColumn::Added => defined.width(80.).resizable(true),
        ListColumn::Tags => defined.width(160.).resizable(true),
        ListColumn::Status => defined.width(24.).resizable(false).selectable(false),
        ListColumn::Open => defined.width(28.).resizable(false).selectable(false),
    }
}

/// The sort a click on `column`'s heading asks for, if it can sort.
pub(super) fn sort_method_for(column: ListColumn) -> Option<SortMethod> {
    match column {
        ListColumn::Title => Some(SortMethod::Title),
        ListColumn::Publisher
        | ListColumn::System
        | ListColumn::Pages
        | ListColumn::Size
        | ListColumn::Added => Some(SortMethod::Custom { col_key: column.key(), }),
        ListColumn::Tags | ListColumn::Status | ListColumn::Open => None,
    }
}

/// The column the list is sorted by, if it has one.
pub(super) fn column_sorted_by(sort: SortMethod) -> Option<ListColumn> {
    match sort {
        SortMethod::Title => Some(ListColumn::Title),
        SortMethod::Publisher => Some(ListColumn::Publisher),
        SortMethod::PageCount => Some(ListColumn::Pages),
        SortMethod::DateAdded => Some(ListColumn::Added),
        SortMethod::Custom { col_key } => ListColumn::from_key(col_key),
    }
}

/// The columns the table shows, and the widths the user has dragged them to.
///
/// Both list delegates hold one, so the headings and the cells of the grouped
/// and ungrouped lists always line up.
pub(super) struct ListColumnSet {
    shown:       Vec<ListColumn>,
    columns:     Vec<Column>,
    /// Index for index with `shown`: `None` means the usual width.
    user_widths: Vec<Option<Pixels>>,
}

impl ListColumnSet {
    /// The columns shown when `chosen` are the optional ones chosen.
    pub(super) fn new(chosen: &[ListColumn]) -> Self {
        let shown = ListColumn::shown(chosen);
        Self { columns: shown.iter().map(|column| table_column(*column)).collect(),
               user_widths: vec![None; shown.len()],
               shown }
    }

    /// How many columns are shown.
    pub(super) fn len(&self) -> usize {
        self.shown.len()
    }

    /// The column at `index`.
    pub(super) fn kind(&self, index: usize) -> Option<ListColumn> {
        self.shown.get(index).copied()
    }

    /// Where `column` is, if it is shown.
    pub(super) fn position_of(&self, column: ListColumn) -> Option<usize> {
        self.shown.iter().position(|shown| *shown == column)
    }

    /// The table's definition of the column at `index`, at the width the user
    /// has dragged it to, if they have.
    pub(super) fn column(&self, index: usize) -> Column {
        let column = self.columns[index].clone();
        match self.user_widths.get(index) {
            Some(Some(width)) => column.width(width.as_f32()),
            _ => column,
        }
    }

    /// Records the widths the table reports after the user drags a column.
    /// Widths for a different set of columns, as can arrive while the columns
    /// are changing, are ignored.
    pub(super) fn apply_widths(&mut self, widths: &[Pixels]) {
        if widths.len() != self.user_widths.len() {
            return;
        }
        for (slot, width) in self.user_widths.iter_mut().zip(widths) {
            *slot = Some(*width);
        }
    }

    /// Shows the columns for `chosen`, keeping the width of each column that
    /// stays. Returns whether anything changed.
    pub(super) fn set_chosen(&mut self, chosen: &[ListColumn]) -> bool {
        let shown = ListColumn::shown(chosen);
        if shown == self.shown {
            return false;
        }
        let kept: HashMap<ListColumn, Pixels> =
            self.shown
                .iter()
                .zip(&self.user_widths)
                .filter_map(|(column, width)| width.map(|width| (*column, width)))
                .collect();
        self.columns = shown.iter().map(|column| table_column(*column)).collect();
        self.user_widths = shown.iter()
                                .map(|column| kept.get(column).copied())
                                .collect();
        self.shown = shown;
        true
    }
}

/// Adds to `menu` an item for each column the user can show or hide, checked
/// when `chosen` has it, that shows or hides it when clicked.
pub(super) fn column_chooser_menu(menu: PopupMenu, chosen: &[ListColumn],
                                  controller: &Entity<LibraryController>)
                                  -> PopupMenu {
    let mut menu = menu;
    for column in ListColumn::CHOOSABLE {
        let controller = controller.clone();
        menu = menu.item(PopupMenuItem::new(heading(column)).checked(chosen.contains(&column))
                                                            .on_click(move |_, _, cx| {
                                                                controller.update(cx, |ctrl, cx| {
                                                                    ctrl.toggle_list_column(column,
                                                                                            cx);
                                                                });
                                                            }));
    }
    menu
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use gpui::px;

    use super::*;

    #[test]
    fn the_columns_follow_the_chosen_ones_with_the_title_first_and_status_and_open_last() {
        let columns = ListColumnSet::new(&[ListColumn::Tags, ListColumn::Size]);

        assert_eq!(columns.len(), 5);
        assert_eq!(columns.kind(0), Some(ListColumn::Title));
        assert_eq!(columns.position_of(ListColumn::Size), Some(1));
        assert_eq!(columns.position_of(ListColumn::Tags), Some(2));
        assert_eq!(columns.kind(4), Some(ListColumn::Open));
        assert_eq!(columns.position_of(ListColumn::Pages), None);
    }

    #[test]
    fn a_dragged_width_stays_with_its_column_when_others_come_and_go() {
        let mut columns = ListColumnSet::new(&[ListColumn::Publisher, ListColumn::Size]);
        // Title, Publisher, Size, Status, Open.
        columns.apply_widths(&[px(300.), px(200.), px(60.), px(24.), px(28.)]);

        let changed = columns.set_chosen(&[ListColumn::Size, ListColumn::Tags]);

        assert!(changed);
        // Title, Size, Tags, Status, Open: Publisher's width is gone, Size's is
        // not.
        let size = columns.position_of(ListColumn::Size).unwrap();
        let tags = columns.position_of(ListColumn::Tags).unwrap();
        assert_eq!(columns.column(size).width, px(60.));
        assert_eq!(columns.column(tags).width, px(160.));
    }

    #[test]
    fn choosing_the_same_columns_again_changes_nothing() {
        let mut columns = ListColumnSet::new(&ListColumn::DEFAULT_CHOSEN);

        assert!(!columns.set_chosen(&ListColumn::DEFAULT_CHOSEN));
    }

    #[test]
    fn widths_for_a_different_number_of_columns_are_ignored() {
        let mut columns = ListColumnSet::new(&[]);

        columns.apply_widths(&[px(500.)]);

        assert_eq!(columns.column(0).width, px(300.));
    }

    #[test]
    fn a_heading_sorts_by_its_column_and_the_sort_names_its_heading() {
        for column in ListColumn::shown(&ListColumn::CHOOSABLE) {
            match sort_method_for(column) {
                Some(method) => {
                    assert!(column.is_sortable());
                    assert_eq!(column_sorted_by(method), Some(column));
                }
                None => assert!(!column.is_sortable()),
            }
        }
    }

    #[test]
    fn the_older_named_sorts_light_up_their_columns() {
        assert_eq!(column_sorted_by(SortMethod::Publisher),
                   Some(ListColumn::Publisher));
        assert_eq!(column_sorted_by(SortMethod::PageCount),
                   Some(ListColumn::Pages));
        assert_eq!(column_sorted_by(SortMethod::DateAdded),
                   Some(ListColumn::Added));
        assert_eq!(column_sorted_by(SortMethod::Custom { col_key: "nonsense", }),
                   None);
    }
}
