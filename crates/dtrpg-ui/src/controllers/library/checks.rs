//! Item-level download-availability checks (single item and batch).

use super::*;

impl LibraryController {
    /// Verifies `id`'s files against disk in the background and applies any
    /// resulting `downloaded`/`status` change, per
    /// `verify-downloaded-status-against-disk`.
    ///
    /// Independent of [`Self::maybe_check_item`]'s network-bound
    /// availability check (and its cooldown): a missing or reappeared file
    /// on disk is a local fact unrelated to whether the server still lists
    /// the item, and a local `Path::exists()` call per file is cheap enough
    /// that gating it adds complexity without a real cost problem to solve.
    pub(super) fn verify_selected_item_download(&mut self, id: Arc<str>, cx: &mut Context<Self>) {
        let Some(item) = self.item_by_id(&id).cloned()
        else {
            return;
        };
        let collection_ids = collection_ids_for_item(&item, &self.collections);
        self.verifying_downloads.insert(Arc::clone(&id));
        cx.emit(LibraryChanged);

        cx.spawn(async move |this, async_cx| {
              let verification = async_cx.background_executor()
                                         .spawn(async move {
                                             find_presence_changes_in_collections(&item,
                                                                   &StorageConfig::load(),
                                                                   &collection_ids)
                                         })
                                         .await;

              this.update(async_cx, |ctrl, cx| {
                      ctrl.verifying_downloads.remove(&id);
                      if let Some(verification) = verification {
                          ctrl.apply_verifications(&[verification]);
                      }
                      cx.emit(LibraryChanged);
                  })
                  .ok();
          })
          .detach();
    }

    /// Applies disk-check results to the live catalog and, if any file flag
    /// or entry status changed, refreshes the section counts and the visible
    /// list. Returns whether anything changed.
    ///
    /// Each result carries only the files that differed when the check read
    /// the disk. Applying that, rather than swapping in the checked copy of
    /// the entry, leaves everything else about the live entry as it is now,
    /// so a download that completed after the check started is not undone.
    pub(super) fn apply_verifications(&mut self, verifications: &[ItemVerification]) -> bool {
        let mut changed = false;
        for verification in verifications {
            if let Some(item) = self.catalog
                                    .iter_mut()
                                    .find(|i| i.id == verification.item_id)
            {
                changed |= apply_presence_changes(item, &verification.changes);
            }
        }
        if changed {
            self.section_counts =
                section_counts(&self.catalog, now_secs(), self.recently_updated_window_days);
            self.invalidate_cache();
        }
        changed
    }

    /// Returns `true` if `id`'s files are currently being verified against
    /// disk (catalog-wide load-time pass or the on-demand single-item
    /// check), per `verify-downloaded-status-against-disk`.
    #[must_use]
    pub fn is_verifying_downloads(&self, id: &str) -> bool {
        self.verifying_downloads.contains(id)
    }

    /// Returns what the app is busy doing with catalog item `id` right now.
    #[must_use]
    pub fn item_activity(&self, id: &str) -> ItemActivity {
        ItemActivity::of(self.is_downloading(id), self.is_verifying_downloads(id))
    }

    /// Returns [`Self::item_activity`] for every item that is not idle,
    /// mirroring [`Self::checking_items_snapshot`]'s role for non-virtualized
    /// render paths. Items missing from the map are idle.
    #[must_use]
    pub fn item_activity_snapshot(&self) -> HashMap<Arc<str>, ItemActivity> {
        item_activity_map(&self.verifying_downloads, self.download_jobs.item_ids())
    }

    // ── Item-level availability checks
    // ────────────────────────────────────────

    /// Returns `true` if an availability check is currently in flight for
    /// catalog item `id` (on-demand or from the periodic queue).
    #[must_use]
    pub fn is_checking(&self, id: &str) -> bool {
        self.checking_items.contains(id)
    }

    /// Returns a snapshot of every item id with an availability check
    /// currently in flight.
    ///
    /// Used by non-virtualized render paths (e.g. grouped Thumbs) that build
    /// their per-item element list inside a `move` closure without ongoing
    /// access to `cx`, mirroring how `CoverCache`'s image map is snapshotted
    /// up front for the same reason.
    #[must_use]
    pub fn checking_items_snapshot(&self) -> HashSet<Arc<str>> {
        self.checking_items.clone()
    }

    /// Checks `id`'s availability against the server if it hasn't been
    /// checked within `ITEM_CHECK_COOLDOWN_SECS`, or if it has never been
    /// checked. No-ops if the item is unknown, already within its cooldown,
    /// or already has a check in flight.
    ///
    /// Called when the user views an entry's details (the single-click
    /// popover via [`Self::select_item`], or an expanded detail tab — see
    /// each `open_detail_tab` call site).
    pub fn maybe_check_item(&mut self, id: Arc<str>, cx: &mut Context<Self>) {
        let Some(item) = self.item_by_id(&id)
        else {
            return;
        };
        if !item_check_due(item.availability_last_checked) {
            return;
        }
        self.start_item_check(id, cx);
    }

