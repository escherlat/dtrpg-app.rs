//! `Library::start_load` and the catalog-sync stages up through the
//! auto-load policy's cache/partial-fetch decision. The fresh-install totals
//! request and the full paginated live fetch live in the sibling `fetch.rs`
//! — split out so this file (and that one) stay under the repo's 500-line
//! cap, not along any other seam.
//!
//! Ported from the UI controller's `cx.spawn`-based orchestration in ADR
//! 0002 Slice 2. The whole run is synchronous, blocking work (SQLite reads
//! and writes, blocking HTTP calls through [`LibraryService`]) — the
//! original `async_cx.background_executor().spawn(async move { .. })` calls
//! were only there to make blocking work awaitable on `gpui`'s executor, not
//! because the work was genuinely async. Here the whole run is one
//! `spawn_blocking` closure on the shared runtime (see [`crate::runtime`]),
//! the same way [`crate::work::WorkQueue`] runs a submitted job.
//!
//! Single-flight and generation tracking (`SyncState`) is the direct
//! equivalent of the UI controller's former `catalog_sync_in_flight`/
//! `load_generation`/`pending_reload` fields: at most one run is ever in
//! flight, and a request that arrives while one is running is recorded as a
//! pending follow-up rather than started or dropped.

use std::sync::{Arc, PoisonError};

use super::merge::{dedupe_by_product_id, partial_fetch_since, should_attempt_partial_fetch};
use super::types::{LibraryEvent, SyncStage};
use crate::data::catalog_cache::load_cache_metadata;
use crate::data::catalog_cache::load_catalog_cache;
use crate::data::collections_cache::{load_collections_cache, save_collections_cache};
use crate::data::library::LibraryItem;
use crate::library::Library;
use crate::services::LibraryService;
use crate::services::collections::{CollectionsService, CollectionsServiceErrorKind};

impl Library {
    /// Starts a catalog-sync run: collections, then the on-disk cache, then
    /// (unless the auto-load policy decides the cache is fresh enough) a
    /// live fetch from `service`.
    ///
    /// Runs on the shared runtime; returns immediately. Events describing
    /// the run's progress and outcome arrive on the [`super::LibraryEvents`]
    /// receiver returned by [`Library::open`].
    ///
    /// When a run is already in flight, this one is recorded as a pending
    /// follow-up instead of started — the in-flight run's `Finished` event
    /// carries `reload_requested: true`, and the caller is expected to call
    /// `start_load` again on receiving it. Returns `true` if this call
    /// actually started a run, `false` if it was queued as a follow-up.
    pub fn start_load(self: &Arc<Self>, service: Arc<dyn LibraryService>,
                      collections_service: Arc<dyn CollectionsService>, force_reload: bool)
                      -> bool {
        let generation = {
            let mut state = self.sync_state
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner);
            if state.in_flight {
                state.pending_reload = true;
                return false;
            }
            state.in_flight = true;
            state.generation += 1;
            state.generation
        };

        let handle = match crate::runtime::handle() {
            Ok(handle) => handle,
            Err(error) => {
                tracing::error!(%error, "catalog sync cannot start: no runtime available");
                self.emit(LibraryEvent::Failed {
                    message: format!("unable to start the runtime: {error}"),
                    quiet:   false,
                });
                self.finish_sync(generation);
                return true;
            }
        };

