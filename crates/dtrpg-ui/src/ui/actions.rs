//! App-level and window-level gpui actions for Libri.

use gpui::actions;
use schemars::JsonSchema;
use serde::Deserialize;

// App-level actions
actions!(libri,
         [Quit,
          HideApplication,
          HideOthers,
          ShowAll,
          ShowSettings,
          About,
          ReportFeatureBug,]);

// Window-level actions
actions!(libri, [Minimize, Zoom, ToggleFullscreen]);

// Catalog menu actions
actions!(libri,
         [AddCollection,
          ReloadCatalog,
          RefreshThumbnails,
          CheckItemAvailability]);

// Collection context menu actions
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, gpui::Action)]
pub struct ReloadCollection {
    /// The numeric id of the collection to reload.
    pub id: u64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, gpui::Action)]
pub struct DeleteCollection {
    /// The numeric id of the collection to delete.
    pub id: u64,
}

// Window menu actions
actions!(libri, [ShowActivity, ShowAlertHistory]);

// Thin no-op action types for OS text-editing routing via OsAction
actions!(libri, [Undo, Redo, Cut, Copy, Paste, SelectAll]);

// View menu actions: catalog presentation mode, sort, and search focus.
actions!(libri,
         [ViewAsList,
          ViewAsThumbs,
          ViewAsGrid,
          SortByTitle,
          SortByPublisher,
          SortByDateAdded,
          SortByPages,
          SortAscending,
          SortDescending,
          ToggleGroupByPublisher,
          FocusSearch,]);

// Window menu tab-selection actions
actions!(libri,
         [SelectTab0, SelectTab1, SelectTab2, SelectTab3, SelectTab4, SelectTab5, SelectTab6,
          SelectTab7, SelectTab8, SelectTab9,]);
