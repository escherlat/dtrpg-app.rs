//! The fresh-install totals request and the full paginated live fetch —
//! split out of `run.rs` to stay under the repo's 500-line-per-file cap, not
//! along any other seam. See `run.rs`'s module doc comment for the overall
//! shape of a catalog-sync run.

use std::sync::{Arc, PoisonError};

use super::merge::{fresh_install_request_gated, reconcile_catalog};
use super::types::{LibraryEvent, SyncStage};
use crate::data::catalog_cache::{
    load_cache_metadata, save_catalog_cache, save_fresh_install_request_timestamp,
};
use crate::data::constants::{
    CATALOG_CHECKPOINT_MIN_INTERVAL_SECS, CATALOG_CHECKPOINT_PAGE_INTERVAL,
    CATALOG_SYNC_MAX_ATTEMPTS, CATALOG_SYNC_RETRY_BASE_DELAY_SECS,
    CATALOG_SYNC_RETRY_MAX_DELAY_SECS, DTRPG_API_HOST,
};
use crate::data::library::LibraryItem;
use crate::library::Library;
use crate::services::{LibraryService, LibraryServiceError, LibraryServiceErrorKind, retry};

/// Captures the current backtrace and keeps only frames that mention a
/// `dtrpg_` symbol, so a logged load failure points at application code
/// rather than the runtime stack beneath it. Moved here from the UI
/// controller's `diagnostics.rs` in ADR 0002 Slice 2.
fn app_backtrace() -> String {
    let bt = std::backtrace::Backtrace::capture();
    if bt.status() != std::backtrace::BacktraceStatus::Captured {
        return "<set RUST_BACKTRACE=1 to capture backtraces>".to_owned();
    }
    let full = format!("{bt}");
    let mut out: Vec<&str> = Vec::new();
    let mut take_location = false;
    for line in full.lines() {
        if line.trim_start().starts_with("at ") {
            if take_location {
                out.push(line);
                take_location = false;
            }
        }
        else if line.contains("dtrpg_") {
            out.push(line);
            take_location = true;
        }
        else {
            take_location = false;
        }
    }
    if out.is_empty() {
        "<no app frames found in backtrace>".to_owned()
    }
    else {
        out.join("\n")
    }
}

impl Library {
    /// Issues a dedicated totals request ahead of the page-by-page fetch, on
    /// a fresh install only, so its progress bar can start determinate.
    /// Gated by a minimum interval so repeated fresh-install attempts don't
    /// hammer the API with a totals request every time.
    pub(super) fn fresh_install_totals(&self, service: &Arc<dyn LibraryService>,
                                       force_reload: bool, cached: &Option<Vec<LibraryItem>>)
                                       -> Option<usize> {
        let is_fresh_install = !force_reload && cached.as_ref().is_none_or(Vec::is_empty);
        if !is_fresh_install {
            return None;
        }

        let last_request =
            load_cache_metadata(&self.cache_root).and_then(|m| m.last_fresh_install_request_secs);
        let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                                                   .unwrap_or_default()
                                                   .as_secs();
        if fresh_install_request_gated(last_request, now_secs) {
            tracing::debug!("fresh-install initialization skipped: last request was within the minimum interval");
            return None;
        }
        if !self.network_monitor.check_endpoint(DTRPG_API_HOST) {
            tracing::debug!("fresh-install totals request skipped: DriveThruRPG API unreachable");
            return None;
        }

        let cancel = std::sync::atomic::AtomicBool::new(false);
        let config = retry::RetryConfig { max_attempts: CATALOG_SYNC_MAX_ATTEMPTS,
                                          base_secs:    CATALOG_SYNC_RETRY_BASE_DELAY_SECS,
                                          max_secs:     CATALOG_SYNC_RETRY_MAX_DELAY_SECS, };
        let mut on_retry = |attempt: u32, delay: std::time::Duration, e: &LibraryServiceError| {
            tracing::debug!(attempt, delay_secs = delay.as_secs(), reason = %e,
                            "fresh-install totals request retrying");
        };
        // `count_items()` returns `Option<Result<usize, _>>` — `None` means
        // "count not supported" (not an error, nothing to retry); folded
        // into `Ok(None)` so `retry_with_backoff` only sees a plain `Result`.
        let totals: Result<Option<usize>, LibraryServiceError> =
            retry::retry_with_backoff(config,
                                      &cancel,
                                      || match service.count_items() {
                                          None => Ok(None),
                                          Some(Ok(n)) => Ok(Some(n)),
                                          Some(Err(e)) => Err(e),
                                      },
                                      |e: &LibraryServiceError| {
                                          e.kind == LibraryServiceErrorKind::Network
                                          || e.kind == LibraryServiceErrorKind::RateLimited
                                      },
                                      |e: &LibraryServiceError| e.retry_after,
                                      || {
                                          LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                                                   "totals request cancelled")
                                      },
                                      Some(&mut on_retry));

        let fresh_install_total = match totals {
            Ok(Some(count)) => {
                tracing::debug!(total_items = count, "fresh-install: totals request");
                Some(count)
            }
            Ok(None) => None,
            Err(e) if e.kind == LibraryServiceErrorKind::Session => {
                tracing::warn!(error = %e,
                               "fresh-install initialization waiting for authenticated session");
                None
            }
            Err(e) => {
                tracing::warn!(error = %e, "fresh-install totals request failed");
                None
            }
        };

