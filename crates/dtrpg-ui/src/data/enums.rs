//! Catalog enumerations: item status and presentation modes.

use serde::{Deserialize, Serialize};

// ── Enumerations ─────────────────────────────────────────────────────────────

/// Download state of a catalog item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemStatus {
    Downloaded,
    Cloud,
}

/// How the catalog is displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CatalogPresentation {
    List,
    Thumbs,
    #[default]
    Grid,
}

/// Which `LibraryItem` field a bulk-selection pattern match is applied
/// against (see `LibraryController::select_by_pattern`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionField {
    Title,
    Publisher,
    /// Game line / series (`LibraryItem::line`).
    System,
}
