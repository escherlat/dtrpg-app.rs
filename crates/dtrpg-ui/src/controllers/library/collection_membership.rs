//! Collection membership and filesystem-link updates.

use dtrpg_core::library::RemovedMember;

use super::*;

impl LibraryController {
    /// Returns the catalog item that `item_id` or `product_id` identify, if it
    /// is in the catalog.
    fn catalog_item_for_member(&self, item_id: u64, product_id: u64) -> Option<LibraryItem> {
        self.all_items().into_iter().find(|item| {
                                        collection_member_id(item) == item_id
                                        || (product_id > 0 && item.product_id == product_id)
                                    })
    }

    /// Removes the links to a downloaded item from the folder of the collection
    /// it has just left, so the folder shows only what is in the collection.
    ///
    /// `collections_before` is what the item belonged to before it left, which
    /// decides where its files are expected to be. Only links that point at the
    /// item's files are removed. Does nothing for an item that is not in the
    /// catalog or not downloaded.
    fn unlink_item_from_removed_collection(&self, item_id: u64, product_id: u64,
                                           collection_name: Arc<str>,
                                           collections_before: Arc<Vec<CollectionEntry>>,
                                           cx: &mut Context<Self>) {
        let Some(item) = self.catalog_item_for_member(item_id, product_id)
        else {
            return;
        };
        let storage = self.storage_override.clone();
        cx.background_executor()
          .spawn(async move {
              let storage = storage.unwrap_or_else(|| Arc::new(StorageConfig::load()));
              unlink_item_from_collection(&storage, &item, &collections_before, &collection_name);
          })
          .detach();
    }

    /// Puts the files of the item that are already on disk in the folder of the
    /// collection it was just added to, as the download would have if the item
    /// had been a member then, when "Create collections" is on.
    ///
    /// Does nothing for an item that is not in the catalog or not downloaded.
    fn link_item_into_added_collection(&self, item_id: u64, product_id: u64, collection_id: u64,
                                       cx: &mut Context<Self>) {
        let Some(item) = self.catalog_item_for_member(item_id, product_id)
        else {
            return;
        };
        let collections = self.library.collections();
        let Some(collection) = collections.iter().find(|c| c.id == collection_id)
        else {
            return;
        };
        let collection_name = Arc::clone(&collection.name);
        let storage = self.storage_override.clone();
        cx.background_executor()
          .spawn(async move {
              let storage = storage.unwrap_or_else(|| Arc::new(StorageConfig::load()));
              if storage.create_collections() {
                  link_item_into_collection(&storage, &item, &collections, &collection_name);
              }
          })
          .detach();
    }

    /// Adds `item_id` (an item's `order_product_id`/`product_id`) as a member
    /// of the collection with `collection_id`.
    ///
    /// `item_id` drives the local optimistic `member_ids`/`collection_members`
    /// update and its rollback-on-failure branch, matching the id space
    /// `CollectionEntry::member_ids` already uses. `product_id` is the item's
    /// catalog `product_id`, sent as the network add call's product
    /// identifier — the API's `product_list_items` endpoint rejects
    /// `order_product_id` values with an invalid-product-id error.
    ///
    /// Updates `member_ids` (and `collection_members`, if that collection is
    /// the active filter) immediately, then confirms the change via the
    /// service. On failure the optimistic update is rolled back and
    /// [`CollectionMemberAddFailed`] is emitted so the window can show an
    /// error notification.
    pub fn add_item_to_collection(&mut self, collection_id: u64, item_id: u64, product_id: u64,
                                  cx: &mut Context<Self>) {
        let Some(collection_name) = self.library
                                        .add_collection_member(collection_id, item_id, product_id)
        else {
            return;
        };

        if matches!(&self.filter, SidebarFilter::Collection(id, _) if *id == collection_id) {
            self.collection_members.insert(item_id);
        }
        cx.emit(LibraryChanged);

        let collections_service = Arc::clone(&self.collections_service);
        cx.spawn(async move |this, async_cx| {
              let result = async_cx.background_executor()
                                   .spawn(async move {
                                       collections_service.add_member(collection_id, product_id)
                                   })
                                   .await;
              // Once the server agrees the item is in the collection, put an
              // item that is already downloaded in the collection's folder too.
              // A conflict means it was already a member, which is the same.
              let is_member = match &result {
                  Ok(()) => true,
                  Err(e) => e.kind == CollectionsServiceErrorKind::Conflict,
              };
              if is_member {
                  this.update(async_cx, |ctrl, cx| {
                          ctrl.link_item_into_added_collection(item_id, product_id, collection_id, cx);
                      })
                      .ok();
              }
              if let Err(e) = result {
                  // A conflict means the item is already a member server-side: the
                  // optimistic local update already matches reality, so it is left
                  // in place rather than rolled back, and this is surfaced as a
                  // low-severity notice instead of a hard failure.
                  if e.kind == CollectionsServiceErrorKind::Conflict {
                      this.update(async_cx, |_ctrl, cx| {
                              cx.emit(CollectionMemberAlreadyPresent { message: e.message.clone(), });
                          })
                          .ok();
                      return;
                  }
                  this.update(async_cx, |ctrl, cx| {
                          ctrl.library.revert_added_collection_member(collection_id, item_id);
                          if matches!(&ctrl.filter, SidebarFilter::Collection(id, _) if *id == collection_id)
                          {
                              ctrl.collection_members.remove(&item_id);
                          }
                          cx.emit(LibraryChanged);
                          ctrl.activity.update(cx, |a, cx| {
                                           a.log_alert(format!(
                                               "Add to collection '{collection_name}'"
                                           ),
                                                       e.message.clone(),
                                                       cx);
                                       });
                          cx.emit(CollectionMemberAddFailed { message: e.message.clone(), });
                      })
                      .ok();
              }
          })
          .detach();
    }

