//! Optimistic collection-membership edits on a list of collections.
//!
//! The UI shows a membership change at once, then confirms it with the
//! collections service. These functions make the local change and its
//! rollback, so the rules for which ids are touched can be tested without a
//! UI. Each takes the collection list by slice and changes only the collection
//! it is asked about.

use std::sync::Arc;

use crate::data::collection::CollectionEntry;
use crate::util::matching::member_ids_contain;

/// What [`remove_member`] took out of a collection, and what it needs to put
/// it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedMember {
    /// The collection's name, for reporting a failure.
    pub collection_name: Arc<str>,
    /// The ids that were in the collection and were removed. A rollback
    /// restores exactly these, rather than introducing an id that was never
    /// there.
    pub removed_ids:     Vec<u64>,
}

/// Adds `item_id` to the collection `collection_id`.
///
/// Returns the collection's name if the item was added, or `None` if the
/// collection is not in the list or already has the item under either of its
/// ids.
///
/// # Examples
///
/// ```
/// use dtrpg_core::data::collection::CollectionEntry;
/// use dtrpg_core::library::add_member;
///
/// let mut collections = vec![CollectionEntry { id:         1,
///                                              name:       "Favorites".into(),
///                                              member_ids: [].into(),
///                                              created_at: 0, }];
/// assert!(add_member(&mut collections, 1, 10, 20).is_some());
/// assert_eq!(&*collections[0].member_ids, &[10]);
/// assert!(add_member(&mut collections, 1, 10, 20).is_none());
/// ```
pub fn add_member(collections: &mut [CollectionEntry], collection_id: u64, item_id: u64,
                  product_id: u64)
                  -> Option<Arc<str>> {
    let collection = collections.iter_mut().find(|c| c.id == collection_id)?;
    if member_ids_contain(&collection.member_ids, item_id, product_id) {
        return None;
    }
    let mut member_ids: Vec<u64> = collection.member_ids.to_vec();
    member_ids.push(item_id);
    collection.member_ids = Arc::from(member_ids);
    Some(Arc::clone(&collection.name))
}

/// Undoes [`add_member`] after the service refused it.
pub fn revert_added_member(collections: &mut [CollectionEntry], collection_id: u64, item_id: u64) {
    if let Some(collection) = collections.iter_mut().find(|c| c.id == collection_id) {
        let member_ids: Vec<u64> = collection.member_ids
                                             .iter()
                                             .copied()
                                             .filter(|id| *id != item_id)
                                             .collect();
        collection.member_ids = Arc::from(member_ids);
    }
}

/// Removes `item_id` from the collection `collection_id`, under whichever of
/// `item_id` and `product_id` it is held.
///
/// Returns what was removed, or `None` if the collection is not in the list or
/// does not have the item.
///
/// # Examples
///
/// ```
/// use dtrpg_core::data::collection::CollectionEntry;
/// use dtrpg_core::library::{remove_member, restore_removed_member};
///
/// let mut collections = vec![CollectionEntry { id:         1,
///                                              name:       "Favorites".into(),
///                                              member_ids: [20, 30].into(),
///                                              created_at: 0, }];
/// let removed = remove_member(&mut collections, 1, 10, 20).unwrap();
/// assert_eq!(removed.removed_ids, vec![20]);
/// assert_eq!(&*collections[0].member_ids, &[30]);
///
/// restore_removed_member(&mut collections, 1, &removed.removed_ids);
/// assert_eq!(&*collections[0].member_ids, &[30, 20]);
/// ```
pub fn remove_member(collections: &mut [CollectionEntry], collection_id: u64, item_id: u64,
                     product_id: u64)
                     -> Option<RemovedMember> {
    let collection = collections.iter_mut().find(|c| c.id == collection_id)?;
    if !member_ids_contain(&collection.member_ids, item_id, product_id) {
        return None;
    }
    let (removed_ids, kept): (Vec<u64>, Vec<u64>) =
        collection.member_ids
                  .iter()
                  .partition(|id| **id == item_id || **id == product_id);
    collection.member_ids = Arc::from(kept);
    Some(RemovedMember { collection_name: Arc::clone(&collection.name),
                         removed_ids })
}

/// Undoes [`remove_member`] after the service refused it.
pub fn restore_removed_member(collections: &mut [CollectionEntry], collection_id: u64,
                              removed_ids: &[u64]) {
    if let Some(collection) = collections.iter_mut().find(|c| c.id == collection_id) {
        let mut member_ids: Vec<u64> = collection.member_ids.to_vec();
        member_ids.extend_from_slice(removed_ids);
        collection.member_ids = Arc::from(member_ids);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn collection(id: u64, member_ids: &[u64]) -> CollectionEntry {
        CollectionEntry { id,
                          name: format!("Collection {id}").into(),
                          member_ids: member_ids.into(),
                          created_at: 0 }
    }

    #[test]
    fn add_appends_the_item_id_and_names_the_collection() {
        let mut collections = vec![collection(1, &[5])];

        let name = add_member(&mut collections, 1, 10, 20);

        assert_eq!(name.as_deref(), Some("Collection 1"));
        assert_eq!(&*collections[0].member_ids, &[5, 10]);
    }

    #[test]
    fn add_ignores_an_item_already_held_under_its_product_id() {
        let mut collections = vec![collection(1, &[20])];

        assert!(add_member(&mut collections, 1, 10, 20).is_none());
        assert_eq!(&*collections[0].member_ids, &[20]);
    }

    #[test]
    fn add_to_an_unknown_collection_does_nothing() {
        let mut collections = vec![collection(1, &[])];

        assert!(add_member(&mut collections, 2, 10, 20).is_none());
        assert!(collections[0].member_ids.is_empty());
    }

    #[test]
    fn revert_takes_only_the_added_id_back_out() {
        let mut collections = vec![collection(1, &[5])];
        add_member(&mut collections, 1, 10, 20);

        revert_added_member(&mut collections, 1, 10);

        assert_eq!(&*collections[0].member_ids, &[5]);
    }

    #[test]
    fn remove_takes_out_both_id_spaces_and_keeps_the_rest() {
        let mut collections = vec![collection(1, &[10, 20, 30])];

        let removed = remove_member(&mut collections, 1, 10, 20).unwrap();

        assert_eq!(removed.removed_ids, vec![10, 20]);
        assert_eq!(&*collections[0].member_ids, &[30]);
    }

    #[test]
    fn remove_of_a_non_member_does_nothing() {
        let mut collections = vec![collection(1, &[30])];

        assert!(remove_member(&mut collections, 1, 10, 20).is_none());
        assert_eq!(&*collections[0].member_ids, &[30]);
    }

    #[test]
    fn restore_puts_back_exactly_what_was_removed() {
        let mut collections = vec![collection(1, &[20, 30])];
        let removed = remove_member(&mut collections, 1, 10, 20).unwrap();

        restore_removed_member(&mut collections, 1, &removed.removed_ids);

        let mut member_ids = collections[0].member_ids.to_vec();
        member_ids.sort_unstable();
        assert_eq!(member_ids, vec![20, 30]);
    }
}
