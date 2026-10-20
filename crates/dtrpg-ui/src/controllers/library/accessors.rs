//! Field accessors for `LibraryController` that reach through the core
//! `Library` session. Kept in a sibling file so `mod.rs` stays under the
//! line-limit; these will shrink as later decomposition slices move logic
//! into `Library` itself.

use std::path::PathBuf;
use std::sync::Arc;

use dtrpg_core::data::index::LibraryIndex;
use dtrpg_core::services::covers::CoverFetcher;
use dtrpg_core::services::network_monitor::NetworkMonitor;
use dtrpg_core::work::WorkQueue;

use super::LibraryController;

impl LibraryController {
    /// Shared reference to the core library index.
    pub(super) fn index(&self) -> &LibraryIndex {
        &self.library.index
    }

    /// Mutable reference to the core library index.
    pub(super) fn index_mut(&mut self) -> &mut LibraryIndex {
        &mut self.library.index
    }

    /// Shared reference to the core work queue.
    pub(super) fn work(&self) -> &WorkQueue {
        &self.library.work
    }

    /// Shared reference to the cover fetcher.
    pub(super) fn cover_fetcher(&self) -> &Arc<CoverFetcher> {
        &self.library.cover_fetcher
    }

    /// Shared reference to the network monitor.
    pub(super) fn network_monitor(&self) -> &Arc<NetworkMonitor> {
        &self.library.network_monitor
    }

    /// Path to the directory that holds the library index database.
    pub(super) fn cache_root(&self) -> &PathBuf {
        &self.library.cache_root
    }
}
