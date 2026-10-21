//! The library's collections: the list the sidebar shows, whether the first
//! fetch of it has finished, and the edits made to it.

use std::collections::HashSet;
use std::sync::{Arc, PoisonError};

use super::collection_members::{
    RemovedMember, add_member, remove_member, restore_removed_member, revert_added_member,
};
use crate::data::collection::CollectionEntry;
use crate::library::Library;
use crate::util::sort::{CollectionSortMethod, SortDirection, sort_collections};

/// The collections list and whether it has been fetched this session.
#[derive(Default)]
pub(in crate::library) struct CollectionsState {
    /// Shared so that a reader takes a cheap `Arc` clone; an edit while a
    /// reader holds one copies the list first.
    entries: Arc<Vec<CollectionEntry>>,
    /// True once the first fetch this session has finished. The sidebar shows
    /// "?" for the count until then, rather than a misleading `0`.
    loaded:  bool,
}

impl Library {
    /// Runs `edit` on the collections list under its lock.
    fn edit_collections<T>(&self, edit: impl FnOnce(&mut Vec<CollectionEntry>) -> T) -> T {
        let mut state = self.collections
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner);
        edit(Arc::make_mut(&mut state.entries))
    }

    /// The collections as they are now.
    #[must_use]
    pub fn collections(&self) -> Arc<Vec<CollectionEntry>> {
        Arc::clone(&self.collections
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .entries)
    }

    /// Whether the collections have been fetched this session.
    #[must_use]
    pub fn collections_loaded(&self) -> bool {
        self.collections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .loaded
    }

    /// Replaces the collections with a fetched list, in the order given, and
    /// marks them loaded.
    pub fn set_collections(&self, entries: Vec<CollectionEntry>) {
        let mut state = self.collections
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner);
        state.entries = Arc::new(entries);
        state.loaded = true;
    }

    /// Empties the collections and marks them not loaded, as when signing in
    /// again.
    pub fn clear_collections(&self) {
        *self.collections
             .lock()
             .unwrap_or_else(PoisonError::into_inner) = CollectionsState::default();
    }

    /// Orders the collections for the sidebar. `catalog_ids` are the product
    /// ids in the catalog, which the count-based orders use.
    pub fn sort_collections(&self, method: CollectionSortMethod, direction: SortDirection,
                            catalog_ids: &HashSet<u64>) {
        self.edit_collections(|entries| sort_collections(entries, method, direction, catalog_ids));
    }

    /// Adds a collection that was just created.
    pub fn push_collection(&self, entry: CollectionEntry) {
        self.edit_collections(|entries| entries.push(entry));
    }

    /// Drops the collection `id`.
    pub fn remove_collection(&self, id: u64) {
        self.edit_collections(|entries| entries.retain(|c| c.id != id));
    }

    /// The ids of the members of the collection `id`, or none if there is no
    /// such collection.
    #[must_use]
    pub fn collection_member_ids(&self, id: u64) -> HashSet<u64> {
        self.collections()
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.member_ids.iter().copied().collect())
            .unwrap_or_default()
    }

    /// See [`add_member`].
    pub fn add_collection_member(&self, collection_id: u64, item_id: u64, product_id: u64)
                                 -> Option<Arc<str>> {
        self.edit_collections(|entries| add_member(entries, collection_id, item_id, product_id))
    }

    /// See [`revert_added_member`].
    pub fn revert_added_collection_member(&self, collection_id: u64, item_id: u64) {
        self.edit_collections(|entries| revert_added_member(entries, collection_id, item_id));
    }

    /// See [`remove_member`].
    pub fn remove_collection_member(&self, collection_id: u64, item_id: u64, product_id: u64)
                                    -> Option<RemovedMember> {
        self.edit_collections(|entries| remove_member(entries, collection_id, item_id, product_id))
    }

    /// See [`restore_removed_member`].
    pub fn restore_removed_collection_member(&self, collection_id: u64, removed_ids: &[u64]) {
        self.edit_collections(|entries| {
                restore_removed_member(entries, collection_id, removed_ids);
            });
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::data::storage::StorageConfig;

    fn open_library() -> Library {
        let dir = tempfile::tempdir().expect("tempdir");
        Library::open(dir.keep(), &StorageConfig::for_test(std::env::temp_dir())).expect("open")
                                                                                 .0
    }

    fn collection(id: u64, member_ids: &[u64]) -> CollectionEntry {
        CollectionEntry { id,
                          name: format!("Collection {id}").into(),
                          member_ids: member_ids.into(),
                          created_at: 0 }
    }

    #[test]
    fn collections_are_not_loaded_until_set() {
        let library = open_library();
        assert!(!library.collections_loaded());

        library.set_collections(vec![collection(1, &[])]);
        assert!(library.collections_loaded());
        assert_eq!(library.collections().len(), 1);

        library.clear_collections();
        assert!(!library.collections_loaded());
        assert!(library.collections().is_empty());
    }

    #[test]
    fn a_reader_keeps_the_list_it_took_while_an_edit_is_made() {
        let library = open_library();
        library.set_collections(vec![collection(1, &[])]);
        let before = library.collections();

        library.add_collection_member(1, 10, 20);

        assert!(before[0].member_ids.is_empty());
        assert_eq!(&*library.collections()[0].member_ids, &[10]);
    }

    #[test]
    fn member_ids_follow_the_edits() {
        let library = open_library();
        library.set_collections(vec![collection(1, &[])]);

        library.add_collection_member(1, 10, 20);
        assert_eq!(library.collection_member_ids(1), HashSet::from([10]));

        let removed = library.remove_collection_member(1, 10, 20)
                             .expect("a member");
        assert!(library.collection_member_ids(1).is_empty());

        library.restore_removed_collection_member(1, &removed.removed_ids);
        assert_eq!(library.collection_member_ids(1), HashSet::from([10]));
    }

    #[test]
    fn push_and_remove_change_the_list() {
        let library = open_library();
        library.push_collection(collection(1, &[]));
        library.push_collection(collection(2, &[]));

        library.remove_collection(1);

        let ids: Vec<u64> = library.collections().iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![2]);
    }
}
