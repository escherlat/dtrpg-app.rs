//! Tabs controller: tracks which main-window tabs are open and active.
//!
//! The catalog tab is always open and non-closable. Expanded detail tabs are
//! opened by double-clicking a catalog item and closed via their tab's close
//! button — see the `main-window-tabs` capability.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{App, AppContext as _, Context, Entity, Window};
use gpui_kit::component::table::{TableEvent, TableState};

use crate::controllers::library::LibraryController;
use crate::data::events::TabsChanged;
use crate::ui::views::detail_panel_view::{ItemListDelegate, item_list_columns};

/// Identifies a tab in the main window's tab strip.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TabTarget {
    /// The always-open, non-closable catalog tab.
    Catalog,
    /// A closable expanded detail tab for the catalog item with this id.
    Detail(Arc<str>),
}

/// Read-only snapshot of tab state for rendering.
#[derive(Clone)]
pub struct TabsSnapshot {
    /// Open tabs, catalog first, in tab-strip order.
    pub open_tabs: Vec<TabTarget>,
    /// The currently active tab.
    pub active:    TabTarget,
    /// Display titles for open detail tabs, keyed by item id.
    pub titles:    HashMap<Arc<str>, String>,
}

/// Owns the set of open tabs and which one is active.
pub struct TabsController {
    open_tabs:        Vec<TabTarget>,
    active:           TabTarget,
    titles:           HashMap<Arc<str>, String>,
    /// Per-entry item list `TableState`, keyed by entry id. Gives the
    /// expanded detail tab's item list a persistent home so column widths
    /// and selection survive re-renders of the tab — see
    /// `detail-item-list-column-resize`. Evicted in `close_detail_tab` so
    /// the cache doesn't grow unbounded across a session.
    item_list_tables: HashMap<Arc<str>, Entity<TableState<ItemListDelegate>>>,
}

impl TabsController {
    /// Creates a new controller with only the catalog tab open and active.
    #[must_use]
    pub fn new() -> Self {
        Self { open_tabs:        vec![TabTarget::Catalog],
               active:           TabTarget::Catalog,
               titles:           HashMap::new(),
               item_list_tables: HashMap::new(), }
    }

    /// Returns a snapshot of the current tab state.
    #[must_use]
    pub fn snapshot(&self) -> TabsSnapshot {
        TabsSnapshot { open_tabs: self.open_tabs.clone(),
                       active:    self.active.clone(),
                       titles:    self.titles.clone(), }
    }

    /// Opens (or activates, if already open) an expanded detail tab for `id`.
    pub fn open_detail_tab(&mut self, id: Arc<str>, title: String, cx: &mut Context<Self>) {
        let target = TabTarget::Detail(id.clone());
        if !self.open_tabs.contains(&target) {
            self.open_tabs.push(target.clone());
        }
        self.titles.insert(id, title);
        self.active = target;
        cx.emit(TabsChanged);
    }

    /// Activates an already-open tab; no-op if `target` is not currently open.
    pub fn activate(&mut self, target: TabTarget, cx: &mut Context<Self>) {
        if self.open_tabs.contains(&target) {
            self.active = target;
            cx.emit(TabsChanged);
        }
    }

    /// Closes the detail tab for `id`, falling back to the catalog tab if it
    /// was active. No-op if no such tab is open.
    pub fn close_detail_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        let before = self.open_tabs.len();
        self.open_tabs.retain(|t| match t {
                          TabTarget::Detail(open_id) => open_id.as_ref() != id,
                          TabTarget::Catalog => true,
                      });
        if self.open_tabs.len() != before {
            self.titles.remove(id);
            self.item_list_tables.remove(id);
            if matches!(&self.active, TabTarget::Detail(active_id) if active_id.as_ref() == id) {
                self.active = TabTarget::Catalog;
            }
            cx.emit(TabsChanged);
        }
    }
}

/// Returns the cached item list `TableState` entity for `entry_id`, creating
/// and caching one on first use.
///
/// On cache-miss creation, subscribes to the new entity's `TableEvent`s so
/// row selection drives `LibraryController::select_item_file`, and — if the
/// controller already has a selection recorded for this entry (e.g. the tab
/// was closed and reopened after a cache eviction) — restores the visual
/// selection once via `TableState::set_selected_row`.
pub(crate) fn item_list_table(tabs: &Entity<TabsController>,
                              controller: &Entity<LibraryController>, entry_id: &Arc<str>,
                              entry_dir: PathBuf, window: &mut Window, cx: &mut App)
                              -> Entity<TableState<ItemListDelegate>> {
    if let Some(table) = tabs.read(cx).item_list_tables.get(entry_id) {
        return table.clone();
    }

    let cols = item_list_columns();
    let col_count = cols.len();
    let delegate = ItemListDelegate { controller: controller.clone(),
                                      entry_id: Arc::clone(entry_id),
                                      columns: cols,
                                      user_widths: vec![None; col_count],
                                      entry_dir,
                                      table_width: None };
    let table = cx.new(|cx| {
                      TableState::new(delegate, window, cx).row_selectable(true)
                                                           .col_resizable(true)
                                                           .sortable(false)
                  });

    if let Some(selected_ix) = controller.read(cx).selected_item_file(entry_id) {
        table.update(cx, |state, cx| state.set_selected_row(selected_ix, cx));
    }

    cx.subscribe(&table, {
          let controller = controller.clone();
          let entry_id = Arc::clone(entry_id);
          move |_table, event: &TableEvent, cx| {
              if let TableEvent::SelectRow(row_ix) = event {
                  let row_ix = *row_ix;
                  let entry_id = Arc::clone(&entry_id);
                  controller.update(cx, |ctrl, cx| ctrl.select_item_file(entry_id, row_ix, cx));
              }
          }
      })
      .detach();

    tabs.update(cx, |t, _cx| {
            t.item_list_tables
             .insert(Arc::clone(entry_id), table.clone());
        });

    table
}

impl Default for TabsController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_controller_starts_with_only_catalog_tab_active() {
        let ctrl = TabsController::new();
        let snap = ctrl.snapshot();
        assert_eq!(snap.open_tabs, vec![TabTarget::Catalog]);
        assert!(matches!(snap.active, TabTarget::Catalog));
    }
}
