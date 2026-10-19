//! Tests for the symlinks that put a downloaded file in each of its
//! collections' folders: made when the file downloads, and again when an item
//! that is already downloaded is added to another collection.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use dtrpg_core::data::storage::collection_dir;
use dtrpg_core::services::collections::CollectionsServiceError;
use gpui::TestAppContext;

use super::collection_links::root_of;
#[cfg(unix)]
use super::collection_links::{link_into_collection, unlink_from_collection};
use super::{tests_support::*, *};

/// What the service says when asked to add a member.
#[derive(Clone, Copy)]
enum Answer {
    Accepts,
    Rejects,
    /// Only the unix tests make a link after this answer.
    #[cfg(unix)]
    AlreadyAMember,
}

/// A collections service that answers every add the same way and counts them.
struct Scripted {
    answer: Answer,
    adds:   AtomicUsize,
}

impl CollectionsService for Scripted {
    fn list_collections(&self) -> Result<Vec<CollectionEntry>, CollectionsServiceError> {
        Ok(Vec::new())
    }

    fn create_collection(&self, _name: &str) -> Result<CollectionEntry, CollectionsServiceError> {
        Err(CollectionsServiceError::new(CollectionsServiceErrorKind::Network, "unused"))
    }

    fn delete_collection(&self, _id: u64) -> Result<(), CollectionsServiceError> {
        Ok(())
    }

    fn add_member(&self, _collection_id: u64, _item_id: u64)
                  -> Result<(), CollectionsServiceError> {
        self.adds.fetch_add(1, Ordering::SeqCst);
        match self.answer {
            Answer::Accepts => Ok(()),
            Answer::Rejects => {
                Err(CollectionsServiceError::new(CollectionsServiceErrorKind::Network, "no"))
            }
            #[cfg(unix)]
            Answer::AlreadyAMember => {
                Err(CollectionsServiceError::new(CollectionsServiceErrorKind::Conflict, "already"))
            }
        }
    }

    fn remove_member(&self, _collection_id: u64, _item_id: u64)
                     -> Result<(), CollectionsServiceError> {
        match self.answer {
            Answer::Rejects => {
                Err(CollectionsServiceError::new(CollectionsServiceErrorKind::Network, "no"))
            }
            _ => Ok(()),
        }
    }
}

const COLLECTION_ID: u64 = 7;
const COLLECTION_NAME: &str = "Thursday Game";

/// A controller holding item `a` (order product 1), a collection it is not in,
/// storage under a temporary folder with "Create collections" as `create`, and
/// a service that answers adds as `answer`.
struct Setup {
    fixture: Fixture,
    service: Arc<Scripted>,
    storage: Arc<StorageConfig>,
    item:    LibraryItem,
    _root:   tempfile::TempDir,
}

fn setup(cx: &mut TestAppContext, create: bool, answer: Answer) -> Setup {
    let root = tempfile::tempdir().unwrap();
    let storage = Arc::new(StorageConfig::for_test(root.path().to_path_buf())
                               .with_create_collections(create));
    let item = item_with_files("a", 1, 1);
    let fixture = fixture_with(cx,
                               ScriptedTransport::open(),
                               ScriptedLibrary::open(),
                               3,
                               vec![item.clone()]);
    let service = Arc::new(Scripted { answer,
                                      adds: AtomicUsize::new(0) });
    let (for_controller, shared) = (Arc::clone(&service), Arc::clone(&storage));
    fixture.controller.update(cx, move |ctrl, _| {
                          ctrl.collections_service = for_controller;
                          ctrl.storage_override = Some(shared);
                          ctrl.collections
                              .push(CollectionEntry { id:         COLLECTION_ID,
                                                      name:       COLLECTION_NAME.into(),
                                                      member_ids: Arc::from([]),
                                                      created_at: 0, });
                      });
    Setup { fixture,
            service,
            storage,
            item,
            _root: root }
}

