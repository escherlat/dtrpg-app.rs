//! Tests for the download transfer, against a mock HTTP server.

#![allow(clippy::unwrap_used, clippy::expect_used)]

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
    std::env::temp_dir().join(format!("dtrpg-download-test-{}-{name}-{unique}", std::process::id()))
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

    fn get_order_product(&self, _id: u64) -> Result<OrderProductItemResponse, LibraryServiceError> {
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

    fn get_order_product(&self, _id: u64) -> Result<OrderProductItemResponse, LibraryServiceError> {
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
    let (url, _server) = spawn_mock_server(vec![MockResponse::ConnectionReset,
                                                MockResponse::Success(b"should never be fetched"),]);
    let gateway = CountingGateway { url,
                                    calls: AtomicU32::new(0) };
    let dest = temp_dest("cancel-during-backoff");
    let cancel = AtomicBool::new(false);

    let mut on_retry = |_attempt: u32, _delay: Duration| {
        cancel.store(true, Ordering::SeqCst);
    };
    let result = download_item_with_config(&gateway,
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
