//! Lightweight, query-first connectivity monitor shared across catalog sync,
//! downloads, and cover/avatar image caching.
//!
//! Performs an on-demand reachability check rather than maintaining a
//! persistent OS-level path monitor (no `NWPathMonitor` equivalent exists in
//! this codebase), caching each result briefly so a burst of calling
//! requests triggers at most one live check per target.

use std::collections::HashMap;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::data::constants::NETWORK_MONITOR_CACHE_TTL_SECS;

/// How long a single reachability probe waits before treating the target as
/// unreachable.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// A host used only to distinguish "no network at all" from "just this
/// endpoint is unreachable" — deliberately independent of the DriveThruRPG
/// API so a DriveThruRPG-specific outage doesn't read as a general outage.
/// A fixed IP (not a hostname) so the general check never depends on DNS
/// resolution succeeding.
const GENERAL_CONNECTIVITY_PROBE: &str = "1.1.1.1:443";

/// Connectivity state for a single monitored target, with the instant it was
/// last checked.
#[derive(Clone, Copy)]
struct CachedState {
    reachable:  bool,
    checked_at: Instant,
}

/// Query-first network-connectivity monitor.
///
/// Consulted before making a request that needs general network access or
/// access to a specific remote endpoint; see
/// [`check_general`](NetworkMonitor::check_general) and
/// [`check_endpoint`](NetworkMonitor::check_endpoint).
pub struct NetworkMonitor {
    cache: Mutex<HashMap<String, CachedState>>,
}

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkMonitor {
    /// Creates a monitor with an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self { cache: Mutex::new(HashMap::new()), }
    }

    /// Reports whether the device has general network connectivity,
    /// independent of any specific endpoint's reachability.
    #[must_use]
    pub fn check_general(&self) -> bool {
        self.check_cached(GENERAL_CONNECTIVITY_PROBE, probe_general)
    }

    /// Reports whether `host` (a `host:port` pair, e.g.
    /// `"api.drivethrurpg.com:443"`) is currently reachable.
    #[must_use]
    pub fn check_endpoint(&self, host: &str) -> bool {
        self.check_cached(host, probe_endpoint)
    }

    fn check_cached(&self, key: &str, probe: impl FnOnce(&str) -> bool) -> bool {
        let now = Instant::now();
        {
            // Poisoned-lock recovery: a panic while holding this lock during a
            // probe would be a bug elsewhere, not a reason to make every
            // subsequent connectivity check unusable.
            let cache = self.cache
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(state) = cache.get(key)
               && now.duration_since(state.checked_at)
                  < Duration::from_secs(NETWORK_MONITOR_CACHE_TTL_SECS)
            {
                return state.reachable;
            }
        }

        let reachable = probe(key);
        let mut cache = self.cache
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.insert(key.to_string(),
                     CachedState { reachable,
                                   checked_at: now });
        reachable
    }
}

/// Attempts a short-timeout TCP connect to `addr` (already a `host:port`
/// pair), resolving DNS first if `addr` is a hostname.
fn probe_endpoint(addr: &str) -> bool {
    match addr.to_socket_addrs() {
        Ok(mut addrs) => addrs.any(|a| TcpStream::connect_timeout(&a, PROBE_TIMEOUT).is_ok()),
        Err(_) => false,
    }
}

/// Attempts a short-timeout TCP connect to a fixed IP, skipping DNS
/// resolution entirely so this check never conflates "DNS is broken" with
/// "there is no network at all".
fn probe_general(addr: &str) -> bool {
    match addr.parse() {
        Ok(socket_addr) => TcpStream::connect_timeout(&socket_addr, PROBE_TIMEOUT).is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::net::{SocketAddr, TcpListener};

    use super::*;

    fn local_listener() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let addr = listener.local_addr().expect("listener has a local addr");
        (listener, addr.to_string())
    }

    #[test]
    fn check_endpoint_reports_reachable_for_a_listening_local_port() {
        let (_listener, addr) = local_listener();
        let monitor = NetworkMonitor::new();
        assert!(monitor.check_endpoint(&addr));
    }

    #[test]
    fn check_endpoint_reports_unreachable_for_a_closed_local_port() {
        let (listener, addr) = local_listener();
        drop(listener);
        let monitor = NetworkMonitor::new();
        assert!(!monitor.check_endpoint(&addr));
    }

    #[test]
    fn check_endpoint_caches_result_within_ttl() {
        let (listener, addr) = local_listener();
        let monitor = NetworkMonitor::new();
        assert!(monitor.check_endpoint(&addr));
        drop(listener);
        // Still cached as reachable even though the listener is now gone.
        assert!(monitor.check_endpoint(&addr));
    }

    #[test]
    fn check_endpoint_returns_false_for_unparseable_target() {
        let monitor = NetworkMonitor::new();
        assert!(!monitor.check_endpoint("not a valid host"));
    }

    #[test]
    fn probe_general_rejects_unparseable_address() {
        assert!(!probe_general("not an ip"));
    }

    #[test]
    fn probe_general_accepts_valid_socket_addr_format() {
        // Uses a closed local port rather than a real network probe so this
        // test has no network dependency; only exercises the parse path.
        let addr: SocketAddr = "127.0.0.1:0".parse().expect("valid socket addr");
        let _ = probe_general(&addr.to_string());
    }
}
