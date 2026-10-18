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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use dtrpg_sdk::{LibraryItemsParams, OrderProductItemResponse, OrderProductListResponse};

    use super::*;

    #[test]
    fn part_path_appends_extension() {
        let dest = Path::new("/tmp/items/b1/Book.pdf");
        assert_eq!(part_path_for(dest),
                   Path::new("/tmp/items/b1/Book.pdf.part"));
    }

    #[test]
    fn part_path_handles_no_extension() {
        let dest = Path::new("/tmp/items/b1/README");
        assert_eq!(part_path_for(dest), Path::new("/tmp/items/b1/README.part"));
    }

    // ── Retry loop tests ─────────────────────────────────────────────────

    fn fast_config(max_attempts: u32) -> RetryConfig {
        RetryConfig { max_attempts,
                      base_secs: 0,
                      max_secs: 0 }
    }

    fn temp_dest(name: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("dtrpg-download-test-{}-{name}-{unique}",
                                          std::process::id()))
    }

    /// What a scripted mock HTTP connection does when the download stream
    /// tries to fetch it.
    enum MockResponse {
        /// Drops the connection without writing anything, simulating a
        /// network failure.
        ConnectionReset,
        /// Writes a 503 status, simulating a retryable server failure.
        ServerError,
        /// Writes a 429 with the given `Retry-After` value (seconds),
        /// simulating a rate-limited response.
        RateLimited(u64),
        /// Writes a 200 with the given body.
        Success(&'static [u8]),
        /// Writes a 200 with the given body and no `Content-Length`, so the
        /// total size is unknown until the connection closes.
        SuccessWithoutLength(&'static [u8]),
    }

    /// Serves one response per script entry, in order, on a background
    /// thread bound to an ephemeral local port. Returns the base URL and the
    /// thread's join handle.
    fn spawn_mock_server(script: Vec<MockResponse>) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/file", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            for resp in script {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                match resp {
                    MockResponse::ConnectionReset => drop(stream),
                    MockResponse::ServerError => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                    MockResponse::RateLimited(retry_after_secs) => {
                        let header = format!("HTTP/1.1 429 Too Many Requests\r\nRetry-After: {retry_after_secs}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        let _ = stream.write_all(header.as_bytes());
                    }
                    MockResponse::Success(body) => {
                        let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                             body.len());
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(body);
                    }
                    MockResponse::SuccessWithoutLength(body) => {
                        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n");
                        let _ = stream.write_all(body);
                    }
                }
            }
        });
        (url, handle)
    }

    /// A gateway whose `prepare_download` always resolves to `url`, counting
    /// how many times it was called.
    struct CountingGateway {
        url:   String,
        calls: AtomicU32,
    }

    impl SdkLibraryGateway for CountingGateway {
        fn list_order_products(&self, _params: LibraryItemsParams)
                               -> Result<OrderProductListResponse, LibraryServiceError> {
            unimplemented!("not exercised by download tests")
        }

        fn get_order_product(&self, _id: u64)
                             -> Result<OrderProductItemResponse, LibraryServiceError> {
            unimplemented!("not exercised by download tests")
        }

        fn prepare_download(&self, _order_product_id: u64, _index: u32)
                            -> Result<serde_json::Value, LibraryServiceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({ "data": { "attributes": { "url": self.url } } }))
        }
    }

    /// A gateway whose `prepare_download` always fails with a non-`Network`
    /// error, counting how many times it was called.
    struct AlwaysSessionErrorGateway {
        calls: AtomicU32,
    }

    impl SdkLibraryGateway for AlwaysSessionErrorGateway {
        fn list_order_products(&self, _params: LibraryItemsParams)
                               -> Result<OrderProductListResponse, LibraryServiceError> {
            unimplemented!("not exercised by download tests")
        }

        fn get_order_product(&self, _id: u64)
                             -> Result<OrderProductItemResponse, LibraryServiceError> {
            unimplemented!("not exercised by download tests")
        }

        fn prepare_download(&self, _order_product_id: u64, _index: u32)
                            -> Result<serde_json::Value, LibraryServiceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(LibraryServiceError::new(LibraryServiceErrorKind::Session,
                                         "stub: simulated session error"))
        }
    }

    #[test]
    fn download_item_succeeds_after_retryable_failures() {
        let (url, server) = spawn_mock_server(vec![MockResponse::ConnectionReset,
                                                   MockResponse::ServerError,
                                                   MockResponse::Success(b"file contents"),]);
        let gateway = CountingGateway { url,
                                        calls: AtomicU32::new(0) };
        let dest = temp_dest("succeeds-after-retries");
        let cancel = AtomicBool::new(false);

        let result = download_item_with_config(&gateway,
                                               1,
                                               0,
                                               &dest,
                                               DownloadControl::new(&cancel),
                                               fast_config(3));

        server.join().unwrap();
        assert!(result.is_ok(), "expected success, got {result:?}");
        assert_eq!(gateway.calls.load(Ordering::SeqCst), 3);
        assert_eq!(std::fs::read(&dest).unwrap(), b"file contents");
        assert!(!part_path_for(&dest).exists());
        let _ = std::fs::remove_file(&dest);
    }

    #[test]
    fn download_item_fails_after_exhausting_all_retryable_attempts() {
        let (url, server) = spawn_mock_server(vec![MockResponse::ConnectionReset,
                                                   MockResponse::ConnectionReset,
                                                   MockResponse::ConnectionReset,]);
        let gateway = CountingGateway { url,
                                        calls: AtomicU32::new(0) };
        let dest = temp_dest("exhausts-retries");
        let cancel = AtomicBool::new(false);

        let result = download_item_with_config(&gateway,
                                               1,
                                               0,
                                               &dest,
                                               DownloadControl::new(&cancel),
                                               fast_config(3));

        server.join().unwrap();
        assert!(result.is_err());
        assert_eq!(gateway.calls.load(Ordering::SeqCst), 3);
        assert!(!dest.exists());
        assert!(!part_path_for(&dest).exists());
    }

    #[test]
    fn download_item_does_not_retry_a_non_retryable_failure() {
        let gateway = AlwaysSessionErrorGateway { calls: AtomicU32::new(0), };
        let dest = temp_dest("non-retryable");
        let cancel = AtomicBool::new(false);

        let result = download_item_with_config(&gateway,
                                               1,
                                               0,
                                               &dest,
                                               DownloadControl::new(&cancel),
                                               fast_config(3));

        assert!(matches!(result, Err(e) if e.kind == LibraryServiceErrorKind::Session));
        assert_eq!(gateway.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn download_item_makes_no_request_when_cancelled_before_start() {
        // Pre-cancelled: neither the retry loop nor `attempt_download` may call
        // `prepare_download`, so the gateway invocation count stays at zero and
        // no authenticated request is made.
        let gateway = CountingGateway { url:   "http://127.0.0.1:1/unused".to_string(),
                                        calls: AtomicU32::new(0), };
        let dest = temp_dest("pre-cancelled");
        let cancel = AtomicBool::new(true);

        let result = download_item_with_config(&gateway,
                                               1,
                                               0,
                                               &dest,
                                               DownloadControl::new(&cancel),
                                               fast_config(3));

        assert!(result.is_err());
        assert_eq!(gateway.calls.load(Ordering::SeqCst), 0);
        assert!(!dest.exists());
        assert!(!part_path_for(&dest).exists());
    }

    #[test]
    fn download_item_stops_retrying_once_cancelled_during_backoff() {
        // Only the first script entry is ever consumed: cancellation during
        // the backoff wait stops the loop before a second attempt is made,
        // so the mock server's background thread is left parked in its
        // second `accept()` for the lifetime of the test process — harmless
        // for a short-lived test binary, and not joined here.
        let (url, _server) =
            spawn_mock_server(vec![MockResponse::ConnectionReset,
                                   MockResponse::Success(b"should never be fetched"),]);
        let gateway = CountingGateway { url,
                                        calls: AtomicU32::new(0) };
        let dest = temp_dest("cancel-during-backoff");
        let cancel = AtomicBool::new(false);

        let mut on_retry = |_attempt: u32, _delay: Duration| {
            cancel.store(true, Ordering::SeqCst);
        };
        let result =
            download_item_with_config(&gateway,
                                      1,
                                      0,
                                      &dest,
                                      DownloadControl::new(&cancel).on_retry(&mut on_retry),
                                      fast_config(3));

        assert!(result.is_err());
        assert_eq!(gateway.calls.load(Ordering::SeqCst), 1);
        assert!(!dest.exists());
        assert!(!part_path_for(&dest).exists());
    }

    #[test]
    fn download_item_produces_rate_limited_error_with_parsed_retry_after() {
        // max_attempts(1) so the single 429 response is never retried — the
        // parsed retry_after value would otherwise become a real wait time
        // in this test (see the `extract_retry_after` override in
        // `download_item_with_config`).
        let (url, server) = spawn_mock_server(vec![MockResponse::RateLimited(30)]);
        let gateway = CountingGateway { url,
                                        calls: AtomicU32::new(0) };
        let dest = temp_dest("rate-limited");
        let cancel = AtomicBool::new(false);

        let result = download_item_with_config(&gateway,
                                               1,
                                               0,
                                               &dest,
                                               DownloadControl::new(&cancel),
                                               fast_config(1));

        server.join().unwrap();
        match result {
            Err(e) => {
                assert_eq!(e.kind, LibraryServiceErrorKind::RateLimited);
                assert_eq!(e.retry_after, Some(Duration::from_secs(30)));
            }
            Ok(()) => panic!("expected a RateLimited error"),
        }
    }

    #[test]
    fn download_item_retries_a_rate_limited_response_like_a_network_failure() {
        let (url, server) = spawn_mock_server(vec![MockResponse::RateLimited(0),
                                                   MockResponse::Success(b"file contents"),]);
        let gateway = CountingGateway { url,
                                        calls: AtomicU32::new(0) };
        let dest = temp_dest("rate-limited-then-succeeds");
        let cancel = AtomicBool::new(false);

        let result = download_item_with_config(&gateway,
                                               1,
                                               0,
                                               &dest,
                                               DownloadControl::new(&cancel),
                                               fast_config(3));

        server.join().unwrap();
        assert!(result.is_ok(), "expected success, got {result:?}");
        assert_eq!(gateway.calls.load(Ordering::SeqCst), 2);
        let _ = std::fs::remove_file(&dest);
    }

    // ── Progress tests ───────────────────────────────────────────────────

    /// Runs a download of `response` and returns every progress report.
    fn progress_of(response: MockResponse) -> Vec<(u64, Option<u64>)> {
        let (url, server) = spawn_mock_server(vec![response]);
        let gateway = CountingGateway { url,
                                        calls: AtomicU32::new(0) };
        let dest = temp_dest("progress");
        let cancel = AtomicBool::new(false);
        let mut reports = Vec::new();
        let mut on_progress = |received: u64, total: Option<u64>| reports.push((received, total));

        let result =
            download_item_with_config(&gateway,
                                      1,
                                      0,
                                      &dest,
                                      DownloadControl::new(&cancel).on_progress(&mut on_progress),
                                      fast_config(1));

        server.join().unwrap();
        assert!(result.is_ok(), "expected success, got {result:?}");
        let _ = std::fs::remove_file(&dest);
        reports
    }

    #[test]
    fn download_item_reports_progress_ending_at_the_full_size() {
        let reports = progress_of(MockResponse::Success(b"file contents"));

        assert_eq!(reports.last(), Some(&(13, Some(13))));
        assert!(reports.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    }

    #[test]
    fn download_item_reports_an_unknown_total_when_the_server_gives_no_length() {
        let reports = progress_of(MockResponse::SuccessWithoutLength(b"file contents"));

        assert_eq!(reports.last(), Some(&(13, None)));
    }

    #[test]
    fn download_item_reports_no_progress_when_cancelled_before_start() {
        let gateway = CountingGateway { url:   "http://127.0.0.1:1/unused".to_string(),
                                        calls: AtomicU32::new(0), };
        let dest = temp_dest("progress-cancelled");
        let cancel = AtomicBool::new(true);
        let mut reports = Vec::new();
        let mut on_progress = |received: u64, total: Option<u64>| reports.push((received, total));

        let result =
            download_item_with_config(&gateway,
                                      1,
                                      0,
                                      &dest,
                                      DownloadControl::new(&cancel).on_progress(&mut on_progress),
                                      fast_config(3));

        assert!(result.is_err());
        assert!(reports.is_empty());
    }
}
