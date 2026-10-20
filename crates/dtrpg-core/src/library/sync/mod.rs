//! The catalog-sync state machine: `Library::start_load` and the stages it
//! runs through, plus the pure merge/gating helpers those stages use.
//!
//! This module has no `gpui` dependency, so its stages — unlike the
//! `cx.spawn`-based orchestration the UI controller used before ADR 0002
//! Slice 2 — can be driven and asserted on directly in tests (see
//! `tests.rs`).

mod fetch;
mod merge;
mod run;
mod types;

#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod tests;

pub use merge::reload_cooldown_active;
pub(super) use types::SyncState;
pub use types::{LibraryEvent, LibraryEvents, SyncStage};
