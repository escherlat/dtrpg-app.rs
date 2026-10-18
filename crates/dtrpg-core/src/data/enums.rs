//! Catalog enumerations: item status and presentation modes.

use serde::{Deserialize, Serialize};

// ── Enumerations ─────────────────────────────────────────────────────────────

/// Download state of a catalog item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemStatus {
    Downloaded,
    Cloud,
}

/// What the app is busy doing with a catalog entry right now, shown by the
/// entry's status glyph in place of its downloaded/cloud state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemActivity {
    /// Nothing in progress; show the entry's [`ItemStatus`].
    #[default]
    Idle,
    /// Its files are being checked against what is on disk.
    Verifying,
    /// One or more of its files are queued or downloading.
    Downloading,
}

impl ItemActivity {
    /// The activity to show for an entry given what is in progress for it. A
    /// download wins over a disk check, since it is the more useful thing to
    /// show.
    #[must_use]
    pub fn of(is_downloading: bool, is_verifying: bool) -> Self {
        if is_downloading {
            Self::Downloading
        }
        else if is_verifying {
            Self::Verifying
        }
        else {
            Self::Idle
        }
    }
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
