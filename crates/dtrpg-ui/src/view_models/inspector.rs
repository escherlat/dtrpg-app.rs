//! UI-facing state for the v2 item inspector.

use std::sync::Arc;

use dtrpg_core::data::enums::ItemStatus;
use dtrpg_core::data::library::{LibraryItem, LibraryItemFile};
use dtrpg_core::util::container::ContainerEntry;

/// Inspector sections shown by the v2 detail surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorTab {
    #[default]
    Overview,
    Files,
    Collections,
    Contents,
    Summary,
}

/// State of the lazily loaded archive contents tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorContentsState {
    Unavailable,
    Loading,
    Loaded(Vec<ContainerEntry>),
}

/// Presentation data for one file in the inspector.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectorFile {
    pub name: Arc<str>,
    pub format: Arc<str>,
    pub size_mb: f64,
    pub downloaded: bool,
    pub offloaded: bool,
}

impl From<&LibraryItemFile> for InspectorFile {
    fn from(file: &LibraryItemFile) -> Self {
        Self {
            name: Arc::clone(&file.name),
            format: Arc::clone(&file.format),
            size_mb: file.size_mb,
            downloaded: file.downloaded,
            offloaded: file.offloaded,
        }
    }
}

/// Immutable item data consumed by inspector views.
///
/// The snapshot deliberately contains no SDK service, GPUI entity, or
/// filesystem handle. Expensive local data and archive operations can be
/// loaded by commands without coupling them to rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectorSnapshot {
    pub item_id: Arc<str>,
    pub title: Arc<str>,
    pub publisher: Arc<str>,
    pub line: Arc<str>,
    pub description: Arc<str>,
    pub status: ItemStatus,
    pub is_available: bool,
    pub files: Vec<InspectorFile>,
}

impl From<&LibraryItem> for InspectorSnapshot {
    fn from(item: &LibraryItem) -> Self {
        Self {
            item_id: Arc::clone(&item.id),
            title: Arc::clone(&item.title),
            publisher: Arc::clone(&item.publisher),
            line: Arc::clone(&item.line),
            description: Arc::clone(&item.desc),
            status: item.status,
            is_available: item.is_available,
            files: item.files.iter().map(InspectorFile::from).collect(),
        }
    }
}

/// View-model state shared by inspector surfaces.
#[derive(Debug, Clone, PartialEq)]
pub struct InspectorViewModel {
    snapshot: InspectorSnapshot,
    active_tab: InspectorTab,
}

impl InspectorViewModel {
    /// Creates an inspector from a catalog item with the Overview tab active.
    #[must_use]
    pub fn new(item: &LibraryItem) -> Self {
        Self {
            snapshot: InspectorSnapshot::from(item),
            active_tab: InspectorTab::default(),
        }
    }

    /// Returns the immutable data rendered by the active inspector surface.
    #[must_use]
    pub fn snapshot(&self) -> &InspectorSnapshot {
        &self.snapshot
    }

    /// Returns the active inspector tab.
    #[must_use]
    pub fn active_tab(&self) -> InspectorTab {
        self.active_tab
    }

    /// Changes the active inspector tab without performing I/O.
    pub fn set_active_tab(&mut self, tab: InspectorTab) {
        self.active_tab = tab;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtrpg_core::data::enums::ItemStatus;

    #[test]
    fn snapshot_preserves_item_and_file_presentation_data() {
        let mut item = LibraryItem::new(
            "item-1",
            "The Manual",
            "Publisher",
            "Core",
            "Rulebook",
            "PDF",
            120,
            12.5,
            2026,
            1,
            ItemStatus::Cloud,
            "#123456",
            "Description",
            None,
        );
        item.is_available = false;
        item.files.push(LibraryItemFile {
            id: "file-1".into(),
            index: 0,
            name: "manual.pdf".into(),
            format: "PDF".into(),
            size_mb: 12.5,
            downloaded: false,
            offloaded: false,
        });

        let snapshot = InspectorSnapshot::from(&item);

        assert_eq!(snapshot.item_id.as_ref(), "item-1");
        assert!(!snapshot.is_available);
        assert_eq!(snapshot.files[0].name.as_ref(), "manual.pdf");
        assert!(!snapshot.files[0].downloaded);
    }

    #[test]
    fn view_model_defaults_to_overview_and_changes_tabs_without_io() {
        let item = LibraryItem::new(
            "item-1",
            "Title",
            "Publisher",
            "",
            "",
            "PDF",
            0,
            0.0,
            2026,
            0,
            ItemStatus::Cloud,
            "#000000",
            "",
            None,
        );
        let mut model = InspectorViewModel::new(&item);

        assert_eq!(model.active_tab(), InspectorTab::Overview);
        model.set_active_tab(InspectorTab::Files);
        assert_eq!(model.active_tab(), InspectorTab::Files);
    }
}
