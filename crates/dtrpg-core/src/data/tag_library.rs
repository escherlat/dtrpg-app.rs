//! The library's tags as the sidebar and the detail panel show them.
//!
//! Reading the index on every render is not allowed, so the tags are read once
//! into a [`TagLibrary`], which the UI keeps and asks questions of.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::data::index::{IndexError, LibraryIndex};
use crate::data::tag_color::TagColor;

/// A tag, with its color and how many items have it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagEntry {
    /// The tag's name, spelled as the index first stored it.
    pub name:  Arc<str>,
    /// The tag's color, if it has one.
    pub color: Option<TagColor>,
    /// How many items in the catalog have the tag.
    pub count: usize,
}

/// Every tag in the library, and which items have which.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TagLibrary {
    /// Each item's tag names, ordered ignoring case, keyed by item id.
    by_item: HashMap<Arc<str>, Vec<Arc<str>>>,
    /// The color of each tag that has one, keyed by the tag's name lower cased.
    colors:  HashMap<String, TagColor>,
    /// Every tag, ordered ignoring case.
    entries: Vec<TagEntry>,
}

impl TagLibrary {
    /// Builds the library from each item's tags and the colors of the tags that
    /// have one (keyed by the tag's name lower cased).
    #[must_use]
    pub fn new(tags_by_item: HashMap<String, Vec<String>>, colors: HashMap<String, TagColor>)
               -> Self {
        let mut counts: HashMap<String, (Arc<str>, usize)> = HashMap::new();
        let mut by_item = HashMap::new();
        for (item_id, names) in tags_by_item {
            let names: Vec<Arc<str>> = names.into_iter().map(Arc::from).collect();
            for name in &names {
                counts.entry(name.to_lowercase())
                      .or_insert_with(|| (Arc::clone(name), 0))
                      .1 += 1;
            }
            by_item.insert(Arc::from(item_id), names);
        }
        let mut entries: Vec<TagEntry> =
            counts.into_iter()
                  .map(|(key, (name, count))| TagEntry { color: colors.get(&key).copied(),
                                                         name,
                                                         count })
                  .collect();
        entries.sort_by_key(|entry| entry.name.to_lowercase());
        Self { by_item,
               colors,
               entries }
    }

    /// Reads every item's tags and every tag's color from `index`.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be read.
    pub fn load(index: &LibraryIndex) -> Result<Self, IndexError> {
        Ok(Self::new(index.all_item_tags()?, index.tag_colors()?))
    }

    /// Every tag, ordered ignoring case.
    #[must_use]
    pub fn entries(&self) -> &[TagEntry] {
        &self.entries
    }

    /// Whether no item has any tag.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The tags of the item `item_id`, ordered ignoring case.
    #[must_use]
    pub fn tags_of(&self, item_id: &str) -> Vec<TagEntry> {
        self.by_item
            .get(item_id)
            .map(|names| {
                names.iter()
                     .filter_map(|name| self.entry(name))
                     .cloned()
                     .collect()
            })
            .unwrap_or_default()
    }

    /// The names of the tags of the item `item_id`, ordered ignoring case,
    /// without copying them.
    #[must_use]
    pub fn tag_names_of(&self, item_id: &str) -> &[Arc<str>] {
        self.by_item.get(item_id).map_or(&[], Vec::as_slice)
    }

    /// The tag called `name`, ignoring case, if any item has it.
    #[must_use]
    pub fn entry(&self, name: &str) -> Option<&TagEntry> {
        let wanted = name.to_lowercase();
        self.entries
            .iter()
            .find(|entry| entry.name.to_lowercase() == wanted)
    }

    /// The color of the tag called `name`, ignoring case.
    #[must_use]
    pub fn color_of(&self, name: &str) -> Option<TagColor> {
        self.colors.get(&name.to_lowercase()).copied()
    }

    /// The ids of the items that have the tag called `name`, ignoring case.
    #[must_use]
    pub fn items_with(&self, name: &str) -> HashSet<Arc<str>> {
        let wanted = name.to_lowercase();
        self.by_item
            .iter()
            .filter(|(_, names)| names.iter().any(|tag| tag.to_lowercase() == wanted))
            .map(|(item_id, _)| Arc::clone(item_id))
            .collect()
    }
}

#[cfg(test)]
mod tests;
