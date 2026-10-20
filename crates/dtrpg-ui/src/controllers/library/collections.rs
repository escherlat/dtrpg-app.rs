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
        if let Err(e) = self.index_mut().replace_all_items(&[]) {
            tracing::error!(error = %e, "failed to clear the library index");
        }
        self.section_counts = SectionCounts::default();
        self.publishers.clear();
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
}

#[path = "collection_membership.rs"]
mod collection_membership;
