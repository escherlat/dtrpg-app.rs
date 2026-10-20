//! `LibraryIndex`'s filter/search/sort query: dimensions in `clause`, list
//! and count queries in `query`, and the equivalence bar for the SQL search
//! box in `parity_tests`.
//!
//! Split out of the original `filtered_items.rs` under
//! `openspec/changes/search-box-semantics-sql`.

mod clause;
mod query;

#[cfg(test)]
// Index fixtures use unwrap for known-good in-memory database operations.
#[allow(clippy::unwrap_used)]
mod parity_tests;

pub use clause::ItemFilter;
pub(super) use query::{count_items_filtered, list_items_filtered};