        let this = Arc::clone(self);
        drop(handle.spawn_blocking(move || {
                       this.run_catalog_sync(&service,
                                             &collections_service,
                                             force_reload,
                                             generation);
                       this.finish_sync(generation);
                   }));
        true
    }

    pub(super) fn emit(&self, event: LibraryEvent) {
        // A closed channel means nobody is watching; the run carries on.
        let _ = self.events.send(event);
    }

    /// Whether `generation` is still the current run — always true today,
    /// since `sync_state.in_flight` prevents a second run from starting
    /// (and therefore from bumping `generation`) while this one is in
    /// flight. Kept as a defensive check rather than assumed, matching the
    /// UI controller's former per-stage `load_generation` comparisons.
    pub(super) fn is_current(&self, generation: u64) -> bool {
        self.sync_state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .generation
        == generation
    }

    fn finish_sync(&self, generation: u64) {
        let reload_requested = {
            let mut state = self.sync_state
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner);
            state.in_flight = false;
            if state.generation == generation {
                std::mem::take(&mut state.pending_reload)
            }
            else {
                false
            }
        };
        self.emit(LibraryEvent::Finished { reload_requested });
    }

    fn run_catalog_sync(&self, service: &Arc<dyn LibraryService>,
                        collections_service: &Arc<dyn CollectionsService>, force_reload: bool,
                        generation: u64) {
        let cached = self.prepopulate_from_cache(force_reload, generation);
        self.sync_collections(collections_service, generation);

        if self.try_skip_or_partial_fetch(service, force_reload, &cached, generation) {
            return;
        }

        // `fresh_install_totals` and `run_full_fetch` live in the sibling
        // `fetch.rs` — see this file's module doc comment for why.
        let fresh_install_total = self.fresh_install_totals(service, force_reload, &cached);
        self.run_full_fetch(service, cached, fresh_install_total, generation);
    }

    /// Pre-populates the index from the on-disk cache, ahead of any network
    /// call. Returns the cached items (deduped, possibly empty) so later
    /// stages can decide whether a live fetch is still needed.
    fn prepopulate_from_cache(&self, force_reload: bool, generation: u64)
                              -> Option<Vec<LibraryItem>> {
        let cached = load_catalog_cache(&self.cache_root).map(dedupe_by_product_id);
        if let Some(items) = cached.as_ref()
           && self.is_current(generation)
        {
            if force_reload
               && let Err(e) = self.index
                                   .lock()
                                   .unwrap_or_else(PoisonError::into_inner)
                                   .replace_all_items(&[])
            {
                tracing::error!(error = %e, "failed to clear the library index");
            }
            // Only pre-populate when there's nothing in the index yet (fresh
            // session, or just cleared above) — appending again on an
            // already-populated catalog would duplicate every item.
            let count = self.index
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .count_all()
                            .unwrap_or(0);
            let appended = if count == 0 {
                self.append_items(items.clone());
                items.clone()
            }
            else {
                Vec::new()
            };
            // Whatever the network does next, the tags are on disk — this
            // event fires whenever a cache existed at all, not only when it
            // was actually appended above.
            self.emit(LibraryEvent::CachePrepopulated { items: appended,
                                                        force_reload });
        }
        cached
    }

    /// Loads the cached collections list, then refreshes it from the live
    /// service. Non-fatal on failure: collections and catalog are
    /// independent datasets, so a collections failure does not abort the
    /// catalog stages that follow.
    fn sync_collections(&self, collections_service: &Arc<dyn CollectionsService>, generation: u64) {
        self.emit(LibraryEvent::Stage(SyncStage::Collections));

        if let Some(entries) = load_collections_cache(&self.cache_root)
           && self.is_current(generation)
        {
            self.emit(LibraryEvent::CollectionsChanged(entries));
        }

        match collections_service.list_collections() {
            Ok(entries) => {
                if let Err(e) = save_collections_cache(&self.cache_root, &entries) {
                    tracing::warn!(error = %e, "failed to save collections cache");
                }
                if self.is_current(generation) {
                    self.emit(LibraryEvent::CollectionsChanged(entries));
                }
            }
            Err(e) => {
                // WARN, not debug: the default log filter is `warn`, and
                // "not signed in yet" is exactly the kind of routine-but-
                // worth-knowing condition a user reading logs should see
                // without setting RUST_LOG.
                if e.kind == CollectionsServiceErrorKind::Session {
                    tracing::warn!(error = %e, "collections load skipped: no authenticated session");
                }
                else {
                    tracing::warn!(error = %e, "collections load failed");
                }
            }
        }
    }

    /// Returns `true` if the run is fully handled (either skipped or
    /// resolved by a partial fetch) and the caller should not proceed to the
    /// full paginated fetch.
    fn try_skip_or_partial_fetch(&self, service: &Arc<dyn LibraryService>, force_reload: bool,
                                 cached: &Option<Vec<LibraryItem>>, generation: u64)
                                 -> bool {
        // Skip the live fetch when: the cache is non-empty, was written
        // within the last 7 days, and force_reload is false.
        let Some(items) =
            (!force_reload).then(|| cached.as_ref().filter(|items| !items.is_empty()))
                           .flatten()
        else {
            return false;
        };

        let is_fresh = load_cache_metadata(&self.cache_root).as_ref()
                                                            .is_some_and(|m| !m.is_stale());
        if !is_fresh {
            return false;
        }

        self.emit(LibraryEvent::Stage(SyncStage::CountCheck));
        let count_matches = match service.count_items() {
            Some(Ok(n)) => n == items.len(),
            Some(Err(_)) => false, // count fetch failed; do live fetch
            None => true,          // count not supported; trust fresh cache
        };

        if count_matches {
            tracing::debug!(cached = items.len(),
                            "catalog auto-load: cache is fresh, skipping live fetch");
            if self.is_current(generation) {
                self.emit(LibraryEvent::SkippedFreshCache);
            }
            return true;
        }

        let Some(Ok(remote_n)) = service.count_items()
        else {
            return false;
        };
        if !should_attempt_partial_fetch(remote_n, items.len()) {
            return false;
        }
        let Some(since) = partial_fetch_since(items)
        else {
            return false;
        };

        // Growth-only mismatch: try a cheaper additive partial fetch before
        // falling back to a full paginated one. A decrease (or a mismatch a
        // pure addition can't explain) always falls through to the full
        // fetch, since only reconciliation can identify *which* item is
        // missing.
        self.emit(LibraryEvent::Stage(SyncStage::PartialFetch));
        let mut partial_items = Vec::new();
        let partial =
            service.list_items_updated_since(&since, &mut |page| partial_items.extend(page));
        let Some(Ok(())) = partial
        else {
            // Unsupported, an error, or a stale run: fall through to the
            // full paginated fetch below.
            return false;
        };
        if !self.is_current(generation) {
            return true;
        }

        self.merge_partial(partial_items);
        true
    }

    /// Appends `items` to the index verbatim (no dedup) and emits no event —
    /// used by stages that need to queue their own event after the write.
    pub(super) fn append_items(&self, items: Vec<LibraryItem>) {
        let mut index = self.index.lock().unwrap_or_else(PoisonError::into_inner);
        let mut merged = index.list_items().unwrap_or_default();
        merged.extend(items);
        if let Err(e) = index.replace_all_items(&merged) {
            tracing::error!(error = %e, "failed to write the catalog to the library index");
        }
    }

    /// Merges a partial date-filtered fetch's results into the index
    /// additively and emits [`LibraryEvent::PartialMerged`].
    fn merge_partial(&self, partial: Vec<LibraryItem>) {
        let mut index = self.index.lock().unwrap_or_else(PoisonError::into_inner);
        let merged = super::merge::merge_partial_fetch(index.list_items().unwrap_or_default(),
                                                       partial.clone());
        if let Err(e) = index.replace_all_items(&merged) {
            tracing::error!(error = %e, "failed to write the catalog to the library index");
        }
        drop(index);
        self.emit(LibraryEvent::PartialMerged { items: partial });
    }
}
