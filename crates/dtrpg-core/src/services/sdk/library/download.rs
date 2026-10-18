//! Streams a prepared download to disk.
//!
//! `prepare_download`'s response (`data.attributes.url`) is a watermarking
//! portal URL that redirects to a pre-signed object-storage URL with a short
//! expiry (observed: 30 seconds) — the fetch below happens immediately after
//! preparation, in the same call, so there is no gap for that URL to expire.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::gateway::SdkLibraryGateway;
use crate::constants::{
    DOWNLOAD_RETRY_BASE_DELAY_SECS, DOWNLOAD_RETRY_MAX_DELAY_SECS, MAX_DOWNLOAD_ATTEMPTS,
};
use crate::services::retry::{RetryConfig, retry_with_backoff};
use crate::services::{DownloadControl, LibraryServiceError, LibraryServiceErrorKind};

const CHUNK_SIZE: usize = 64 * 1024;

/// How often a transfer reports progress. Reporting after every chunk would
/// send thousands of updates for a large file, with nobody able to see them.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Reports bytes received so far, and the total size if known.
type ProgressFn<'a> = &'a mut dyn FnMut(u64, Option<u64>);

/// Downloads the file identified by `(order_product_id, index)` to `dest`,
/// via `gateway.prepare_download`, retrying a retryable transfer failure with
/// exponential backoff. See
/// [`LibraryService::download_item`](crate::services::LibraryService::download_item)
/// for the cancellation, temp-file, and retry-progress contract.
pub(super) fn download_item(gateway: &dyn SdkLibraryGateway, order_product_id: u64, index: u32,
                            dest: &Path, control: DownloadControl<'_>)
                            -> Result<(), LibraryServiceError> {
    let config = RetryConfig { max_attempts: MAX_DOWNLOAD_ATTEMPTS,
                               base_secs:    DOWNLOAD_RETRY_BASE_DELAY_SECS,
                               max_secs:     DOWNLOAD_RETRY_MAX_DELAY_SECS, };
    download_item_with_config(gateway, order_product_id, index, dest, control, config)
}

/// Same as [`download_item`] with an injectable retry policy, so tests can
/// use a zero-delay [`RetryConfig`] instead of the production backoff
/// schedule.
fn download_item_with_config(gateway: &dyn SdkLibraryGateway, order_product_id: u64, index: u32,
                             dest: &Path, control: DownloadControl<'_>, config: RetryConfig)
                             -> Result<(), LibraryServiceError> {
    let DownloadControl { cancel,
                          mut on_retry,
                          mut on_progress, } = control;
    let mut adapted_on_retry = |attempt: u32, delay: Duration, _error: &LibraryServiceError| {
        if let Some(f) = on_retry.as_deref_mut() {
            f(attempt, delay);
        }
    };
    let mut report_progress = |received: u64, total: Option<u64>| {
        if let Some(f) = on_progress.as_deref_mut() {
            f(received, total);
        }
    };

    retry_with_backoff(config,
                       cancel,
                       || {
                           attempt_download(gateway,
                                            order_product_id,
                                            index,
                                            dest,
                                            cancel,
                                            &mut report_progress)
                       },
                       |e: &LibraryServiceError| {
                           e.kind == LibraryServiceErrorKind::Network
                           || e.kind == LibraryServiceErrorKind::RateLimited
                       },
                       |e: &LibraryServiceError| e.retry_after,
                       || {
                           LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                                    "download cancelled")
                       },
                       Some(&mut adapted_on_retry))
}

/// Makes a single download attempt: resolves the transfer URL, streams to a
/// `.part` file, then renames it to `dest` on success.
fn attempt_download(gateway: &dyn SdkLibraryGateway, order_product_id: u64, index: u32,
                    dest: &Path, cancel: &AtomicBool, on_progress: ProgressFn<'_>)
                    -> Result<(), LibraryServiceError> {
    // Check cancellation before `prepare_download`: a queued download that was
    // cancelled before it started must not issue the authenticated prepare
    // request. (`retry_with_backoff` also checks before the first attempt; this
    // guards the operation itself so the invariant holds regardless of caller.)
    if cancel.load(Ordering::SeqCst) {
        return Err(LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                            "download cancelled"));
    }

    let response = gateway.prepare_download(order_product_id, index)?;
    let url = response
        .pointer("/data/attributes/url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            LibraryServiceError::new(
                LibraryServiceErrorKind::Network,
                "prepare_download response missing data.attributes.url",
            )
        })?;

    let part_path = part_path_for(dest);
    match stream_to_file(url, &part_path, cancel, on_progress) {
        Ok(()) => std::fs::rename(&part_path, dest).map_err(|e| {
                                                       LibraryServiceError::new(
                LibraryServiceErrorKind::Network,
                format!("failed to finalize download at {}: {e}", dest.display()),
            )
                                                   }),
        Err(e) => {
            let _ = std::fs::remove_file(&part_path);
            Err(e)
        }
    }
}

/// Derives `{dest}.part` — appends rather than replaces `dest`'s extension,
/// so `Foo.pdf` becomes `Foo.pdf.part`, not `Foo.part`.
fn part_path_for(dest: &Path) -> PathBuf {
    let mut name = dest.file_name()
                       .map(std::ffi::OsStr::to_os_string)
                       .unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

fn stream_to_file(url: &str, part_path: &Path, cancel: &AtomicBool, on_progress: ProgressFn<'_>)
                  -> Result<(), LibraryServiceError> {
    if let Some(parent) = part_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
                                           LibraryServiceError::new(
                LibraryServiceErrorKind::Network,
                format!(
                    "failed to create download directory {}: {e}",
                    parent.display()
                ),
            )
                                       })?;
    }

    let mut response = reqwest::blocking::get(url).map_err(|e| {
                           LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                                    format!("download request failed: {e}"))
                       })?;
    if response.status().as_u16() == 429 {
        let retry_after = response.headers()
                                  .get(reqwest::header::RETRY_AFTER)
                                  .and_then(|v| v.to_str().ok())
                                  .and_then(|v| v.trim().parse::<u64>().ok())
                                  .map(Duration::from_secs);
        return Err(LibraryServiceError::new(
            LibraryServiceErrorKind::RateLimited,
            "download rate limited: HTTP 429",
        )
        .with_retry_after(retry_after));
    }
    if !response.status().is_success() {
        return Err(LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                            format!("download failed: HTTP {}",
                                                    response.status())));
    }

    let mut file = std::fs::File::create(part_path).map_err(|e| {
                                                       LibraryServiceError::new(
            LibraryServiceErrorKind::Network,
            format!("failed to create {}: {e}", part_path.display()),
        )
                                                   })?;

    let total = response.content_length();
    let mut received: u64 = 0;
    let mut last_report = Instant::now();
    let mut buf = [0u8; CHUNK_SIZE];
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(LibraryServiceError::new(LibraryServiceErrorKind::Network,
                                                "download cancelled"));
        }
        let n = response.read(&mut buf).map_err(|e| {
                                            LibraryServiceError::new(
                LibraryServiceErrorKind::Network,
                format!("download read failed: {e}"),
            )
                                        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| {
            LibraryServiceError::new(
                LibraryServiceErrorKind::Network,
                format!("download write failed: {e}"),
            )
        })?;
        received += n as u64;
        if last_report.elapsed() >= PROGRESS_INTERVAL {
            on_progress(received, total);
            last_report = Instant::now();
        }
    }
    // Always end with the full count, so even a file that arrived within one
    // interval reports once.
    on_progress(received, total);
    Ok(())
}

#[cfg(test)]
mod tests;
