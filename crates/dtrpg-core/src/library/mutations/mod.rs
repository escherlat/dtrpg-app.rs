//! Catalog mutations: ADR 0002 Slice 5.
//!
//! `Library` owns every change to a catalog entry that is not part of a
//! catalog-sync run: a download finishing or being removed, a disk check
//! finding a file gone, an availability check coming back, the catalog being
//! cleared, and the optimistic collection-membership edits. The UI controller
//! calls these, then does what only it can: refresh its view caches with its
//! own view state, emit events, and drive the activity panel.

mod check_result;
mod collection_members;
mod items;

pub use check_result::apply_check_result;
pub use collection_members::{
    RemovedMember, add_member, remove_member, restore_removed_member, revert_added_member,
};
