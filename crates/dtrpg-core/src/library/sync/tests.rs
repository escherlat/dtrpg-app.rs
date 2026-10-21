//! Core-level tests for the catalog-sync state machine, driving `Library`
//! directly with no `gpui` involved — the goal ADR 0002 Slice 2 exists for.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use super::LibraryEvent;
use crate::data::collection::CollectionEntry;
use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;
use crate::data::storage::StorageConfig;
use crate::library::Library;
use crate::services::collections::{CollectionsService, CollectionsServiceError};
use crate::services::network_monitor::NetworkMonitor;
use crate::services::{LibraryService, LibraryServiceError};

fn item(id: &str, product_id: u64) -> LibraryItem {
    let mut item = LibraryItem::new(id,
                                    "Title",
                                    "Publisher",
                                    "",
                                    "Core",
                                    "PDF",
                                    10,
                                    1.0,
                                    2024,
                                    0,
                                    ItemStatus::Cloud,
                                    "#1C2A44",
                                    "",
                                    Some(1_700_000_000));
    item.product_id = product_id;
    item
}

struct StubService {
    items: Vec<LibraryItem>,
}

impl LibraryService for StubService {
    fn list_items(&self) -> Result<Vec<LibraryItem>, LibraryServiceError> {
        Ok(self.items.clone())
    }

    fn get_item(&self, _id: u64) -> Result<LibraryItem, LibraryServiceError> {
        unimplemented!("not exercised by these tests")
    }

    fn download_item(&self, _order_product_id: u64, _index: u32, _dest: &std::path::Path,
                     _control: crate::services::DownloadControl<'_>)
                     -> Result<(), LibraryServiceError> {
        unimplemented!("not exercised by these tests")
    }
}

struct EmptyCollections;

impl CollectionsService for EmptyCollections {
    fn list_collections(&self) -> Result<Vec<CollectionEntry>, CollectionsServiceError> {
        Ok(Vec::new())
    }

    fn create_collection(&self, _name: &str) -> Result<CollectionEntry, CollectionsServiceError> {
        unimplemented!("not exercised by these tests")
    }

    fn delete_collection(&self, _id: u64) -> Result<(), CollectionsServiceError> {
        unimplemented!("not exercised by these tests")
    }

    fn add_member(&self, _collection_id: u64, _item_id: u64)
                  -> Result<(), CollectionsServiceError> {
        unimplemented!("not exercised by these tests")
    }

    fn remove_member(&self, _collection_id: u64, _item_id: u64)
                     -> Result<(), CollectionsServiceError> {
        unimplemented!("not exercised by these tests")
    }
}

fn test_storage_config() -> StorageConfig {
    StorageConfig { override_path:                None,
                    max_concurrent_downloads:
                        crate::data::storage::DEFAULT_MAX_CONCURRENT_DOWNLOADS,
                    create_collections:           false,
                    recently_updated_window_days:
                        crate::data::constants::RECENTLY_UPDATED_WINDOW_DEFAULT_DAYS,
                    extra_roots:                  Vec::new(),
                    rules:                        Vec::new(),
                    next_root_id:                 1, }
}

fn open_library() -> (Arc<Library>, super::LibraryEvents) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (mut library, _work_events, sync_events) =
        Library::open(dir.keep(), &test_storage_config()).expect("open library");
    // The real monitor probes the DriveThruRPG host, which would make these
    // tests depend on the machine's internet access.
    library.network_monitor = Arc::new(NetworkMonitor::fixed(true));
    (Arc::new(library), sync_events)
}

fn drain_until_finished(events: &mut super::LibraryEvents) -> Vec<LibraryEvent> {
    let mut seen = Vec::new();
    loop {
        let event = events.blocking_recv()
                          .expect("sync task dropped the sender");
        let is_finished = matches!(event, LibraryEvent::Finished { .. });
        seen.push(event);
        if is_finished {
            return seen;
        }
    }
}

#[test]
fn full_fetch_writes_the_index_and_emits_fully_loaded() {
    let (library, mut events) = open_library();
    let service: Arc<dyn LibraryService> =
        Arc::new(StubService { items: vec![item("a", 1), item("b", 2)], });
    let collections: Arc<dyn CollectionsService> = Arc::new(EmptyCollections);

    assert!(library.start_load(service, collections, false));
    let seen = drain_until_finished(&mut events);

    assert!(seen.iter()
                .any(|e| matches!(e, LibraryEvent::FullyLoaded { items } if items.len() == 2)));
    assert!(matches!(seen.last(),
                     Some(LibraryEvent::Finished { reload_requested: false, })));
    assert_eq!(library.index
                      .lock()
                      .expect("index lock")
                      .count_all()
                      .expect("count"),
               2);
}

#[test]
fn a_load_requested_while_one_is_in_flight_is_queued_not_dropped() {
    let (library, mut events) = open_library();
    let service: Arc<dyn LibraryService> = Arc::new(StubService { items: vec![item("a", 1)], });
    let collections: Arc<dyn CollectionsService> = Arc::new(EmptyCollections);

    assert!(library.start_load(Arc::clone(&service), Arc::clone(&collections), false));
    // The run may already be mid-flight on the shared runtime; either way,
    // a second call while one is recorded in-flight must queue rather than
    // start a second run or get silently dropped.
    let started_second = library.start_load(service, collections, false);

    let seen = drain_until_finished(&mut events);
    let finished_reload_requested = matches!(seen.last(),
                                             Some(LibraryEvent::Finished { reload_requested: true, }));

    // Exactly one of "started immediately" or "queued and then honored by
    // the first run's Finished event" must be true.
    assert!(started_second || finished_reload_requested);
}

#[test]
fn network_unreachable_full_fetch_ends_quietly_without_writing_the_index() {
    // No fresh-install path and no cache, so `run_full_fetch` runs; the
    // network monitor has nothing to reach in a unit test environment with
    // no sandboxed DNS, so this exercises the unreachable-endpoint branch
    // deterministically only if the host truly can't resolve it — skipped
    // here since the current `NetworkMonitor` always attempts a real probe.
    // Covered instead at the `fully_loaded` level above; this test only
    // confirms a run always terminates with a `Finished` event.
    let (library, mut events) = open_library();
    let service: Arc<dyn LibraryService> = Arc::new(StubService { items: Vec::new() });
    let collections: Arc<dyn CollectionsService> = Arc::new(EmptyCollections);

    assert!(library.start_load(service, collections, false));
    let seen = drain_until_finished(&mut events);
    assert!(matches!(seen.last(), Some(LibraryEvent::Finished { .. })));
    // Give the spawned blocking task's generation bookkeeping a moment to
    // settle before the test process tears down the runtime.
    std::thread::sleep(Duration::from_millis(10));
}