impl Setup {
    /// Puts the item's file where a download would have, and adds the item to
    /// the collection.
    fn add_downloaded_item(&self, cx: &mut TestAppContext) -> PathBuf {
        let dest = put_on_disk(&self.storage, &self.item, 0);
        self.fixture.controller.update(cx, |ctrl, cx| {
                                   ctrl.add_item_to_collection(COLLECTION_ID, 1, 0, cx)
                               });
        dest
    }

    fn link(&self) -> PathBuf {
        collection_dir(&self.storage.root_path(), COLLECTION_NAME).join("File_0.pdf")
    }

    /// Waits until the service has been asked, and then long enough that a link
    /// that was going to be made has been.
    fn settle(&self, cx: &mut TestAppContext) {
        wait_for(cx, |_| self.service.adds.load(Ordering::SeqCst) == 1);
        cx.executor().allow_parking();
        std::thread::sleep(std::time::Duration::from_millis(300));
        cx.run_until_parked();
    }
}

#[cfg(unix)]
#[gpui::test]
fn an_item_that_is_already_downloaded_gets_its_link_when_added_to_another_collection(cx: &mut TestAppContext)
{
    let setup = setup(cx, true, Answer::Accepts);

    let dest = setup.add_downloaded_item(cx);
    wait_for(cx, |_| setup.link().symlink_metadata().is_ok());

    assert_eq!(std::fs::read_link(setup.link()).unwrap(), dest);
}

#[cfg(unix)]
#[gpui::test]
fn the_link_is_made_when_the_item_was_already_a_member_on_the_server(cx: &mut TestAppContext) {
    let setup = setup(cx, true, Answer::AlreadyAMember);

    setup.add_downloaded_item(cx);
    wait_for(cx, |_| setup.link().symlink_metadata().is_ok());
}

#[gpui::test]
fn no_link_is_made_when_create_collections_is_off(cx: &mut TestAppContext) {
    let setup = setup(cx, false, Answer::Accepts);

    setup.add_downloaded_item(cx);
    setup.settle(cx);

    assert!(setup.link().symlink_metadata().is_err());
}

#[gpui::test]
fn no_link_is_made_when_the_server_refuses_the_add(cx: &mut TestAppContext) {
    let setup = setup(cx, true, Answer::Rejects);

    setup.add_downloaded_item(cx);
    setup.settle(cx);

    assert!(setup.link().symlink_metadata().is_err());
}

#[gpui::test]
fn an_item_that_is_not_downloaded_gets_no_link_and_no_folder(cx: &mut TestAppContext) {
    let setup = setup(cx, true, Answer::Accepts);

    setup.fixture.controller.update(cx, |ctrl, cx| {
                                ctrl.add_item_to_collection(COLLECTION_ID, 1, 0, cx)
                            });
    setup.settle(cx);

    assert!(!collection_dir(&setup.storage.root_path(), COLLECTION_NAME).exists());
}

// ── The link itself ──────────────────────────────────────────────────────────

#[test]
fn a_files_storage_root_is_three_levels_above_it() {
    let dest = Path::new("/lib/items/Publisher/Book.pdf");

    assert_eq!(root_of(dest), Some(PathBuf::from("/lib")));
    assert_eq!(root_of(Path::new("Book.pdf")), None);
}

#[cfg(unix)]
#[test]
fn a_link_is_made_in_the_collections_folder_and_making_it_again_is_harmless() {
    let root = tempfile::tempdir().unwrap();
    let dest = root.path().join("items").join("Publisher").join("Book.pdf");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::write(&dest, b"data").unwrap();

    link_into_collection(&dest, root.path(), "Thursday Game");
    link_into_collection(&dest, root.path(), "Thursday Game");

    let link = root.path()
                   .join("collections")
                   .join("Thursday_Game")
                   .join("Book.pdf");
    assert_eq!(std::fs::read_link(link).unwrap(), dest);
}

