//! Tests for cover fetching, against a fake transport and a local HTTP server.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use reqwest::blocking::Client;

use super::{CoverFetchError, CoverFetcher, CoverTransport, transport::get_bytes};
use crate::data::{constants::IMAGE_CACHE_MAX_ATTEMPTS, cover_cache::load_cached_cover};

/// Bytes that sniff as a PNG.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-cover";

/// Answers each `get` from a script and counts how often it was asked.
struct FakeTransport {
    reachable: bool,
    responses: Mutex<VecDeque<Result<Vec<u8>, CoverFetchError>>>,
    gets:      AtomicUsize,
}

impl FakeTransport {
    fn new(reachable: bool, responses: Vec<Result<Vec<u8>, CoverFetchError>>) -> Arc<Self> {
        Arc::new(Self { reachable,
                        responses: Mutex::new(responses.into()),
                        gets: AtomicUsize::new(0) })
    }

    fn gets(&self) -> usize {
        self.gets.load(Ordering::SeqCst)
    }
}

impl CoverTransport for FakeTransport {
    fn is_reachable(&self) -> bool {
        self.reachable
    }

    fn get(&self, _url: &str) -> Result<Vec<u8>, CoverFetchError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("fake transport ran out of responses")
    }
}

fn fetcher(transport: &Arc<FakeTransport>, root: &tempfile::TempDir) -> CoverFetcher {
    CoverFetcher::new(Arc::clone(transport) as Arc<dyn CoverTransport>,
                      root.path().to_path_buf())
}

fn not_cancelled() -> AtomicBool {
    AtomicBool::new(false)
}

/// A failure that asks for no wait, so retries do not slow the tests down.
fn rate_limited() -> CoverFetchError {
    CoverFetchError { message:     "rate limited (HTTP 429)".to_owned(),
                      retry_after: Some(Duration::ZERO), }
}

#[test]
fn a_cache_miss_downloads_and_saves_the_cover() {
    let root = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(true, vec![Ok(PNG.to_vec())]);

    let bytes = fetcher(&transport, &root).fetch("item", "https://x/c", false, &not_cancelled());

    assert_eq!(bytes.unwrap(), PNG);
    assert_eq!(load_cached_cover(root.path(), "item"), Some(PNG.to_vec()));
}

#[test]
fn a_cache_hit_makes_no_request() {
    let root = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(true, vec![Ok(PNG.to_vec())]);
    let fetcher = fetcher(&transport, &root);
    fetcher.fetch("item", "https://x/c", false, &not_cancelled())
           .unwrap();

    let bytes = fetcher.fetch("item", "https://x/c", false, &not_cancelled());

    assert_eq!(bytes.unwrap(), PNG);
    assert_eq!(transport.gets(), 1);
}

#[test]
fn forcing_the_network_skips_the_cache_and_replaces_it() {
    let root = tempfile::tempdir().unwrap();
    let newer = b"\x89PNG\r\n\x1a\n-newer".to_vec();
    let transport = FakeTransport::new(true, vec![Ok(PNG.to_vec()), Ok(newer.clone())]);
    let fetcher = fetcher(&transport, &root);
    fetcher.fetch("item", "https://x/c", false, &not_cancelled())
           .unwrap();

    let bytes = fetcher.fetch("item", "https://x/c", true, &not_cancelled());

    assert_eq!(bytes.unwrap(), newer);
    assert_eq!(load_cached_cover(root.path(), "item"), Some(newer));
}

#[test]
fn an_unreachable_host_fails_without_a_request() {
    let root = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(false, vec![]);

    let error = fetcher(&transport, &root).fetch("item", "https://x/c", false, &not_cancelled())
                                          .unwrap_err();

    assert!(error.message.contains("unreachable"));
    assert_eq!(transport.gets(), 0);
}

#[test]
fn a_cache_hit_works_while_offline() {
    let root = tempfile::tempdir().unwrap();
    let online = FakeTransport::new(true, vec![Ok(PNG.to_vec())]);
    fetcher(&online, &root).fetch("item", "https://x/c", false, &not_cancelled())
                           .unwrap();
    let offline = FakeTransport::new(false, vec![]);

    let bytes = fetcher(&offline, &root).fetch("item", "https://x/c", false, &not_cancelled());

    assert_eq!(bytes.unwrap(), PNG);
}

#[test]
fn a_failed_request_is_retried() {
    let root = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(true, vec![Err(rate_limited()), Ok(PNG.to_vec())]);

    let bytes = fetcher(&transport, &root).fetch("item", "https://x/c", false, &not_cancelled());

    assert_eq!(bytes.unwrap(), PNG);
    assert_eq!(transport.gets(), 2);
}

#[test]
fn retries_stop_after_the_maximum_attempts() {
    let root = tempfile::tempdir().unwrap();
    let failures = (0..IMAGE_CACHE_MAX_ATTEMPTS).map(|_| Err(rate_limited()))
                                                .collect();
    let transport = FakeTransport::new(true, failures);

    let error = fetcher(&transport, &root).fetch("item", "https://x/c", false, &not_cancelled())
                                          .unwrap_err();

    assert_eq!(error, rate_limited());
    assert_eq!(transport.gets(), IMAGE_CACHE_MAX_ATTEMPTS as usize);
    assert_eq!(load_cached_cover(root.path(), "item"), None);
}

#[test]
fn a_cancelled_fetch_makes_no_request() {
    let root = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(true, vec![]);

    let error =
        fetcher(&transport, &root).fetch("item", "https://x/c", false, &AtomicBool::new(true))
                                  .unwrap_err();

    assert!(error.message.contains("cancelled"));
    assert_eq!(transport.gets(), 0);
}

/// Serves one canned HTTP response, then stops, returning the URL to request.
fn serve_once(response: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/cover.png", listener.local_addr().unwrap());
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        stream.write_all(response.as_bytes()).unwrap();
    });
    url
}

/// A client that ignores any proxy in the environment, so a request to the
/// local test server is not sent elsewhere.
fn direct_client() -> Client {
    Client::builder().no_proxy().build().unwrap()
}

#[test]
fn http_returns_the_response_body() {
    let url = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello");

    let bytes = get_bytes(&direct_client(), &url);

    assert_eq!(bytes.unwrap(), b"hello");
}

#[test]
fn http_429_reports_the_retry_after_wait() {
    let url = serve_once("HTTP/1.1 429 Too Many Requests\r\nRetry-After: 7\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");

    let error = get_bytes(&direct_client(), &url).unwrap_err();

    assert_eq!(error.retry_after, Some(Duration::from_secs(7)));
    assert!(error.message.contains("429"));
}

#[test]
fn http_429_without_retry_after_reports_no_wait() {
    let url = serve_once("HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");

    let error = get_bytes(&direct_client(), &url).unwrap_err();

    assert_eq!(error.retry_after, None);
}

#[test]
fn http_connection_failure_is_an_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/cover.png", listener.local_addr().unwrap());
    drop(listener);

    assert!(get_bytes(&direct_client(), &url).is_err());
}
