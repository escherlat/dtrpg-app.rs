//! Collection CRUD, membership changes, and the collections service swap.

use super::collection_links::{link_item_into_collection, unlink_item_from_collection};
use super::*;

impl LibraryController {
    /// Spawns a background task to fetch product lists from the API and apply
    /// them.
    pub fn load_collections(&mut self, cx: &mut Context<Self>) {
        let collections_service = Arc::clone(&self.collections_service);
        let cache_root = cache_dir();
        let weak_activity = self.activity.downgrade();
        let activity_id = self.activity.update(cx, |a, cx| {
                                           a.start(&t!("activity.loading_collections"), None, cx)
                                       });
        tracing::debug!("load_collections: starting collections fetch");
        cx.spawn(async move |this, async_cx| {
              let result = async_cx.background_executor()
                                   .spawn(async move { collections_service.list_collections() })
                                   .await;
              match result {
                  Ok(entries) => {
                      tracing::debug!(count = entries.len(),
                                      "load_collections: fetched {} entries",
                                      entries.len());
                      let to_save = entries.clone();
                      let save_root = cache_root.clone();
                      async_cx
                        .background_executor()
                        .spawn(async move {
                            if let Err(e) = save_collections_cache(&save_root, &to_save) {
                                tracing::warn!(error = %e, "failed to save collections cache");
                            }
                        })
                        .await;
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.apply_collections(entries, cx);
                          })
                          .ok();
                      weak_activity.update(async_cx, |a, cx| a.complete(activity_id, cx))
                                   .ok();
                  }
                  Err(e) => {
                      // Session errors are expected when starting before auth
                      // completes
                      // (see `start_load_inner`'s matching treatment for the
                      // catalog fetch); quietly complete
                      // rather than surfacing a user-facing error.
                      if e.kind == CollectionsServiceErrorKind::Session {
                          tracing::warn!(error = %e,
                                      "collections load skipped: no authenticated session");
                          weak_activity.update(async_cx, |a, cx| a.complete(activity_id, cx))
                                       .ok();
                      }
                      else {
                          tracing::warn!(error = %e, "collections load failed");
                          weak_activity.update(async_cx, |a, cx| {
                                           a.error(activity_id, e.to_string(), cx)
                                       })
                                       .ok();
                      }
                  }
              }
          })
          .detach();
    }

    /// Stores the fetched collections and emits a change event.
    pub(super) fn apply_collections(&mut self, mut collections: Vec<CollectionEntry>,
                                    cx: &mut Context<Self>) {
        let catalog_ids = self.catalog_ids();
        sort_collections(&mut collections,
                         self.collection_sort,
                         self.collection_sort_direction,
                         &catalog_ids);
        self.collections = collections;
        self.collections_loaded = true;
        // Refresh collection_members if the current filter is a Collection
        // filter, in case the filter was set before the collections
        // loaded.
        if let SidebarFilter::Collection(id, _) = &self.filter {
            let id = *id;
            self.collection_members = self.collections
                                          .iter()
                                          .find(|c| c.id == id)
                                          .map(|c| c.member_ids.iter().copied().collect())
                                          .unwrap_or_default();
        }
        cx.emit(LibraryChanged);
    }

    /// Returns the current pane state from the service layer.
    pub fn pane_state(&self) -> &LibraryPaneState {
        self.vm.pane_state()
    }

    /// Replaces the backing services and triggers a fresh catalog and
    /// collections load.
    ///
    /// Clears the activity panel so stale error messages from the previous
    /// (unauthenticated) service do not persist after sign-in.
    ///
    /// Sets `catalog_loading` before clearing the catalog so the catalog view
    /// shows its loading spinner instead of the "library empty" state for the
    /// gap between this clear and `start_load` repopulating it — otherwise a
    /// catalog already populated from disk cache (e.g. during startup's
    /// silent re-authentication) flashes empty before reappearing.
    pub fn replace_service(&mut self, service: Box<dyn LibraryService>,
                           collections_service: Box<dyn CollectionsService>,
                           cx: &mut Context<Self>) {
        tracing::debug!("replace_service: installing authenticated services");
        self.vm.replace_service(service);
        self.collections_service = Arc::from(collections_service);
        self.catalog_loading = true;
        self.catalog.clear();
        self.section_counts =
            section_counts(&self.catalog, now_secs(), self.recently_updated_window_days);
        self.publishers = publisher_entries(&self.catalog);
        self.collections.clear();
        self.collections_loaded = false;
        self.collection_members.clear();
        self.selection = Selection::default();
        self.invalidate_cache();
        self.activity.update(cx, |a, cx| a.clear(cx));
        cx.emit(LibraryChanged);
        // Collections are loaded as the first stage of `start_load`, sharing
        // its single activity item rather than starting a separate one.
        self.start_load(cx);
    }

    /// Starts a background task to create a new collection with the given name.
    ///
    /// Tracks progress in the activity panel. On success the new entry is
    /// appended and [`LibraryChanged`] is emitted. On failure
    /// [`CollectionCreateFailed`] is emitted so the window can push an
    /// error notification.
    pub fn create_collection(&mut self, name: String, cx: &mut Context<Self>) {
        let label = t!("activity.creating_collection", name = name);
        let activity_id = self.activity.update(cx, |a, cx| a.start(&label, None, cx));

        let collections_service = Arc::clone(&self.collections_service);
        cx.spawn(async move |this, async_cx| {
              let result =
                  async_cx.background_executor()
                          .spawn(async move { collections_service.create_collection(&name) })
                          .await;

              match result {
                  Ok(entry) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.collections.push(entry);
                              ctrl.activity
                                  .update(cx, |a, cx| a.complete(activity_id, cx));
                              cx.emit(LibraryChanged);
                          })
                          .ok();
                  }
                  Err(e) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.activity.update(cx, |a, cx| {
                                               a.error(activity_id, e.to_string(), cx);
                                           });
                              cx.emit(CollectionCreateFailed { message: e.message.clone(), });
                          })
                          .ok();
                  }
              }
          })
          .detach();
    }

    /// Creates a new collection with the given name and, once the create call
    /// succeeds, immediately adds `item_id` as a member of it.
    ///
    /// `item_id` drives local membership tracking/matching (the optimistic
    /// `member_ids` update); `product_id` is the catalog product id sent on
    /// the network add call. See [`Self::add_item_to_collection`].
    ///
    /// Used by the "New collection…" affordance in the Manage Collections
    /// dialog, so a single user action both creates the collection and adds
    /// the current item to it. On create failure, only
    /// [`CollectionCreateFailed`] is emitted — no add is attempted.
    pub fn create_collection_and_add_member(&mut self, name: String, item_id: u64,
                                            product_id: u64, cx: &mut Context<Self>) {
        let label = t!("activity.creating_collection", name = name);
        let activity_id = self.activity.update(cx, |a, cx| a.start(&label, None, cx));

        let collections_service = Arc::clone(&self.collections_service);
        cx.spawn(async move |this, async_cx| {
              let result =
                  async_cx.background_executor()
                          .spawn(async move { collections_service.create_collection(&name) })
                          .await;

              match result {
                  Ok(entry) => {
                      let collection_id = entry.id;
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.collections.push(entry);
                              ctrl.activity
                                  .update(cx, |a, cx| a.complete(activity_id, cx));
                              cx.emit(LibraryChanged);
                              ctrl.add_item_to_collection(collection_id, item_id, product_id, cx);
                          })
                          .ok();
                  }
                  Err(e) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.activity.update(cx, |a, cx| {
                                               a.error(activity_id, e.to_string(), cx);
                                           });
                              cx.emit(CollectionCreateFailed { message: e.message.clone(), });
                          })
                          .ok();
                  }
              }
          })
          .detach();
    }

    /// Deletes the collection with the given id via the service, then removes
    /// it locally.
    ///
    /// Logs failures to the activity panel and leaves the collection in place
    /// on error.
    pub fn delete_collection(&mut self, id: u64, cx: &mut Context<Self>) {
        let label = t!("activity.deleting_collection").to_string();
        let activity_id = self.activity.update(cx, |a, cx| a.start(&label, None, cx));
        let collections_service = Arc::clone(&self.collections_service);
        cx.spawn(async move |this, async_cx| {
              let result = async_cx.background_executor()
                                   .spawn(async move { collections_service.delete_collection(id) })
                                   .await;
              match result {
                  Ok(()) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.collections.retain(|c| c.id != id);
                              if let SidebarFilter::Collection(cid, _) = &ctrl.filter
                                 && *cid == id
                              {
                                  ctrl.filter = SidebarFilter::default();
                                  ctrl.collection_members.clear();
                              }
                              ctrl.activity
                                  .update(cx, |a, cx| a.complete(activity_id, cx));
                              cx.emit(LibraryChanged);
                          })
                          .ok();
                  }
                  Err(e) => {
                      this.update(async_cx, |ctrl, cx| {
                              ctrl.activity.update(cx, |a, cx| {
                                               a.error(activity_id, e.to_string(), cx);
                                           });
                          })
                          .ok();
                  }
              }
          })
          .detach();
    }

    /// Returns the catalog item that `item_id` or `product_id` identify, if it
    /// is in the catalog.
    fn catalog_item_for_member(&self, item_id: u64, product_id: u64) -> Option<LibraryItem> {
        self.catalog
            .iter()
            .find(|item| {
                collection_member_id(item) == item_id
                || (product_id > 0 && item.product_id == product_id)
            })
            .cloned()
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
                                           collections_before: Vec<CollectionEntry>,
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
        let Some(collection) = self.collections.iter().find(|c| c.id == collection_id)
        else {
            return;
        };
        let collection_name = Arc::clone(&collection.name);
        let collections = self.collections.clone();
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
        let Some(collection) = self.collections.iter_mut().find(|c| c.id == collection_id)
        else {
            return;
        };
        if member_ids_contain(&collection.member_ids, item_id, product_id) {
            return;
        }
        let collection_name = collection.name.clone();
        let mut member_ids: Vec<u64> = collection.member_ids.iter().copied().collect();
        member_ids.push(item_id);
        collection.member_ids = Arc::from(member_ids.as_slice());

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
                          if let Some(collection) =
                              ctrl.collections.iter_mut().find(|c| c.id == collection_id)
                          {
                              let member_ids: Vec<u64> = collection.member_ids
                                                                   .iter()
                                                                   .copied()
                                                                   .filter(|id| *id != item_id)
                                                                   .collect();
                              collection.member_ids = Arc::from(member_ids.as_slice());
                          }
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
        let collections_before = self.collections.clone();
        let Some(collection) = self.collections.iter_mut().find(|c| c.id == collection_id)
        else {
            return;
        };
        if !member_ids_contain(&collection.member_ids, item_id, product_id) {
            return;
        }
        let collection_name = collection.name.clone();
        // Remove whichever id(s) are actually present, and remember them so a
        // failed removal restores exactly what was there before rather than
        // introducing an id that was never actually cached.
        let removed_ids: Vec<u64> = collection.member_ids
                                              .iter()
                                              .copied()
                                              .filter(|id| *id == item_id || *id == product_id)
                                              .collect();
        let member_ids: Vec<u64> = collection.member_ids
                                             .iter()
                                             .copied()
                                             .filter(|id| *id != item_id && *id != product_id)
                                             .collect();
        collection.member_ids = Arc::from(member_ids.as_slice());

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
                          if let Some(collection) =
                              ctrl.collections.iter_mut().find(|c| c.id == collection_id)
                          {
                              let mut member_ids: Vec<u64> = collection.member_ids
                                                                       .iter()
                                                                       .copied()
                                                                       .collect();
                              member_ids.extend(&removed_ids);
                              collection.member_ids = Arc::from(member_ids.as_slice());
                          }
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