    /// Removes `item_id` (an item's `order_product_id`/`product_id`) as a
    /// member of the collection with `collection_id`.
    ///
    /// `item_id` drives matching against the local optimistic `member_ids`/
    /// `collection_members` cache, which may hold either id depending on how
    /// the entry was populated (server data is `product_id`-keyed; a locally
    /// added entry may still carry `item_id`). `product_id` is the item's
    /// catalog `product_id`, sent as the network removal call's product
    /// identifier — the API's server-side lookup that resolves the removal
    /// target matches by `product_id`, never `order_product_id`, so sending
    /// the wrong one causes the item to silently not be found.
    ///
    /// Updates `member_ids` (and `collection_members`, if that collection is
    /// the active filter) immediately, then confirms the change via the
    /// service. On failure the optimistic update is rolled back and
    /// [`CollectionMemberRemoveFailed`] is emitted so the window can show an
    /// error notification.
    pub fn remove_item_from_collection(&mut self, collection_id: u64, item_id: u64,
                                       product_id: u64, cx: &mut Context<Self>) {
        let collections_before = self.library.collections();
        // Whichever id(s) are actually present are removed and remembered, so
        // that a failed removal restores exactly what was there before rather
        // than introducing an id that was never actually cached.
        let Some(RemovedMember { collection_name,
                                 removed_ids, }) =
            self.library
                .remove_collection_member(collection_id, item_id, product_id)
        else {
            return;
        };

        if matches!(&self.filter, SidebarFilter::Collection(id, _) if *id == collection_id) {
            self.collection_members.remove(&item_id);
            self.collection_members.remove(&product_id);
        }
        cx.emit(LibraryChanged);

        let collections_service = Arc::clone(&self.collections_service);
        cx.spawn(async move |this, async_cx| {
              let result = async_cx.background_executor()
                                   .spawn(async move {
                                       collections_service.remove_member(collection_id, product_id)
                                   })
                                   .await;
              match result {
                  Ok(()) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.unlink_item_from_removed_collection(item_id,
                                                                       product_id,
                                                                       collection_name,
                                                                       collections_before,
                                                                       cx);
                          })
                          .ok();
                  }
                  Err(e) => {
                  this.update(async_cx, |ctrl, cx| {
                          ctrl.library.restore_removed_collection_member(collection_id, &removed_ids);
                          if matches!(&ctrl.filter, SidebarFilter::Collection(id, _) if *id == collection_id)
                          {
                              ctrl.collection_members.extend(&removed_ids);
                          }
                          cx.emit(LibraryChanged);
                          ctrl.activity.update(cx, |a, cx| {
                                           a.log_alert(format!(
                                               "Remove from collection '{collection_name}'"
                                           ),
                                                       e.message.clone(),
                                                       cx);
                                       });
                          cx.emit(CollectionMemberRemoveFailed { message: e.message.clone(), });
                      })
                      .ok();
                  }
              }
          })
          .detach();
    }
}
