//! Download queue management and request entry points.

use super::collection_links::{link_into_collection, root_of};
use super::*;

impl LibraryController {
    // ── Download queue
    // ────────────────────────────────────────────────────────

    /// Reverts a `Downloaded` entry to `Cloud` status.
    ///
    /// This is "Remove Download" — clearing every already-downloaded item in
    /// the entry — not a cancellation of an in-flight fetch. Use
    /// [`Self::cancel_download`] to cancel a queued or in-progress download.
    /// Entry status is never set directly (see
    /// `LibraryItem::recompute_status`): every file's `downloaded` flag is
    /// cleared first, then the entry status is re-derived.
    pub fn remove_download(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(mut item) = self.item_by_id(id)
           && item.status == ItemStatus::Downloaded
        {
            for file in &mut item.files {
                file.downloaded = false;
                file.offloaded = false;
            }
            item.recompute_status();
            if let Err(e) = self.index_mut().replace_item(&item) {
                tracing::error!(error = %e, "failed to write item status to the library index");
            }
            self.refresh_section_counts();
            self.invalidate_cache();
            self.file_info_cache
                .retain(|(entry_id, _), _| entry_id.as_ref() != id);
            cx.emit(LibraryChanged);
        }
    }

    /// Queues a download for every not-yet-downloaded file in the entry with
    /// the given id, dispatching as many as there are free concurrency
    /// slots. For a single-item entry this enqueues its one file, matching
    /// prior behavior; for a multi-item entry it enqueues each missing item
    /// as an independent download.
    ///
    /// No-op for files that are already `Downloaded` or already
    /// queued/in flight — see [`Self::enqueue_item_download`].
    pub fn enqueue_download(&mut self, id: &str, title: impl Into<String>, cx: &mut Context<Self>) {
        let title = title.into();
        let Some(indices) = self.item_by_id(id)
                                .map(|item| missing_file_indices(&item.files))
        else {
            return;
        };
        for index in indices {
            self.enqueue_item_download(id, index, title.clone(), cx);
        }
    }

    /// Queues a download for every not-yet-downloaded item that is a member
    /// of the collection with `collection_id`, reusing
    /// [`Self::enqueue_download`] per matching item.
    ///
    /// No-op if the collection id doesn't exist or has no matching catalog
    /// items. Already-downloaded, already-queued, and already-active
    /// items/files are left untouched — see [`Self::enqueue_download`].
    pub fn download_all_for_collection(&mut self, collection_id: u64, cx: &mut Context<Self>) {
        let Some(collection) = self.collections.iter().find(|c| c.id == collection_id)
        else {
            return;
        };
        let member_ids = Arc::clone(&collection.member_ids);
        let all = self.all_items();
        let targets = collection_download_targets(&all, &member_ids);
        for (id, title) in targets {
            self.enqueue_download(&id, title.to_string(), cx);
        }
    }

    /// Queues a download for every not-yet-downloaded item under the given
    /// publisher, reusing [`Self::enqueue_download`] per matching item.
    ///
    /// No-op if the publisher has no matching catalog items.
    /// Already-downloaded, already-queued, and already-active items/files
    /// are left untouched — see [`Self::enqueue_download`].
    pub fn download_all_for_publisher(&mut self, publisher: &str, cx: &mut Context<Self>) {
        let all = self.all_items();
        let targets = publisher_download_targets(&all, publisher);
        for (id, title) in targets {
            self.enqueue_download(&id, title.to_string(), cx);
        }
    }

    /// Queues a download for every not-yet-downloaded item matching `filter`
    /// — the sidebar smart filters (All Titles, Recently Updated, On This
    /// Device, In the Cloud) and Collection, reusing [`Self::enqueue_download`]
    /// per matching item. Publisher rows use
    /// [`Self::download_all_for_publisher`] directly.
    ///
    /// No-op if `filter` matches no catalog items. Already-downloaded,
    /// already-queued, and already-active items/files are left untouched —
    /// see [`Self::enqueue_download`].
    pub fn download_all_for_filter(&mut self, filter: &SidebarFilter, cx: &mut Context<Self>) {
        let all = self.all_items();
        let targets = filter_download_targets(&all,
                                              filter,
                                              &self.collection_members,
                                              now_secs(),
                                              self.recently_updated_window_days);
        for (id, title) in targets {
            self.enqueue_download(&id, title.to_string(), cx);
        }
    }

    /// Queues a single file's download, dispatching it immediately if a
    /// concurrency slot is free.
    ///
    /// No-op if the item/file does not exist, the file is already
    /// downloaded, or that specific file is already queued/in flight.
    pub fn enqueue_item_download(&mut self, id: &str, index: u32, title: impl Into<String>,
                                 cx: &mut Context<Self>) {
        let Some(item) = self.item_by_id(id).filter(|item| {
                                                item.files
                                                    .get(index as usize)
                                                    .is_some_and(|f| !f.downloaded)
                                            })
        else {
            return;
        };
        if self.download_jobs.has_file(id, index) {
            return;
        }
        self.submit_download(item, index, title.into());
        cx.emit(LibraryChanged);
    }

