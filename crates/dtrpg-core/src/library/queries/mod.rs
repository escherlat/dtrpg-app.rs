//! Filtered/grouped catalog queries and their caches: ADR 0002 Slice 3.
//!
//! `Library` owns the result of "what does the catalog look like right now,
//! given the current filter/search/sort/group settings" — the same cache
//! the UI controller used to hold as `items_cache`/`grouped_items_cache`/
//! `filter_count_cache`/`catalog_ids_cache`/`section_counts`/`publishers`.
//! The controller passes its view-state (filter, search text, sort, group)
//! into [`super::Library::invalidate_cache`] whenever one of those inputs
//! changes, and reads the result back with plain `Arc` clones on every
//! render — no new SQLite I/O or whole-catalog copies on the render path.
//!
//! The two ephemeral per-item tracking maps (`thumbnail_attempts`,
//! `availability_checks`) live here too, behind [`overlay`] — they're
//! session-only state the index never persists, but they feed directly into
//! the cached item list this module owns, so splitting them from it would
//! either corrupt the cache or force a per-render re-overlay.

mod cache;
mod overlay;
mod tags;
#[cfg(test)]
mod tests;
mod types;

pub use types::ViewState;
pub(super) use types::{EphemeralOverlay, QueryCache};
