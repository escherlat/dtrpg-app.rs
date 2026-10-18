//! The colors a tag can have.
//!
//! These are the seven colors Finder gives tags, so a color survives a round
//! trip through a file's tags on macOS without being approximated. The UI picks
//! the shade of each from the theme.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A tag's color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TagColor {
    Gray,
    Green,
    Purple,
    Blue,
    Yellow,
    Red,
    Orange,
}

impl TagColor {
    /// Every color, in the order Finder lists them.
    pub const ALL: [Self; 7] = [Self::Gray,
                                Self::Green,
                                Self::Purple,
                                Self::Blue,
                                Self::Yellow,
                                Self::Red,
                                Self::Orange];

    /// The number Finder stores for this color after a tag's name.
    #[must_use]
    pub const fn finder_number(self) -> u8 {
        match self {
            Self::Gray => 1,
            Self::Green => 2,
            Self::Purple => 3,
            Self::Blue => 4,
            Self::Yellow => 5,
            Self::Red => 6,
            Self::Orange => 7,
        }
    }

    /// The color for a number Finder stored, or `None` for "no color" (0) and
    /// any number Finder does not use.
    #[must_use]
    pub fn from_finder_number(number: u8) -> Option<Self> {
        Self::ALL.into_iter()
                 .find(|color| color.finder_number() == number)
    }

    /// The lower-case name stored in the index and shown in text.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gray => "gray",
            Self::Green => "green",
            Self::Purple => "purple",
            Self::Blue => "blue",
            Self::Yellow => "yellow",
            Self::Red => "red",
            Self::Orange => "orange",
        }
    }
}

impl fmt::Display for TagColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A string that is not the name of a tag color.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a tag color")]
pub struct UnknownTagColor(pub String);

impl FromStr for TagColor {
    type Err = UnknownTagColor;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let wanted = text.trim().to_lowercase();
        Self::ALL.into_iter()
                 .find(|color| color.name() == wanted)
                 .ok_or_else(|| UnknownTagColor(text.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_color_round_trips_through_its_name_and_its_finder_number() {
        for color in TagColor::ALL {
            assert_eq!(color.to_string().parse::<TagColor>(), Ok(color));
            assert_eq!(TagColor::from_finder_number(color.finder_number()),
                       Some(color));
        }
    }

    #[test]
    fn finder_numbers_are_one_to_seven_in_order() {
        let numbers: Vec<u8> = TagColor::ALL.iter().map(|c| c.finder_number()).collect();

        assert_eq!(numbers, [1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn no_color_and_unknown_numbers_have_no_color() {
        assert_eq!(TagColor::from_finder_number(0), None);
        assert_eq!(TagColor::from_finder_number(8), None);
    }

    #[test]
    fn a_name_is_read_ignoring_case_and_spaces() {
        assert_eq!(" Red ".parse::<TagColor>(), Ok(TagColor::Red));
    }

    #[test]
    fn a_name_that_is_not_a_color_is_an_error() {
        assert_eq!("teal".parse::<TagColor>(),
                   Err(UnknownTagColor("teal".to_owned())));
    }
}
