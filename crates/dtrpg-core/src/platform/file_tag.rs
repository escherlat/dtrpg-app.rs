//! A tag as a file holds it.

use crate::data::tag_color::TagColor;

/// A tag on a file: its name, and the color the platform keeps for it, if it
/// keeps colors and the tag has one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileTag {
    /// The tag's name.
    pub name:  String,
    /// The tag's color. Only macOS keeps one.
    pub color: Option<TagColor>,
}

impl FileTag {
    /// A tag with a name and no color.
    #[must_use]
    pub fn plain(name: impl Into<String>) -> Self {
        Self { name:  name.into(),
               color: None, }
    }

    /// A tag with a name and a color.
    #[must_use]
    pub fn colored(name: impl Into<String>, color: TagColor) -> Self {
        Self { name:  name.into(),
               color: Some(color), }
    }
}