    /// Gates [`Self::enqueue_download`] behind a free-disk-space check.
    ///
    /// Computes the entry's aggregate missing-file size and compares it to
    /// [`dtrpg_core::data::storage::available_space_mb`]. If space is
    /// sufficient or can't be determined, queues immediately; otherwise
    /// stashes the request and emits [`LowDiskSpaceWarning`] instead of
    /// queuing anything.
    pub fn request_download(&mut self, id: &str, title: impl Into<String>, cx: &mut Context<Self>) {
        let title = title.into();
        let Some(item) = self.item_by_id(id)
        else {
            return;
        };
        let needed_mb = missing_files_size_mb(&item.files);
        if let Some(free_mb) = self.warn_if_insufficient_space_for_item(&item, needed_mb) {
            self.pending_download = Some(PendingDownloadRequest::Item { id: Arc::from(id),
                                                                        title });
            cx.emit(LowDiskSpaceWarning { needed_mb, free_mb });
            return;
        }
        self.enqueue_download(id, title, cx);
    }

    /// Gates [`Self::enqueue_item_download`] behind a free-disk-space check.
    ///
    /// Same pattern as [`Self::request_download`], but for a single file.
    pub fn request_item_download(&mut self, id: &str, index: u32, title: impl Into<String>,
                                 cx: &mut Context<Self>) {
        let title = title.into();
        let Some(item) = self.item_by_id(id)
        else {
            return;
        };
        let Some(needed_mb) = item.files
                                  .get(index as usize)
                                  .filter(|f| !f.downloaded)
                                  .map(|f| f.size_mb)
        else {
            return;
        };
        if let Some(free_mb) = self.warn_if_insufficient_space_for_item(&item, needed_mb) {
            self.pending_download = Some(PendingDownloadRequest::ItemFile { id: Arc::from(id),
                                                                            index,
                                                                            title });
            cx.emit(LowDiskSpaceWarning { needed_mb, free_mb });
            return;
        }
        self.enqueue_item_download(id, index, title, cx);
    }

    /// Gates [`Self::download_all_for_collection`] behind a free-disk-space
    /// check.
    ///
    /// The check is evaluated once as an aggregate across every matching
    /// item's missing files, not per item — see [`Self::request_download`].
    pub fn request_download_all_for_collection(&mut self, collection_id: u64,
                                               cx: &mut Context<Self>) {
        let Some(collection) = self.collections.iter().find(|c| c.id == collection_id)
        else {
            return;
        };
        let member_ids = Arc::clone(&collection.member_ids);
        let all = self.all_items();
        let targets = collection_download_targets(&all, &member_ids);
        let needed_mb =
            missing_files_size_mb_for_items(&all, targets.iter().map(|(id, _)| id.as_ref()));
        if let Some(free_mb) = Self::warn_if_insufficient_space(needed_mb) {
            self.pending_download = Some(PendingDownloadRequest::Collection { collection_id });
            cx.emit(LowDiskSpaceWarning { needed_mb, free_mb });
            return;
        }
        self.download_all_for_collection(collection_id, cx);
    }

    /// Gates [`Self::download_all_for_publisher`] behind a free-disk-space
    /// check.
    ///
    /// The check is evaluated once as an aggregate across every matching
    /// item's missing files, not per item — see [`Self::request_download`].
    pub fn request_download_all_for_publisher(&mut self, publisher: &str, cx: &mut Context<Self>) {
        let all = self.all_items();
        let targets = publisher_download_targets(&all, publisher);
        let needed_mb =
            missing_files_size_mb_for_items(&all, targets.iter().map(|(id, _)| id.as_ref()));
        if let Some(free_mb) = Self::warn_if_insufficient_space(needed_mb) {
            self.pending_download =
                Some(PendingDownloadRequest::Publisher { publisher: Arc::from(publisher), });
            cx.emit(LowDiskSpaceWarning { needed_mb, free_mb });
            return;
        }
        self.download_all_for_publisher(publisher, cx);
    }

    /// Gates [`Self::download_all_for_filter`] behind a free-disk-space
    /// check.
    ///
    /// The check is evaluated once as an aggregate across every matching
    /// item's missing files, not per item — see [`Self::request_download`].
    pub fn request_download_all_for_filter(&mut self, filter: SidebarFilter,
                                           cx: &mut Context<Self>) {
        let all = self.all_items();
        let targets = filter_download_targets(&all,
                                              &filter,
                                              &self.collection_members,
                                              now_secs(),
                                              self.recently_updated_window_days);
        let needed_mb =
            missing_files_size_mb_for_items(&all, targets.iter().map(|(id, _)| id.as_ref()));
        if let Some(free_mb) = Self::warn_if_insufficient_space(needed_mb) {
            self.pending_download = Some(PendingDownloadRequest::Filter { filter });
            cx.emit(LowDiskSpaceWarning { needed_mb, free_mb });
            return;
        }
        self.download_all_for_filter(&filter, cx);
    }

