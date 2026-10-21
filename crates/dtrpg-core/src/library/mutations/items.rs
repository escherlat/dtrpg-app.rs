//! `Library` methods that change a catalog entry and write it to the index.

use std::sync::PoisonError;
use std::time::SystemTime;

use super::check_result::apply_check_result;
use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;
use crate::library::Library;
use crate::services::LibraryServiceError;
use crate::services::tag_editing::TagEdit;
use crate::util::file_presence::{ItemVerification, apply_presence_changes};

impl Library {
    /// Writes `item` to the index. A failure is logged, not returned: the
    /// in-memory result already stands, and the next catalog sync repairs the
    /// index.
    fn persist_item(&self, item: &LibraryItem, what: &str) {
        let written = self.index
                          .lock()
                          .unwrap_or_else(PoisonError::into_inner)
                          .replace_item(item);
        if let Err(error) = written {
            tracing::error!(%error, "failed to write {what} to the library index");
        }
    }

    /// Applies disk-check results to the catalog and writes each changed
    /// entry to the index. Returns whether anything changed.
    ///
    /// Each result carries only the files that differed when the check read
    /// the disk. Applying that, rather than swapping in the checked copy of
    /// the entry, leaves everything else about the live entry as it is now,
    /// so a download that completed after the check started is not undone.
    pub fn apply_presence_verifications(&self, verifications: &[ItemVerification]) -> bool {
        let mut changed = false;
        for verification in verifications {
            if let Some(mut item) = self.item_by_id(&verification.item_id)
               && apply_presence_changes(&mut item, &verification.changes)
            {
                changed = true;
                self.persist_item(&item, "item presence change");
            }
        }
        changed
    }

    /// Applies the result of an availability check to the entry `item_id` and
    /// writes it to the index. Returns whether the entry exists.
    ///
    /// See [`apply_check_result`] for what each kind of result changes.
    pub fn apply_availability_result(&self, item_id: &str,
                                     result: Result<LibraryItem, LibraryServiceError>)
                                     -> bool {
        let Some(mut item) = self.item_by_id(item_id)
        else {
            return false;
        };
        apply_check_result(&mut item, result, SystemTime::now());
        // `apply_check_result` leaves `availability_last_checked` unchanged on
        // a transient error, which keeps whatever the overlay in `item_by_id`
        // already set it to, so only record it again when it is there.
        if let Some(checked_at) = item.availability_last_checked {
            self.note_availability_check(&item.id, checked_at);
        }
        self.persist_item(&item, "availability check result");
        true
    }

    /// Records the end of a download of file `file_index` of entry `item_id`
    /// and writes the entry to the index straight away, so that a restart
    /// before the next catalog sync does not show it as `Cloud`.
    ///
    /// A successful download marks the file downloaded; a failed one leaves
    /// the flags alone. Either way the entry's status is derived again.
    /// Returns the entry as it now is, or `None` if it is no longer in the
    /// catalog.
    pub fn record_download_result(&self, item_id: &str, file_index: u32, succeeded: bool)
                                  -> Option<LibraryItem> {
        let mut item = self.item_by_id(item_id)?;
        if succeeded && let Some(file) = item.files.get_mut(file_index as usize) {
            file.downloaded = true;
            file.offloaded = false;
        }
        item.recompute_status();
        self.persist_item(&item, "download status");
        Some(item)
    }

    /// Reverts a `Downloaded` entry to `Cloud` by clearing every file's flags,
    /// and writes it to the index. Returns whether the entry changed.
    ///
    /// Entry status is never set directly (see
    /// `LibraryItem::recompute_status`): the flags are cleared first and
    /// the status is derived from them.
    pub fn clear_item_downloads(&self, item_id: &str) -> bool {
        let Some(mut item) = self.item_by_id(item_id)
        else {
            return false;
        };
        if item.status != ItemStatus::Downloaded {
            return false;
        }
        for file in &mut item.files {
            file.downloaded = false;
            file.offloaded = false;
        }
        item.recompute_status();
        self.persist_item(&item, "item status");
        true
    }

    /// Removes every entry from the catalog index.
    pub fn clear_catalog(&self) {
        let cleared = self.index
                          .lock()
                          .unwrap_or_else(PoisonError::into_inner)
                          .replace_all_items(&[]);
        if let Err(error) = cleared {
            tracing::error!(%error, "failed to clear the library index");
        }
    }

    /// The entries a tag edit will change, so that the files that need their
    /// tags written are known without copying the whole catalog.
    #[must_use]
    pub fn items_affected_by_tag_edit(&self, edit: &TagEdit) -> Vec<LibraryItem> {
        match edit {
            TagEdit::Add { item_id, .. } | TagEdit::Remove { item_id, .. } => {
                self.item_by_id(item_id).into_iter().collect()
            }
            TagEdit::SetColor { tag, .. } => {
                let having = self.tags().items_with(tag);
                self.all_items()
                    .into_iter()
                    .filter(|item| having.contains(&item.id))
                    .collect()
            }
        }
    }
}
