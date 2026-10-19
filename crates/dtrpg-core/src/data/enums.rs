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

/// A column of the catalog's list layout.
///
/// [`Title`](Self::Title), [`Status`](Self::Status) and [`Open`](Self::Open)
/// are always shown. The user chooses which of [`CHOOSABLE`](Self::CHOOSABLE)
/// are.
///
/// # Examples
///
/// ```
/// use dtrpg_core::data::enums::ListColumn;
///
/// assert_eq!(ListColumn::from_key("tags"), Some(ListColumn::Tags));
/// assert_eq!(ListColumn::Pages.key(), "pages");
/// assert!(ListColumn::Size.is_sortable());
/// assert!(!ListColumn::Tags.is_sortable());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListColumn {
    Title,
    Publisher,
    System,
    Pages,
    Size,
    Added,
    Tags,
    /// The download-status glyph.
    Status,
    /// The button that opens a downloaded item.
    Open,
}

impl ListColumn {
    /// The columns the user can show or hide, in the order they appear.
    pub const CHOOSABLE: [Self; 6] = [Self::Publisher,
                                      Self::System,
                                      Self::Pages,
                                      Self::Size,
                                      Self::Added,
                                      Self::Tags];

    /// The choosable columns that are shown until the user chooses otherwise.
    pub const DEFAULT_CHOSEN: [Self; 5] = [Self::Publisher,
                                           Self::System,
                                           Self::Pages,
                                           Self::Size,
                                           Self::Added];

    /// The column's stable name, used to save it and as its sort key.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Publisher => "publisher",
            Self::System => "system",
            Self::Pages => "pages",
            Self::Size => "size",
            Self::Added => "added",
            Self::Tags => "tags",
            Self::Status => "status",
            Self::Open => "open",
        }
    }

    /// The column called `key`, if there is one.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        [Self::Title, Self::Status, Self::Open].into_iter()
                                               .chain(Self::CHOOSABLE)
                                               .find(|column| column.key() == key)
    }

    /// Whether clicking the column's heading sorts the list by it.
    #[must_use]
    pub fn is_sortable(self) -> bool {
        !matches!(self, Self::Tags | Self::Status | Self::Open)
    }

    /// The columns chosen after `column` is shown if it was hidden, or hidden
    /// if it was shown, in their usual order. A column that cannot be
    /// chosen changes nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use dtrpg_core::data::enums::ListColumn;
    ///
    /// let chosen = ListColumn::toggled(&[ListColumn::Size], ListColumn::Tags);
    /// assert_eq!(chosen, [ListColumn::Size, ListColumn::Tags]);
    /// assert_eq!(ListColumn::toggled(&chosen, ListColumn::Size),
    ///            [ListColumn::Tags]);
    /// assert_eq!(ListColumn::toggled(&chosen, ListColumn::Title), chosen);
    /// ```
    #[must_use]
    pub fn toggled(chosen: &[Self], column: Self) -> Vec<Self> {
        if !Self::CHOOSABLE.contains(&column) {
            return chosen.to_vec();
        }
        let mut next = chosen.to_vec();
        if let Some(at) = next.iter().position(|shown| *shown == column) {
            next.remove(at);
        }
        else {
            next.push(column);
        }
        Self::CHOOSABLE.into_iter()
                       .filter(|candidate| next.contains(candidate))
                       .collect()
    }

    /// Every column in the order they appear when `chosen` are shown: the
    /// title, then `chosen` in their usual order whatever order they are
    /// given in, then the status and open columns.
    #[must_use]
    pub fn shown(chosen: &[Self]) -> Vec<Self> {
        let mut columns = vec![Self::Title];
        columns.extend(Self::CHOOSABLE.into_iter()
                                      .filter(|column| chosen.contains(column)));
        columns.extend([Self::Status, Self::Open]);
        columns
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