    /// Returns `Some(free_mb)` if `needed_mb` exceeds the free space at the
    /// storage root; `None` if space is sufficient or can't be determined
    /// (fail open — see `download-disk-space-check`).
    pub(super) fn warn_if_insufficient_space(needed_mb: f64) -> Option<f64> {
        let free_mb = dtrpg_core::data::storage::available_space_mb()?;
        (needed_mb > free_mb).then_some(free_mb)
    }

    /// Applies the same free-space check to the root selected by placement
    /// rules for `item`. Bulk requests retain their aggregate default-root
    /// check; single-item requests can safely use the exact destination root.
    pub(super) fn warn_if_insufficient_space_for_item(&self, item: &LibraryItem, needed_mb: f64)
                                                      -> Option<f64> {
        let storage = StorageConfig::load();
        let collection_ids = collection_ids_for_item(item, &self.collections);
        let root = storage.resolve_root_for(&item.publisher, &collection_ids, item.size_mb);
        let free_mb = available_space_mb_at(&root.path)?;
        (needed_mb > free_mb).then_some(free_mb)
    }

    /// Queues the download stashed in `pending_download` (if any) via the
    /// corresponding unconditional method, then clears it.
    ///
    /// Calls the unconditional method directly — never a `request_*`
    /// wrapper — so confirming can't re-trigger the same space check and
    /// loop back into another warning.
    pub fn confirm_pending_download(&mut self, cx: &mut Context<Self>) {
        match self.pending_download.take() {
            Some(PendingDownloadRequest::Item { id, title }) => {
                self.enqueue_download(&id, title, cx);
            }
            Some(PendingDownloadRequest::ItemFile { id, index, title }) => {
                self.enqueue_item_download(&id, index, title, cx);
            }
            Some(PendingDownloadRequest::Collection { collection_id }) => {
                self.download_all_for_collection(collection_id, cx);
            }
            Some(PendingDownloadRequest::Publisher { publisher }) => {
                self.download_all_for_publisher(&publisher, cx);
            }
            Some(PendingDownloadRequest::Filter { filter }) => {
                self.download_all_for_filter(&filter, cx);
            }
            None => {}
        }
    }

    /// Clears a stashed `pending_download` without queuing anything.
    pub fn cancel_pending_download(&mut self, _cx: &mut Context<Self>) {
        self.pending_download = None;
    }

    /// Cancels a single file's download that has not yet completed.
    ///
    /// A download still waiting in the work queue is removed and never
    /// starts; no activity entry existed yet for it. One that is running has
    /// its job cancelled and its activity panel entry removed immediately; the
    /// job observes the cancellation at its next checkpoint, so the file is
    /// not marked `downloaded`, and the queue frees its slot. Other files
    /// queued or in flight for the same entry are unaffected.
    ///
    /// No-op if `(id, index)` is not queued or in flight.
    pub fn cancel_download(&mut self, id: &str, index: u32, cx: &mut Context<Self>) {
        let key = (Arc::<str>::from(id), index);
        let Some((job_id, activity_id)) = self.download_jobs.cancel(&key)
        else {
            return;
        };
        self.work().cancel(job_id);
        if let Some(activity_id) = activity_id {
            self.activity
                .update(cx, |a, cx| a.remove_in_progress(activity_id, cx));
        }
        cx.emit(LibraryChanged);
    }

    /// Returns `true` if any file of catalog entry `id` is currently queued or
    /// actively downloading.
    #[must_use]
    pub fn is_downloading(&self, id: &str) -> bool {
        self.download_jobs.has_item(id)
    }

    /// Returns `true` if the file at `(id, index)` is currently queued or
    /// actively downloading.
    ///
    /// Used by the detail tab's per-item list to show an in-progress
    /// indicator on a row instead of its download/downloaded affordance.
    #[must_use]
    pub fn is_file_queued_or_active(&self, id: &str, index: u32) -> bool {
        self.download_jobs.has_file(id, index)
    }
}

pub(crate) fn create_collections_symlinks(dest: &Path, order_product_id: u64, product_id: u64,
                                          collections: &[CollectionEntry]) {
    let storage = StorageConfig::load();
    if !storage.create_collections() {
        return;
    }
    let root = root_of(dest).unwrap_or_else(|| storage.root_path());
    for collection in collections {
        if member_ids_contain(&collection.member_ids, order_product_id, product_id) {
            link_into_collection(dest, &root, &collection.name);
        }
    }
}
