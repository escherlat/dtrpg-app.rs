//! Catalog enumerations: item status and presentation modes.

use std::fmt;
use std::str::FromStr;

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

/// What the catalog is grouped by.
///
/// # Examples
///
/// ```
/// use dtrpg_core::data::enums::GroupBy;
///
/// assert_eq!("Tag".parse::<GroupBy>(), Ok(GroupBy::Tag));
/// assert_eq!(GroupBy::Publisher.to_string(), "Publisher");
/// assert!(!GroupBy::None.is_grouped());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupBy {
    /// One list, with no groups.
    #[default]
    None,
    /// One group per publisher.
    Publisher,
    /// One group per tag, with an item in the group of each tag it has.
    Tag,
}

impl GroupBy {
    /// Whether the catalog is split into groups.
    #[must_use]
    pub fn is_grouped(self) -> bool {
        self != Self::None
    }

    /// The grouping after a menu item for `wanted` is chosen: `wanted`, or no
    /// grouping if that is what the catalog is grouped by already.
    ///
    /// # Examples
    ///
    /// ```
    /// use dtrpg_core::data::enums::GroupBy;
    ///
    /// assert_eq!(GroupBy::None.toggled(GroupBy::Tag), GroupBy::Tag);
    /// assert_eq!(GroupBy::Tag.toggled(GroupBy::Tag), GroupBy::None);
    /// assert_eq!(GroupBy::Publisher.toggled(GroupBy::Tag), GroupBy::Tag);
    /// ```
    #[must_use]
    pub fn toggled(self, wanted: Self) -> Self {
        if self == wanted { Self::None } else { wanted }
    }
}

impl fmt::Display for GroupBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
                        Self::None => "None",
                        Self::Publisher => "Publisher",
                        Self::Tag => "Tag",
                    })
    }
}

/// A string that is not the name of a grouping.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a grouping")]
pub struct UnknownGroupBy(pub String);

impl FromStr for GroupBy {
    type Err = UnknownGroupBy;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "None" => Ok(Self::None),
            "Publisher" => Ok(Self::Publisher),
            "Tag" => Ok(Self::Tag),
            other => Err(UnknownGroupBy(other.to_owned())),
        }
    }
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
