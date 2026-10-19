//! Shared fakes and a fixture for the controller's end-to-end tests: a scripted
//! network, a scripted library service, and a real controller wired to both.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{
    collections::HashSet,
    sync::{
        Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use dtrpg_core::{
    data::collection::CollectionEntry,
    services::{
        collections::CollectionsServiceError,
        covers::{CoverFetchError, CoverTransport},
    },
};
use gpui::TestAppContext;

use super::*;
use crate::util::file_presence::resolved_file_path;

pub(super) const WAIT: Duration = Duration::from_secs(5);

/// The title a successful availability check brings back.
pub(super) const REFRESHED_TITLE: &str = "Refreshed";

/// Bytes that sniff as a PNG.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-cover";

/// Holds callers until the test releases a permit for each, so a test decides
/// when each piece of work may finish.
struct Permits {
    available: Mutex<usize>,
    changed:   Condvar,
}

impl Permits {
    fn new(available: usize) -> Self {
        Self { available: Mutex::new(available),
               changed:   Condvar::new(), }
    }

    fn release(&self, count: usize) {
        *self.available.lock().unwrap() += count;
        self.changed.notify_all();
    }

    /// Takes a permit, giving up and returning `false` if `stop` becomes true
    /// first. Fails the test if no permit arrives in time.
    fn take(&self, what: &str, stop: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + WAIT;
        let mut available = self.available.lock().unwrap();
        loop {
            if stop() {
                return false;
            }
            if *available > 0 {
                *available -= 1;
                return true;
            }
            assert!(Instant::now() < deadline,
                    "no permit was released for {what}");
            available = self.changed
                            .wait_timeout(available, Duration::from_millis(5))
                            .unwrap()
                            .0;
        }
    }
}

/// A cover transport that hands out one permit per request, and fails the URLs
/// it is told to.
pub(super) struct ScriptedTransport {
    permits:  Permits,
    failing:  HashSet<String>,
    requests: AtomicUsize,
}

impl ScriptedTransport {
    /// A transport that answers every request immediately.
    pub(super) fn open() -> Arc<Self> {
        Self::with(usize::MAX / 2, &[])
    }

    /// A transport that holds each request until a permit is released.
    pub(super) fn gated() -> Arc<Self> {
        Self::with(0, &[])
    }

    pub(super) fn with(permits: usize, failing: &[&str]) -> Arc<Self> {
        Arc::new(Self { permits:  Permits::new(permits),
                        failing:  failing.iter().map(|url| (*url).to_owned()).collect(),
                        requests: AtomicUsize::new(0), })
    }

    pub(super) fn release(&self, count: usize) {
        self.permits.release(count);
    }

    pub(super) fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl CoverTransport for ScriptedTransport {
    fn is_reachable(&self) -> bool {
        true
    }

    fn get(&self, url: &str) -> Result<Vec<u8>, CoverFetchError> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.permits.take(url, || false);
        if self.failing.contains(url) {
            // No wait between attempts, so a failing fetch is quick.
            return Err(CoverFetchError { message:     "boom".to_owned(),
                                         retry_after: Some(Duration::ZERO), });
        }
        Ok(PNG.to_vec())
    }
}

/// A library service whose downloads hold for a permit each, can fail or report
/// a retry for chosen order products, and notice cancellation. It writes
/// nothing to disk.
pub(super) struct ScriptedLibrary {
    permits:       Permits,
    failing:       HashSet<u64>,
    retrying:      HashSet<u64>,
    started:       Mutex<Vec<u64>>,
    cancelled:     Mutex<Vec<u64>>,
    check_permits: Permits,
    missing:       HashSet<u64>,
    erroring:      HashSet<u64>,
    checked:       Mutex<Vec<u64>>,
    /// A progress report every download makes as soon as it begins.
    progress:      Option<(u64, Option<u64>)>,
    /// Whether every request for the catalog fails as if no one were signed in.
    signed_out:    bool,
}

impl ScriptedLibrary {
    /// A service whose downloads all finish immediately.
    pub(super) fn open() -> Arc<Self> {
        Self::with(usize::MAX / 2, &[], &[])
    }

    /// A service that holds each download until a permit is released.
    pub(super) fn gated() -> Arc<Self> {
        Self::with(0, &[], &[])
    }

    /// A service with `permits` to start, whose downloads fail for the
    /// `failing` order products and report a retry for the `retrying` ones.
    pub(super) fn with(permits: usize, failing: &[u64], retrying: &[u64]) -> Arc<Self> {
        Arc::new(Self { permits:       Permits::new(permits),
                        failing:       failing.iter().copied().collect(),
                        retrying:      retrying.iter().copied().collect(),
                        started:       Mutex::new(Vec::new()),
                        cancelled:     Mutex::new(Vec::new()),
                        check_permits: Permits::new(usize::MAX / 2),
                        missing:       HashSet::new(),
                        erroring:      HashSet::new(),
                        checked:       Mutex::new(Vec::new()),
                        progress:      None,
                        signed_out:    false, })
    }

    /// A service whose downloads each report `received` of `total` bytes as
    /// soon as they begin, then hold for a permit.
    pub(super) fn reporting(received: u64, total: Option<u64>) -> Arc<Self> {
        Arc::new(Self { permits:       Permits::new(0),
                        failing:       HashSet::new(),
                        retrying:      HashSet::new(),
                        started:       Mutex::new(Vec::new()),
                        cancelled:     Mutex::new(Vec::new()),
                        check_permits: Permits::new(usize::MAX / 2),
                        missing:       HashSet::new(),
                        erroring:      HashSet::new(),
                        checked:       Mutex::new(Vec::new()),
                        progress:      Some((received, total)),
                        signed_out:    false, })
    }

    /// A service whose availability checks hold for a permit each. The
    /// `missing` items are reported gone, and the `erroring` ones fail as if
    /// the network were down. Downloads finish immediately.
    pub(super) fn checking(check_permits: usize, missing: &[u64], erroring: &[u64]) -> Arc<Self> {
        Arc::new(Self { permits:       Permits::new(usize::MAX / 2),
                        failing:       HashSet::new(),
                        retrying:      HashSet::new(),
                        started:       Mutex::new(Vec::new()),
                        cancelled:     Mutex::new(Vec::new()),
                        check_permits: Permits::new(check_permits),
                        missing:       missing.iter().copied().collect(),
                        erroring:      erroring.iter().copied().collect(),
                        checked:       Mutex::new(Vec::new()),
                        progress:      None,
                        signed_out:    false, })
    }

    /// A service that fails every request for the catalog, as if no one were
    /// signed in.
    pub(super) fn signed_out() -> Arc<Self> {
        Arc::new(Self { permits:       Permits::new(usize::MAX / 2),
                        failing:       HashSet::new(),
                        retrying:      HashSet::new(),
                        started:       Mutex::new(Vec::new()),
                        cancelled:     Mutex::new(Vec::new()),
                        check_permits: Permits::new(usize::MAX / 2),
                        missing:       HashSet::new(),
                        erroring:      HashSet::new(),
                        checked:       Mutex::new(Vec::new()),
                        progress:      None,
                        signed_out:    true, })
    }

    /// Lets one more availability check finish.
    pub(super) fn release_checks(&self, count: usize) {
        self.check_permits.release(count);
    }

    /// The item number of each availability check that has begun, in order.
    pub(super) fn checked(&self) -> Vec<u64> {
        self.checked.lock().unwrap().clone()
    }

    pub(super) fn release(&self, count: usize) {
        self.permits.release(count);
    }

    /// The order product of each download that has begun, in order.
    pub(super) fn started(&self) -> Vec<u64> {
        self.started.lock().unwrap().clone()
    }

    /// The order product of each download that noticed it was cancelled.
    pub(super) fn cancelled(&self) -> Vec<u64> {
        self.cancelled.lock().unwrap().clone()
    }
}

/// Lets a test keep its own handle to the service the controller owns.
struct SharedLibrary(Arc<ScriptedLibrary>);

impl LibraryService for SharedLibrary {
    fn list_items(&self) -> Result<Vec<LibraryItem>, LibraryServiceError> {
        if self.0.signed_out {
            return Err(LibraryServiceError::new(LibraryServiceErrorKind::Session, "signed out"));
        }
        Ok(Vec::new())
    }

    fn count_items(&self) -> Option<Result<usize, LibraryServiceError>> {
        self.0
            .signed_out
            .then(|| Err(LibraryServiceError::new(LibraryServiceErrorKind::Session, "signed out")))
    }

    fn get_item(&self, id: u64) -> Result<LibraryItem, LibraryServiceError> {
        let library = &self.0;
        library.checked.lock().unwrap().push(id);
        library.check_permits.take("a check", || false);
        if library.missing.contains(&id) {
            return Err(LibraryServiceError::new(LibraryServiceErrorKind::NotFound, "gone"));
        }
        if library.erroring.contains(&id) {
            return Err(LibraryServiceError::new(LibraryServiceErrorKind::Network, "offline"));
        }
        let mut fresh = bare_item("fresh");
        fresh.title = Arc::from(REFRESHED_TITLE);
        Ok(fresh)
    }

    fn download_item(&self, order_product_id: u64, _index: u32, _dest: &Path,
                     mut control: DownloadControl<'_>)
                     -> Result<(), LibraryServiceError> {
        let library = &self.0;
        library.started.lock().unwrap().push(order_product_id);
        if library.retrying.contains(&order_product_id) {
            control.report_retry(2, Duration::from_secs(3));
        }
        if let Some((received, total)) = library.progress {
            control.report_progress(received, total);
        }
        let go = library.permits.take("a download", || {
                                    control.cancel_flag().load(Ordering::SeqCst)
                                });
        if !go {
            library.cancelled.lock().unwrap().push(order_product_id);
            return Err(LibraryServiceError::new(LibraryServiceErrorKind::Network, "cancelled"));
        }
        if library.failing.contains(&order_product_id) {
            return Err(LibraryServiceError::new(LibraryServiceErrorKind::Network, "boom"));
        }
        Ok(())
    }
}

/// Never called: these tests do not touch collections.
struct UnusedCollections;

impl CollectionsService for UnusedCollections {
    fn list_collections(&self) -> Result<Vec<CollectionEntry>, CollectionsServiceError> {
        Err(unused())
    }

    fn create_collection(&self, _name: &str) -> Result<CollectionEntry, CollectionsServiceError> {
        Err(unused())
    }

    fn delete_collection(&self, _id: u64) -> Result<(), CollectionsServiceError> {
        Err(unused())
    }

    fn add_member(&self, _collection_id: u64, _item_id: u64)
                  -> Result<(), CollectionsServiceError> {
        Err(unused())
    }

    fn remove_member(&self, _collection_id: u64, _item_id: u64)
                     -> Result<(), CollectionsServiceError> {
        Err(unused())
    }
}

fn unused() -> CollectionsServiceError {
    CollectionsServiceError::new(CollectionsServiceErrorKind::Network, "unused in this test")
}

pub(super) fn url_for(id: &str) -> Arc<str> {
    Arc::from(format!("https://example.com/{id}.png"))
}

fn bare_item(id: &str) -> LibraryItem {
    LibraryItem::new(id,
                     id,
                     "Publisher",
                     "Line",
                     "Core",
                     "PDF",
                     0,
                     0.0,
                     2020,
                     0,
                     ItemStatus::Cloud,
                     "#000000",
                     "",
                     None)
}

/// An entry with a cover URL and no files.
pub(super) fn item_with_cover(id: &str) -> LibraryItem {
    let mut item = bare_item(id);
    item.cover_url = Some(url_for(id));
    item
}

/// An entry the server knows as item number `numeric_id`, with no files.
pub(super) fn item_numbered(id: &str, numeric_id: u64) -> LibraryItem {
    let mut item = bare_item(id);
    item.numeric_id = numeric_id;
    item
}

/// An entry with `files` undownloaded files, belonging to `order_product_id`.
pub(super) fn item_with_files(id: &str, order_product_id: u64, files: u32) -> LibraryItem {
    let mut item = bare_item(id);
    item.order_product_id = order_product_id;
    item.files = (0..files).map(|index| LibraryItemFile { id: format!("{id}-{index}").into(),
                                                          index,
                                                          name:
                                                              format!("File {index}.pdf").into(),
                                                          format: "PDF".into(),
                                                          size_mb: 1.0,
                                                          downloaded: false,
                                                          offloaded: false })
                           .collect();
    item
}

/// A real controller wired to the scripted fakes.
pub(super) struct Fixture {
    pub(super) controller: Entity<LibraryController>,
    pub(super) activity:   Entity<ActivityController>,
    pub(super) transport:  Arc<ScriptedTransport>,
    pub(super) library:    Arc<ScriptedLibrary>,
    /// The directory the catalog cache is saved to.
    pub(super) cache:      PathBuf,
    /// Keeps the temporary directories alive for the length of the test.
    _dirs:                 Vec<tempfile::TempDir>,
}

/// Builds a controller holding entries with covers for `ids`, fetching covers
/// through `transport` and running at most `limit` jobs at once.
pub(super) fn fixture(cx: &mut TestAppContext, transport: Arc<ScriptedTransport>, limit: usize,
                      ids: &[&str])
                      -> Fixture {
    let items = ids.iter().map(|id| item_with_cover(id)).collect();
    fixture_with(cx, transport, ScriptedLibrary::open(), limit, items)
}

/// Builds a controller holding `items`, fetching covers through `transport`,
/// downloading through `library`, and running at most `limit` jobs at once.
pub(super) fn fixture_with(cx: &mut TestAppContext, transport: Arc<ScriptedTransport>,
                           library: Arc<ScriptedLibrary>, limit: usize, items: Vec<LibraryItem>)
                           -> Fixture {
    cx.update(|cx| cx.set_global(CoverCache::new()));
    let covers = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let cache_path = cache.path().to_path_buf();
    let activity = cx.new(|_| ActivityController::new());
    let controller =
        cx.new(|cx| {
              let (mut ctrl, events) =
                  LibraryController::build(Box::new(SharedLibrary(Arc::clone(&library))),
                                           Box::new(UnusedCollections),
                                           activity.clone());
              ctrl.cover_fetcher = Arc::new(CoverFetcher::new(Arc::clone(&transport)
                                                              as Arc<dyn CoverTransport>,
                                                              covers.path().to_path_buf()));
              ctrl.cache_root = cache_path.clone();
              // A test must not wait on, or depend on, the real network.
              ctrl.network_monitor = Arc::new(NetworkMonitor::fixed(true));
              ctrl.work.set_limit(limit);
              ctrl.catalog = items;
              ctrl.start_work_event_pump(events, cx);
              ctrl
          });
    Fixture { controller,
              activity,
              transport,
              library,
              cache: cache_path,
              _dirs: vec![covers, cache] }
}

/// Lets the event loop run until `done` holds. Work runs on other threads, so
/// this polls rather than waiting on the test scheduler alone.
pub(super) fn wait_for(cx: &mut TestAppContext, mut done: impl FnMut(&mut TestAppContext) -> bool) {
    cx.executor().allow_parking();
    let deadline = Instant::now() + WAIT;
    loop {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(Instant::now() < deadline,
                "condition not met within {WAIT:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn cached(cx: &mut TestAppContext, id: &str) -> bool {
    cx.update(|cx| cx.global::<CoverCache>().get(id).is_some())
}

pub(super) fn in_flight(cx: &mut TestAppContext, id: &str) -> bool {
    cx.update(|cx| cx.global::<CoverCache>().is_in_flight(id))
}

/// How many activity entries are running and how many have finished.
pub(super) fn activity_counts(fixture: &Fixture, cx: &mut TestAppContext) -> (usize, usize) {
    fixture.activity.read_with(cx, |activity, _| {
                        let snapshot = activity.snapshot();
                        (snapshot.in_progress_count, snapshot.recent_count)
                    })
}

/// Puts the file of `item` at `index` on disk where the app would have put it.
pub(super) fn put_on_disk(storage: &StorageConfig, item: &LibraryItem, index: usize) -> PathBuf {
    let path = resolved_file_path(storage, item, &item.files[index]);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"data").unwrap();
    path
}
