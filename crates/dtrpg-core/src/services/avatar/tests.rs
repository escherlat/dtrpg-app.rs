//! Tests for avatar fetching, against a fake transport and a local HTTP server.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use reqwest::blocking::Client;

use super::{AvatarFetchError, AvatarFetcher, AvatarTransport, transport::get_bytes};
use crate::services::retry::RetryConfig;

const AVATAR: &[u8] = b"avatar-bytes";

/// Answers each `get` from a script and counts how often it was asked.
struct FakeTransport {
    reachable: bool,
    responses: Mutex<VecDeque<Result<Vec<u8>, AvatarFetchError>>>,
    gets:      AtomicUsize,
}

impl FakeTransport {
    fn new(reachable: bool, responses: Vec<Result<Vec<u8>, AvatarFetchError>>) -> Arc<Self> {
        Arc::new(Self { reachable,
                        responses: Mutex::new(responses.into()),
                        gets: AtomicUsize::new(0) })
    }

    fn gets(&self) -> usize {
        self.gets.load(Ordering::SeqCst)
    }
}

impl AvatarTransport for FakeTransport {
    fn is_reachable(&self) -> bool {
        self.reachable
    }

    fn get(&self, _url: &str) -> Result<Vec<u8>, AvatarFetchError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("fake transport ran out of responses")
    }
}

/// A fetcher that retries without waiting, so retry tests are quick.
fn fetcher(transport: &Arc<FakeTransport>, dir: &tempfile::TempDir) -> AvatarFetcher {
    AvatarFetcher::new(Arc::clone(transport) as Arc<dyn AvatarTransport>,
                       dir.path().join("avatar")).with_retry(RetryConfig { max_attempts: 3,
                                                                           base_secs:    0,
                                                                           max_secs:     0, })
}

fn transient() -> AvatarFetchError {
    AvatarFetchError::Transient("timed out".to_owned())
}

#[test]
fn a_cache_miss_downloads_and_saves_the_avatar() {
    let dir = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(true, vec![Ok(AVATAR.to_vec())]);

    let bytes = fetcher(&transport, &dir).fetch("someone@example.com");

    assert_eq!(bytes.as_deref(), Some(AVATAR));
    assert_eq!(std::fs::read(dir.path().join("avatar")).unwrap(), AVATAR);
}

#[test]
fn a_cache_hit_makes_no_request() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("avatar"), AVATAR).unwrap();
    let transport = FakeTransport::new(true, vec![]);

    let bytes = fetcher(&transport, &dir).fetch("someone@example.com");

    assert_eq!(bytes.as_deref(), Some(AVATAR));
    assert_eq!(transport.gets(), 0);
}

#[test]
fn an_empty_cache_file_counts_as_a_miss() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("avatar"), b"").unwrap();
    let transport = FakeTransport::new(true, vec![Ok(AVATAR.to_vec())]);

    let bytes = fetcher(&transport, &dir).fetch("someone@example.com");

    assert_eq!(bytes.as_deref(), Some(AVATAR));
    assert_eq!(transport.gets(), 1);
}

#[test]
fn an_unreachable_host_gives_no_avatar_without_a_request() {
    let dir = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(false, vec![]);

    assert_eq!(fetcher(&transport, &dir).fetch("someone@example.com"), None);

    assert_eq!(transport.gets(), 0);
}

#[test]
fn a_transient_failure_is_retried() {
    let dir = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(true, vec![Err(transient()), Ok(AVATAR.to_vec())]);

    let bytes = fetcher(&transport, &dir).fetch("someone@example.com");

    assert_eq!(bytes.as_deref(), Some(AVATAR));
    assert_eq!(transport.gets(), 2);
}

#[test]
fn transient_failures_stop_after_the_maximum_attempts() {
    let dir = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(true,
                                       vec![Err(transient()), Err(transient()), Err(transient())]);

    assert_eq!(fetcher(&transport, &dir).fetch("someone@example.com"), None);

    assert_eq!(transport.gets(), 3);
    assert!(!dir.path().join("avatar").exists());
}

#[test]
fn a_final_answer_is_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(true, vec![Err(AvatarFetchError::Final)]);

    assert_eq!(fetcher(&transport, &dir).fetch("someone@example.com"), None);

    assert_eq!(transport.gets(), 1);
}

#[test]
fn the_cache_directory_is_created_when_missing() {
    let dir = tempfile::tempdir().unwrap();
    let transport = FakeTransport::new(true, vec![Ok(AVATAR.to_vec())]);
    let nested = dir.path().join("not").join("yet").join("avatar");
    let fetcher = AvatarFetcher::new(Arc::clone(&transport) as Arc<dyn AvatarTransport>,
                                     nested.clone());

    let _ = fetcher.fetch("someone@example.com");

    assert_eq!(std::fs::read(nested).unwrap(), AVATAR);
}

/// Serves one canned HTTP response, then stops, returning the URL to request.
fn serve_once(response: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/avatar", listener.local_addr().unwrap());
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

    assert_eq!(get_bytes(&direct_client(), &url).unwrap(), b"hello");
}

#[test]
fn http_not_found_is_final() {
    let url =
        serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");

    assert_eq!(get_bytes(&direct_client(), &url),
               Err(AvatarFetchError::Final));
}

#[test]
fn http_server_error_is_final() {
    let url = serve_once("HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");

    assert_eq!(get_bytes(&direct_client(), &url),
               Err(AvatarFetchError::Final));
}

#[test]
fn http_connection_failure_is_transient() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/avatar", listener.local_addr().unwrap());
    drop(listener);

    assert!(matches!(get_bytes(&direct_client(), &url),
                     Err(AvatarFetchError::Transient(_))));
}
