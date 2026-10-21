//! Field accessors for `LibraryController` that reach through the core
//! `Library` session. Kept in a sibling file so `mod.rs` stays under the
//! line-limit; these will shrink as later decomposition slices move logic
//! into `Library` itself.

use std::path::PathBuf;
use std::sync::{MutexGuard, PoisonError};

use dtrpg_core::data::index::LibraryIndex;
use dtrpg_core::work::WorkQueue;

use super::LibraryController;

impl LibraryController {
    /// Locks and returns the core library index.
    ///
    /// The index lives behind a `Mutex` (since ADR 0002 Slice 2: the
    /// catalog-sync run mutates it from a background thread while `Library`
    /// is shared via `Arc`), not because callers need read/write
    /// distinction — both `index()` and `index_mut()` take the same lock;
    /// kept as two methods only so call sites written for the old
    /// `&LibraryIndex`/`&mut LibraryIndex` split keep compiling unchanged.
    pub(super) fn index(&self) -> MutexGuard<'_, LibraryIndex> {
        self.library
            .index
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// See [`Self::index`].
    pub(super) fn index_mut(&self) -> MutexGuard<'_, LibraryIndex> {
        self.library
            .index
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Shared reference to the core work queue.
    pub(super) fn work(&self) -> &WorkQueue {
        &self.library.work
    }

    /// Path to the directory that holds the library index database.
    pub(super) fn cache_root(&self) -> &PathBuf {
        &self.library.cache_root
    }
}
