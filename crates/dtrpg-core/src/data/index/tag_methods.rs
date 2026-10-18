//! The tag methods of [`LibraryIndex`].

use std::collections::{HashMap, HashSet};

use crate::data::index::{IndexError, LibraryIndex, tag_sync, tags};
use crate::data::tag_color::TagColor;

impl LibraryIndex {
    /// Returns every tag that some item has, ordered ignoring case.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn list_tags(&self) -> Result<Vec<String>, IndexError> {
        tags::list_tags(&self.conn)
    }

    /// Returns the tags of the item `item_id`, ordered ignoring case.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn tags_for_item(&self, item_id: &str) -> Result<Vec<String>, IndexError> {
        tags::tags_for_item(&self.conn, item_id)
    }

    /// Returns the tags of every item that has any, keyed by item id.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn all_item_tags(&self) -> Result<HashMap<String, Vec<String>>, IndexError> {
        tags::all_item_tags(&self.conn)
    }

    /// Returns the color of every tag that has one, keyed by the tag's name
    /// lower cased.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn tag_colors(&self) -> Result<HashMap<String, TagColor>, IndexError> {
        tags::tag_colors(&self.conn)
    }

    /// Sets the color of the tag `tag`, or clears it with `None`; returns
    /// `false` if there is no such tag.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if a statement fails.
    pub fn set_tag_color(&self, tag: &str, color: Option<TagColor>) -> Result<bool, IndexError> {
        tags::set_tag_color(&self.conn, tag, color)
    }

    /// Returns when the item's tags last changed in the index, in Unix
    /// seconds, or `None` if they never have.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn tags_changed_at(&self, item_id: &str) -> Result<Option<i64>, IndexError> {
        tags::tags_changed_at(&self.conn, item_id)
    }

    /// Makes `tags` exactly the item's tags, as of `changed_at`. A tag already
    /// in the index keeps its spelling, and its color unless it has none.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::InvalidTag`] if a tag cannot be stored,
    /// [`IndexError::NoSuchItem`] if the item is not in the catalog, and
    /// [`IndexError::Sqlite`] if a statement fails; nothing is changed on
    /// error.
    pub fn set_item_tags(&mut self, item_id: &str, tags: &[(String, Option<TagColor>)],
                         changed_at: i64)
                         -> Result<(), IndexError> {
        tags::set_item_tags(&mut self.conn, item_id, tags, changed_at)
    }

    /// Returns the tags the file at `path` last agreed on with the index, or
    /// `None` if its tags have never been synced.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the query fails.
    pub fn file_tag_snapshot(&self, path: &str) -> Result<Option<Vec<String>>, IndexError> {
        tag_sync::snapshot(&self.conn, path)
    }

    /// Records that the file at `path`, a file of `item_id`, and the index
    /// agree on `tags` as of `synced_at`.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if the statement fails.
    pub fn record_file_tag_sync(&self, path: &str, item_id: &str, tags: &[String],
                                synced_at: i64)
                                -> Result<(), IndexError> {
        tag_sync::record(&self.conn, path, item_id, tags, synced_at)
    }

    /// Forgets the sync record of every file not in `present` and returns how
    /// many it forgot.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if a statement fails.
    pub fn forget_file_tag_syncs_except(&mut self, present: &HashSet<String>)
                                        -> Result<usize, IndexError> {
        tag_sync::forget_all_but(&mut self.conn, present)
    }

    /// Tags the item `item_id` with `tag`; returns `false` if it already had
    /// the tag. Tags are free text, unique ignoring case.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::InvalidTag`] if `tag` is empty, too long, or
    /// contains a comma or control character, [`IndexError::NoSuchItem`] if
    /// the item is not in the catalog, and [`IndexError::Sqlite`] if a write
    /// fails.
    pub fn add_tag(&mut self, item_id: &str, tag: &str) -> Result<bool, IndexError> {
        tags::add_tag(&mut self.conn, item_id, tag)
    }

    /// Removes `tag` from the item `item_id`; returns `false` if it did not
    /// have the tag.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if a write fails.
    pub fn remove_tag(&mut self, item_id: &str, tag: &str) -> Result<bool, IndexError> {
        tags::remove_tag(&mut self.conn, item_id, tag)
    }

    /// Renames the tag `from` to `to` on every item and returns how many items
    /// that was. If `to` already exists, the two tags are merged.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::InvalidTag`] if `to` cannot be stored, and
    /// [`IndexError::Sqlite`] if a write fails.
    pub fn rename_tag(&mut self, from: &str, to: &str) -> Result<usize, IndexError> {
        tags::rename_tag(&mut self.conn, from, to)
    }

    /// Removes the tag `tag` from every item and returns how many items had it.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError::Sqlite`] if a write fails.
    pub fn delete_tag(&mut self, tag: &str) -> Result<usize, IndexError> {
        tags::delete_tag(&mut self.conn, tag)
    }
}
