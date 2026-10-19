//! Tags in the library controller: the copy the views read, loading it, editing
//! it, and filtering the catalog by a tag.
//!
//! The library index owns tags. The views must not read it, so the controller
//! keeps a [`TagLibrary`] read from it in the background, replaced whenever the
//! tags change.

use std::sync::Arc;

use dtrpg_core::data::index::{IndexError, LibraryIndex};
use dtrpg_core::data::native_preferences::NativePreferences;
use dtrpg_core::data::tag_color::TagColor;
use dtrpg_core::data::tag_library::{TagEntry, TagLibrary};
use dtrpg_core::platform::Platform;
use dtrpg_core::services::tag_editing::{TagEdit, TagEditOutcome, apply_tag_edit};
use dtrpg_core::services::tag_projection::TaggedFile;

use super::file_tags::{downloaded_files, sync_settings};
use super::*;

impl LibraryController {
    /// Every tag, with its color and item count, ordered ignoring case, for the
    /// sidebar.
    #[must_use]
    pub fn tag_entries(&self) -> Vec<TagEntry> {
        self.tags.entries().to_vec()
    }

    /// The tags of the item `item_id`, for the detail panel.
    #[must_use]
    pub fn tags_of(&self, item_id: &str) -> Vec<TagEntry> {
        self.tags.tags_of(item_id)
    }

    /// The tags to offer when tagging `item_id`: every tag it does not have.
    #[must_use]
    pub fn tags_to_offer(&self, item_id: &str) -> Vec<TagEntry> {
        let has = self.tags.tags_of(item_id);
        self.tags
            .entries()
            .iter()
            .filter(|entry| !has.iter().any(|tag| tag.name == entry.name))
            .cloned()
            .collect()
    }

    /// Reads the tags from the library index in the background and shows them.
    pub(super) fn load_tags(&mut self, cx: &mut Context<Self>) {
        self.tag_generation += 1;
        let generation = self.tag_generation;
        let cache_root = self.cache_root.clone();
        cx.spawn(async move |this, async_cx| {
              let loaded = async_cx.background_executor()
                                   .spawn(async move { read_tag_library(&cache_root) })
                                   .await;
              this.update(async_cx, |ctrl, cx| match loaded {
                      Ok(library) => ctrl.show_tags(library, generation, cx),
                      Err(error) => {
                          tracing::warn!(%error, "could not read tags from the library index");
                      }
                  })
                  .ok();
          })
          .detach();
    }

    /// Shows `library`, unless something newer has been asked for since
    /// `generation`, and keeps a tag filter on what it now holds.
    pub(super) fn show_tags(&mut self, library: TagLibrary, generation: u64,
                            cx: &mut Context<Self>) {
        if generation < self.tag_generation {
            return;
        }
        self.tags = Arc::new(library);
        self.refresh_tag_filter();
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }

    /// Rebuilds an active tag filter from the tags as they are now, or drops it
    /// if no item has the tag any more.
    fn refresh_tag_filter(&mut self) {
        let SidebarFilter::Tag { name, .. } = &self.filter
        else {
            return;
        };
        self.filter = match self.tags.entry(name) {
            Some(entry) => tag_filter(&entry.name, &self.tags),
            None => SidebarFilter::AllTitles,
        };
    }

    /// Shows only the items that have the tag called `name`.
    pub fn filter_by_tag(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.tags.entry(name)
        else {
            return;
        };
        let filter = tag_filter(&entry.name, &self.tags);
        self.set_filter(filter, cx);
    }

    /// Gives the item the tag called `name`.
    pub fn add_tag(&mut self, item_id: &str, name: &str, cx: &mut Context<Self>) {
        self.edit_tags(TagEdit::Add { item_id: item_id.to_owned(),
                                      tag:     name.to_owned(), },
                       cx);
    }