        if let Err(e) = save_fresh_install_request_timestamp(&self.cache_root, now_secs) {
            tracing::warn!(error = %e, "failed to save fresh-install request timestamp");
        }
        fresh_install_total
    }

    /// The full paginated live fetch, with checkpointing and progress
    /// reporting.
    pub(super) fn run_full_fetch(&self, service: &Arc<dyn LibraryService>,
                                 cached: Option<Vec<LibraryItem>>,
                                 fresh_install_total: Option<usize>, generation: u64) {
        // Query the network monitor before committing to the paginated fetch
        // — a process needing a specific endpoint stops rather than
        // requesting when it's reported unreachable.
        if !self.network_monitor.check_endpoint(DTRPG_API_HOST) {
            tracing::debug!("catalog fetch skipped: DriveThruRPG API unreachable");
            if self.is_current(generation) {
                self.emit(LibraryEvent::Failed { message: "DriveThruRPG API unreachable".to_owned(),
                                                 quiet:   true });
            }
            return;
        }

        self.emit(LibraryEvent::Stage(SyncStage::LiveFetch));

        let catalog_was_empty = self.index
                                    .lock()
                                    .unwrap_or_else(PoisonError::into_inner)
                                    .count_all()
                                    .unwrap_or(0)
                                == 0;

        // Seed the progress denominator, preferring the fresh-install totals
        // request (reflects the remote count directly) over a best-effort
        // `count_items()` call, over the stale cache's count.
        let live_count = if fresh_install_total.is_none() {
            service.count_items().and_then(Result::ok)
        }
        else {
            None
        };
        let estimated_total: Option<usize> =
            fresh_install_total.or(live_count)
                               .or_else(|| cached.as_ref().map(Vec::len).filter(|&n| n > 0));
        if let Some(total) = estimated_total {
            self.emit(LibraryEvent::Total(total));
            tracing::debug!(estimated_total = total, "catalog load: seeded total");
        }

        // `on_page` and `on_total` are two independent closures the service
        // may call interleaved (never concurrently — `list_items_paged`
        // calls back on this one thread), so their shared mutable state is
        // behind `RefCell`s rather than captured by `&mut`, which the borrow
        // checker would not allow two sibling closures to share.
        let live_items = std::cell::RefCell::new(Vec::<LibraryItem>::new());
        let checkpoint = std::cell::RefCell::new((0u32, std::time::Instant::now()));
        let page_num = std::cell::Cell::new(0u32);

        // Accumulate all live SDK pages into a local buffer so a *populated*
        // cache remains visible throughout the fetch instead of flashing
        // down to a single page (see `catalog-live-merge`). When there is
        // nothing cached to protect, each page is appended to the index (and
        // its event emitted) as it arrives instead of waiting for the whole
        // fetch to finish.
        let result =
            service.list_items_paged(
                              &mut |page: Vec<LibraryItem>| {
                                  if catalog_was_empty && self.is_current(generation) {
                                      self.append_items(page.clone());
                                      self.emit(LibraryEvent::PageAppended { items: page.clone() });
                                  }
                                  live_items.borrow_mut().extend(page);
                                  let page_num = {
                                      page_num.set(page_num.get() + 1);
                                      page_num.get()
                                  };

                                  // Checkpoint the accumulating buffer every N
                                  // pages, or after
                                  // a minimum interval, whichever fires first
                                  // (see
                                  // `catalog-cache-checkpointing`).
                                  let (last_checkpoint_page, last_checkpoint_at) =
                                      *checkpoint.borrow();
                                  let pages_since_checkpoint = page_num - last_checkpoint_page;
                                  if pages_since_checkpoint >= CATALOG_CHECKPOINT_PAGE_INTERVAL
                   || last_checkpoint_at.elapsed()
                      >= std::time::Duration::from_secs(CATALOG_CHECKPOINT_MIN_INTERVAL_SECS)
                {
                    if let Err(e) = save_catalog_cache(&self.cache_root, &live_items.borrow()) {
                        tracing::warn!(error = %e, "failed to checkpoint catalog cache");
                    }
                    *checkpoint.borrow_mut() = (page_num, std::time::Instant::now());
                }

                                  self.emit(LibraryEvent::Page(page_num));
                              },
                              Some(&mut |total| {
                                  tracing::debug!(estimated_total = total,
                                                  "catalog load: API-reported item count");
                                  self.emit(LibraryEvent::Total(total));
                              }),
            );

        if !self.is_current(generation) {
            return;
        }

        match result {
            Ok(()) => {
                self.replace_catalog(live_items.into_inner(), !catalog_was_empty);
            }
            Err(e) => {
                // Session errors are expected when starting before auth
                // completes; treat them as a quiet completion rather than a
                // user-facing alert. Network and other errors are genuine
                // failures worth surfacing.
                let quiet = e.kind == LibraryServiceErrorKind::Session;
                if quiet {
                    tracing::warn!(error = %e, "catalog load skipped: no authenticated session");
                }
                else {
                    tracing::error!(error = %e, backtrace = %app_backtrace(), "catalog load failed");
                }
                self.emit(LibraryEvent::Failed { message: e.to_string(),
                                                 quiet });
            }
        }
    }

    /// Atomically replaces the index with a complete live dataset — merging
    /// by id via [`reconcile_catalog`] when `reconcile` is true (there was an
    /// existing baseline to reconcile against) or replacing it outright when
    /// `false` (first launch, or right after a cache clear) — and emits
    /// [`LibraryEvent::FullyLoaded`].
    fn replace_catalog(&self, items: Vec<LibraryItem>, reconcile: bool) {
        let mut index = self.index.lock().unwrap_or_else(PoisonError::into_inner);
        let merged = if reconcile {
            reconcile_catalog(index.list_items().unwrap_or_default(), items.clone())
        }
        else {
            items.clone()
        };
        if let Err(e) = index.replace_all_items(&merged) {
            tracing::error!(error = %e, "failed to write the catalog to the library index");
        }
        drop(index);
        self.emit(LibraryEvent::FullyLoaded { items });
    }
}
