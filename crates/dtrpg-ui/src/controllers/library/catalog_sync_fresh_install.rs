//! The fresh-install totals stage of catalog sync: on a fresh install (no
//! cached catalog data), request an item count ahead of the page-by-page
//! fetch so its progress bar can start determinate. See `catalog_sync.rs`
//! for the orchestrator this stage is called from.

use super::catalog_sync::CatalogSyncCtx;
use super::*;

/// Issues a dedicated totals request (item count only — the API has no
/// aggregate-size endpoint; per-file `size`/`sizeMB` only exist within an
/// already-fetched item's file list) before any page of item data, gated by
/// a minimum interval so repeated fresh-install attempts (e.g. relaunching
/// shortly after a failed first attempt) don't hammer the API with a totals
/// request every time. Returns `None` on anything but a fresh install, or
/// when the request was gated, skipped, or failed.
pub(super) async fn fresh_install_totals(ctx: &CatalogSyncCtx, async_cx: &mut AsyncApp,
                                         force_reload: bool, cached: &Option<Vec<LibraryItem>>)
                                         -> Option<usize> {
    let is_fresh_install = !force_reload && cached.as_ref().is_none_or(Vec::is_empty);
    if !is_fresh_install {
        return None;
    }

    let meta_root = ctx.storage_root.clone();
    let last_request = async_cx.background_executor()
                               .spawn(async move { load_cache_metadata(&meta_root) })
                               .await
                               .and_then(|m| m.last_fresh_install_request_secs);
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                                               .unwrap_or(std::time::Duration::ZERO)
                                               .as_secs();
    let gated = fresh_install_request_gated(last_request, now_secs);
    let monitor_for_totals = ctx.network_monitor.clone();
    let endpoint_reachable =
        async_cx.background_executor()
                .spawn(async move {
                    monitor_for_totals.check_endpoint(dtrpg_core::data::constants::DTRPG_API_HOST)
                })
                .await;

    if gated {
        tracing::debug!("fresh-install initialization skipped: last request was within the minimum interval");
        return None;
    }
    if !endpoint_reachable {
        tracing::debug!("fresh-install totals request skipped: DriveThruRPG API unreachable");
        return None;
    }

    let svc = ctx.service_arc.clone();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let config =
        retry::RetryConfig { max_attempts: dtrpg_core::data::constants::CATALOG_SYNC_MAX_ATTEMPTS,
                             base_secs:
                                 dtrpg_core::data::constants::CATALOG_SYNC_RETRY_BASE_DELAY_SECS,
                             max_secs:
                                 dtrpg_core::data::constants::CATALOG_SYNC_RETRY_MAX_DELAY_SECS, };
    let mut on_retry = |attempt: u32, delay: std::time::Duration, e: &LibraryServiceError| {
        tracing::debug!(
            attempt,
            delay_secs = delay.as_secs(),
            reason = %e,
            "fresh-install totals request retrying"
        );
    };
    // `count_items()` returns `Option<Result<usize, _>>` — `None` means
    // "count not supported by this implementation" (not an error, nothing to
    // retry); folded into `Ok(None)` here so `retry_with_backoff` only sees a
    // plain `Result` and only retries a genuine `Network`-kind failure.
    let totals: Result<Option<usize>, LibraryServiceError> = async_cx
        .background_executor()
        .spawn(async move {
            retry::retry_with_backoff(
                config,
                &cancel,
                || match svc.count_items() {
                    None => Ok(None),
                    Some(Ok(n)) => Ok(Some(n)),
                    Some(Err(e)) => Err(e),
                },
                |e: &LibraryServiceError| {
                    e.kind == LibraryServiceErrorKind::Network || e.kind == LibraryServiceErrorKind::RateLimited
                },
                |e: &LibraryServiceError| e.retry_after,
                || LibraryServiceError::new(LibraryServiceErrorKind::Network, "totals request cancelled"),
                Some(&mut on_retry),
            )
        })
        .await;

    let fresh_install_total = match totals {
        Ok(Some(count)) => {
            tracing::debug!(total_items = count, "fresh-install: totals request");
            Some(count)
        }
        Ok(None) => None,
        Err(e) if e.kind == LibraryServiceErrorKind::Session => {
            // Routine, not a failure: no authenticated session yet. WARN
            // (not debug) so it's visible under the default `warn` log
            // filter without setting RUST_LOG, matching the collections/
            // catalog-fetch Session-error branches elsewhere in this
            // sequence.
            tracing::warn!(error = %e, "fresh-install initialization waiting for authenticated session");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "fresh-install totals request failed");
            None
        }
    };

    let request_root = ctx.storage_root.clone();
    async_cx.background_executor()
            .spawn(async move {
                if let Err(e) = save_fresh_install_request_timestamp(&request_root, now_secs) {
                    tracing::warn!(error = %e, "failed to save fresh-install request timestamp");
                }
            })
            .await;

    fresh_install_total
}