    /// Starts a background availability check for `id`, bypassing the
    /// per-item cooldown (callers are expected to have already filtered by
    /// staleness — see [`Self::maybe_check_item`] and
    /// [`Self::drain_check_queue`]). No-ops if a check for this id is
    /// already in flight or the item is unknown.
    pub(super) fn start_item_check(&mut self, id: Arc<str>, cx: &mut Context<Self>) {
        if self.checking_items.contains(&id) || self.item_by_id(&id).is_none() {
            return;
        }
        self.checking_items.insert(Arc::clone(&id));
        cx.emit(LibraryChanged);

        let numeric_id = self.item_by_id(&id).map(|i| i.numeric_id).unwrap_or(0);
        let service_arc = self.vm.service_arc();

        cx.spawn(async move |this, async_cx| {
            let result = async_cx.background_executor()
                                 .spawn(async move { service_arc.get_item(numeric_id) })
                                 .await;
            if let Err(e) = &result
               && e.kind != LibraryServiceErrorKind::NotFound
            {
                // Transient failure: not evidence of removal, leave the item
                // and its flag entirely unchanged.
                tracing::warn!(id = %id, error = %e, "item availability check failed transiently");
            }
            this.update(async_cx, |ctrl, cx| {
                    if let Some(existing) = ctrl.catalog.iter_mut().find(|i| i.id == id) {
                        apply_check_result(existing, result, std::time::SystemTime::now());
                        ctrl.section_counts = section_counts(&ctrl.catalog,
                                                             now_secs(),
                                                             ctrl.recently_updated_window_days);
                        ctrl.invalidate_cache();
                    }
                    ctrl.checking_items.remove(&id);
                    cx.emit(LibraryChanged);
                    ctrl.drain_check_queue(cx);
                })
                .ok();
        })
        .detach();
    }

    /// Pushes `ids` not already queued or in flight onto `check_queue`, then
    /// starts the drain loop.
    ///
    /// Used by [`Self::request_check_batch`] to enqueue a batch of items
    /// overdue for a re-check.
    pub fn enqueue_checks(&mut self, ids: impl Iterator<Item = Arc<str>>, cx: &mut Context<Self>) {
        for id in ids {
            if should_enqueue_check(&self.check_queue, &self.checking_items, &id) {
                self.check_queue.push_back(id);
            }
        }
        self.drain_check_queue(cx);
    }

    /// Starts the next queued availability check if none is currently in
    /// flight. No-ops if a check is already in flight or the queue is
    /// empty; re-invoked from [`Self::start_item_check`]'s completion
    /// handler for the next queued id.
    pub(super) fn drain_check_queue(&mut self, cx: &mut Context<Self>) {
        if !self.checking_items.is_empty() {
            return;
        }
        let Some(id) = self.check_queue.pop_front()
        else {
            return;
        };
        self.start_item_check(id, cx);
    }

    /// Requests a batch of per-item availability checks, gated by
    /// `ITEM_CHECK_BATCH_COOLDOWN_SECS` and shared between the manual
    /// trigger and the automatic periodic timer so neither can flood the API
    /// by stacking on top of the other.
    ///
    /// No-ops (silently) if a batch was already requested too recently.
    /// Otherwise selects up to `ITEM_CHECK_BATCH_SIZE` catalog items overdue
    /// for a check (oldest-checked first), enqueues them, and immediately
    /// persists the batch timestamp — before the queue finishes draining —
    /// so a slow-draining batch doesn't leave the cooldown window open to a
    /// second trigger firing mid-batch.
    pub fn request_check_batch(&mut self, cx: &mut Context<Self>) {
        let root = cache_dir();
        let last_batch_secs = load_cache_metadata(&root).and_then(|m| m.last_item_check_batch_secs);
        let now = std::time::SystemTime::now();
        let now_secs = now.duration_since(std::time::UNIX_EPOCH)
                          .map(|d| d.as_secs())
                          .unwrap_or_default();
        if check_batch_cooldown_active(last_batch_secs, now_secs) {
            return;
        }

        let ids = select_check_batch(&self.catalog,
                                     dtrpg_core::data::constants::ITEM_CHECK_BATCH_SIZE);
        if let Err(e) = save_check_batch_timestamp(&root, now_secs) {
            tracing::warn!(error = %e, "failed to persist check-batch cooldown timestamp");
        }
        self.enqueue_checks(ids.into_iter(), cx);
    }
}