#[cfg(unix)]
#[test]
fn an_items_files_on_disk_are_linked_into_the_collection_it_is_in() {
    use super::collection_links::link_item_into_collection;

    let root = tempfile::tempdir().unwrap();
    let storage = StorageConfig::for_test(root.path().to_path_buf());
    let item = item_with_files("a", 1, 2);
    let on_disk = put_on_disk(&storage, &item, 1);
    let collection = CollectionEntry { id:         COLLECTION_ID,
                                       name:       COLLECTION_NAME.into(),
                                       member_ids: Arc::from([1]),
                                       created_at: 0, };

    let looked_at = link_item_into_collection(&storage, &item, &[collection], COLLECTION_NAME);

    assert_eq!(looked_at, 1);
    let link = collection_dir(root.path(), COLLECTION_NAME).join("File_1.pdf");
    assert_eq!(std::fs::read_link(link).unwrap(), on_disk);
}

// ── Removing the link ────────────────────────────────────────────────────────

#[cfg(unix)]
impl Setup {
    /// Removes the item from the collection.
    fn remove_item(&self, cx: &mut TestAppContext) {
        self.fixture.controller.update(cx, |ctrl, cx| {
                                   ctrl.remove_item_from_collection(COLLECTION_ID, 1, 0, cx)
                               });
    }
}

#[cfg(unix)]
#[gpui::test]
fn removing_an_item_from_a_collection_removes_its_link(cx: &mut TestAppContext) {
    let setup = setup(cx, true, Answer::Accepts);
    setup.add_downloaded_item(cx);
    wait_for(cx, |_| setup.link().symlink_metadata().is_ok());

    setup.remove_item(cx);

    wait_for(cx, |_| setup.link().symlink_metadata().is_err());
}

#[cfg(unix)]
#[gpui::test]
fn the_link_stays_when_the_server_refuses_the_removal(cx: &mut TestAppContext) {
    let setup = setup(cx, true, Answer::Rejects);
    let dest = put_on_disk(&setup.storage, &setup.item, 0);
    link_into_collection(&dest, &setup.storage.root_path(), COLLECTION_NAME);
    setup.fixture.controller.update(cx, |ctrl, _| {
                                ctrl.collections[0].member_ids = Arc::from([1]);
                            });

    setup.remove_item(cx);
    cx.executor().allow_parking();
    std::thread::sleep(std::time::Duration::from_millis(300));
    cx.run_until_parked();

    assert!(setup.link().symlink_metadata().is_ok());
}

#[cfg(unix)]
#[test]
fn only_a_link_to_the_file_is_removed() {
    let root = tempfile::tempdir().unwrap();
    let dest = root.path().join("items").join("Publisher").join("Book.pdf");
    std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::fs::write(&dest, b"data").unwrap();
    let folder = collection_dir(root.path(), COLLECTION_NAME);
    std::fs::create_dir_all(&folder).unwrap();
    let link = folder.join("Book.pdf");

    // Nothing there yet.
    assert!(!unlink_from_collection(&dest, root.path(), COLLECTION_NAME));

    // A file with the same name is not a link.
    std::fs::write(&link, b"mine").unwrap();
    assert!(!unlink_from_collection(&dest, root.path(), COLLECTION_NAME));
    assert_eq!(std::fs::read(&link).unwrap(), b"mine");
    std::fs::remove_file(&link).unwrap();

    // A link to somewhere else is not Libri's.
    let elsewhere = root.path().join("Other.pdf");
    std::fs::write(&elsewhere, b"other").unwrap();
    std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
    assert!(!unlink_from_collection(&dest, root.path(), COLLECTION_NAME));
    assert!(link.symlink_metadata().is_ok());
    std::fs::remove_file(&link).unwrap();

    // A link to the file goes, and the file stays.
    link_into_collection(&dest, root.path(), COLLECTION_NAME);
    assert!(unlink_from_collection(&dest, root.path(), COLLECTION_NAME));
    assert!(link.symlink_metadata().is_err());
    assert!(dest.is_file());
}