    /// Takes the tag called `name` off the item.
    pub fn remove_tag(&mut self, item_id: &str, name: &str, cx: &mut Context<Self>) {
        self.edit_tags(TagEdit::Remove { item_id: item_id.to_owned(),
                                         tag:     name.to_owned(), },
                       cx);
    }

    /// Sets the color of the tag called `name`, or clears it with `None`.
    pub fn set_tag_color(&mut self, name: &str, color: Option<TagColor>, cx: &mut Context<Self>) {
        self.edit_tags(TagEdit::SetColor { tag: name.to_owned(),
                                           color },
                       cx);
    }

    /// The items an edit will change, so the files that need their tags written
    /// are known without copying the whole catalog.
    fn items_affected_by(&self, edit: &TagEdit) -> Vec<LibraryItem> {
        match edit {
            TagEdit::Add { item_id, .. } | TagEdit::Remove { item_id, .. } => {
                self.catalog
                    .iter()
                    .filter(|item| &*item.id == item_id.as_str())
                    .cloned()
                    .collect()
            }
            TagEdit::SetColor { tag, .. } => {
                let having = self.tags.items_with(tag);
                self.catalog
                    .iter()
                    .filter(|item| having.contains(&item.id))
                    .cloned()
                    .collect()
            }
        }
    }

    /// Makes an edit in the index in the background and writes the result to
    /// the files it affected. A failure is reported in the activity panel.
    ///
    /// The new tags are shown by reading them from the index once the edit is
    /// done, not from the edit, so that edits that overlap show each other's
    /// result: the last read starts after the last edit has finished.
    fn edit_tags(&mut self, edit: TagEdit, cx: &mut Context<Self>) {
        let affected = self.items_affected_by(&edit);
        let collections = self.collections.clone();
        let cache_root = self.cache_root.clone();
        cx.spawn(async move |this, async_cx| {
              let outcome =
                  async_cx.background_executor()
                          .spawn(async move {
                              let files: Vec<TaggedFile> =
                                  downloaded_files(&affected, &collections, &StorageConfig::load());
                              let settings = sync_settings(&NativePreferences::load());
                              apply_tag_edit(&Platform, &cache_root, &edit, &files, settings)
                          })
                          .await;
              this.update(async_cx, |ctrl, cx| ctrl.finish_tag_edit(outcome, cx))
                  .ok();
          })
          .detach();
    }

    /// Reports what went wrong with an edit, then shows the tags the index now
    /// holds, which is the old tags if the edit failed.
    pub(super) fn finish_tag_edit(&mut self, outcome: Result<TagEditOutcome, IndexError>,
                                  cx: &mut Context<Self>) {
        match outcome {
            Ok(outcome) => {
                for failure in &outcome.failures {
                    tracing::warn!(path = %failure.path.display(),
                                   error = %failure.error,
                                   "could not give a file its tags");
                }
                if !outcome.failures.is_empty() {
                    let label = t!("activity.tags_restore_failed",
                                   count = outcome.failures.len()).to_string();
                    self.fail_note(&label, &label, cx);
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not change tags");
                let label = t!("activity.tag_edit_failed").to_string();
                self.fail_note(&label, &error.to_string(), cx);
            }
        }
        self.load_tags(cx);
    }

    /// Adds a failed entry to the activity panel.
    fn fail_note(&self, label: &str, message: &str, cx: &mut Context<Self>) {
        self.activity.update(cx, |activity, cx| {
                         let id = activity.start(label, None, cx);
                         activity.error(id, message.to_owned(), cx);
                     });
    }
}

/// A filter to the items that have the tag called `name`.
fn tag_filter(name: &Arc<str>, library: &TagLibrary) -> SidebarFilter {
    SidebarFilter::Tag { name:  Arc::clone(name),
                         items: Arc::new(library.items_with(name)), }
}

/// Reads the library's tags from the index under `index_root`.
fn read_tag_library(index_root: &Path) -> Result<TagLibrary, IndexError> {
    TagLibrary::load(&LibraryIndex::open_and_migrate(index_root)?)
}
