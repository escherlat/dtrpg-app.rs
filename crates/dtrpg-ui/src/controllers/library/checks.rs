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
        let Some(item) = self.item_by_id(&id)
        else {
            return;
        };
        let collection_ids = collection_ids_for_item(&item, &self.collections);
        self.library.begin_verifying([Arc::clone(&id)]);
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
                      ctrl.library.end_verifying([&id]);
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
        let changed = self.library.apply_presence_verifications(verifications);
        if changed {
            self.refresh_section_counts();
            self.invalidate_cache();
        }
        changed
    }

    /// Returns `true` if `id`'s files are currently being verified against
    /// disk (catalog-wide load-time pass or the on-demand single-item
    /// check), per `verify-downloaded-status-against-disk`.
    #[must_use]
    pub fn is_verifying_downloads(&self, id: &str) -> bool {
        self.library.is_verifying(id)
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
        let downloading = self.library.downloading_item_ids();
        item_activity_map(&self.library.verifying_snapshot(), downloading.iter())
    }

    // ── Item-level availability checks
    // ────────────────────────────────────────

    /// Returns `true` if an availability check is currently in flight for
    /// catalog item `id` (on-demand or from the periodic queue).
    #[must_use]
    pub fn is_checking(&self, id: &str) -> bool {
        self.library.is_checking(id)
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
        self.library.checking_items_snapshot()
    }

    /// Checks `id`'s availability against the server if it hasn't been
    /// checked within `ITEM_CHECK_COOLDOWN_SECS`, or if it has never been
    /// checked. No-ops if the item is unknown, already within its cooldown,
    /// or already has a check running.
    ///
    /// Called when the user views an entry's details (the single-click
    /// popover via [`Self::select_item`], or an expanded detail tab — see
    /// each `open_detail_tab` call site). The user is waiting on it, so it
    /// goes ahead of any background checks still waiting, and replaces one
    /// already waiting for the same item.
    pub fn maybe_check_item(&mut self, id: Arc<str>, _cx: &mut Context<Self>) {
        let Some(item) = self.item_by_id(&id)
        else {
            return;
        };
        if !item_check_due(item.availability_last_checked) {
            return;
        }
        self.submit_check(id, Priority::Interactive);
    }

    /// Submits an availability check for `id` via `Library`, bypassing the
    /// per-item cooldown (callers are expected to have already filtered by
    /// staleness). No-ops if the item is unknown.
    fn submit_check(&mut self, id: Arc<str>, priority: Priority) {
        let Some(numeric_id) = self.item_by_id(&id).map(|item| item.numeric_id)
        else {
            return;
        };
        let service = self.vm.service_arc();
        self.library.submit_check(id, numeric_id, priority, service);
    }

    /// Shows an item as being checked once its check starts.
    pub(super) fn check_started(&mut self, id: JobId, cx: &mut Context<Self>) {
        if self.library.check_started(id).is_some() {
            cx.emit(LibraryChanged);
        }
    }

    /// Applies a finished check to the catalog.
    ///
    /// A not-found answer marks the item unavailable. Any other failure is
    /// not evidence of removal, so it leaves the item and its flag unchanged.
    pub(super) fn check_finished(&mut self, id: JobId, outcome: Outcome, cx: &mut Context<Self>) {
        let Some(job) = self.library.check_finished(id)
        else {
            // Cancelled before it started, and already forgotten.
            return;
        };

        let result = match outcome {
            Outcome::Succeeded => job.result.take(),
            Outcome::Failed(message) => {
                tracing::warn!(id = %job.item_id, error = %message,
                               "item availability check failed");
                None
            }
            Outcome::Cancelled => None,
        };
        if let Some(result) = result {
            if let Err(error) = &result
               && error.kind != LibraryServiceErrorKind::NotFound
            {
                tracing::warn!(id = %job.item_id, error = %error,
                               "item availability check failed transiently");
            }
            if self.library.apply_availability_result(&job.item_id, result) {
                self.refresh_section_counts();
                self.invalidate_cache();
            }
        }
        cx.emit(LibraryChanged);
    }

    /// Cancels every availability check that has not started. A check already
    /// running is left to finish.
    pub(super) fn cancel_pending_checks(&mut self) {
        self.library.cancel_pending_checks();
    }

    /// Submits availability checks for `ids` that have none waiting or running.
    ///
    /// Used by [`Self::request_check_batch`] to submit a batch of items overdue
    /// for a re-check.
    pub fn enqueue_checks(&mut self, ids: impl Iterator<Item = Arc<str>>, priority: Priority) {
        for id in ids {
            self.submit_check(id, priority);
        }
    }

    /// Requests a batch of per-item availability checks on behalf of the
    /// automatic timer. They run in the background, behind anything the user
    /// asks for. See [`Self::request_manual_check_batch`] for the same batch
    /// requested by the user.
    ///
    /// Gated by `ITEM_CHECK_BATCH_COOLDOWN_SECS`, shared between the manual
    /// trigger and the automatic periodic timer so neither can flood the API
    /// by stacking on top of the other.
    pub fn request_check_batch(&mut self, cx: &mut Context<Self>) {
        self.request_check_batch_as(Priority::Background, cx);
    }

    /// Requests the same batch as [`Self::request_check_batch`], on behalf of
    /// the user ("Check Item Availability"). The user is waiting on it, so it
    /// goes ahead of background work.
    pub fn request_manual_check_batch(&mut self, cx: &mut Context<Self>) {
        self.request_check_batch_as(Priority::Interactive, cx);
    }

    /// No-ops (silently) if a batch was already requested too recently.
    /// Otherwise selects up to `ITEM_CHECK_BATCH_SIZE` catalog items overdue
    /// for a check (oldest-checked first), submits them, and immediately
    /// persists the batch timestamp — before the checks finish — so a
    /// slow-running batch doesn't leave the cooldown window open to a second
    /// trigger firing mid-batch.
    fn request_check_batch_as(&mut self, priority: Priority, _cx: &mut Context<Self>) {
        let root = self.cache_root().clone();
        let last_batch_secs = load_cache_metadata(&root).and_then(|m| m.last_item_check_batch_secs);
        let now = std::time::SystemTime::now();
        let now_secs = now.duration_since(std::time::UNIX_EPOCH)
                          .map(|d| d.as_secs())
                          .unwrap_or_default();
        if check_batch_cooldown_active(last_batch_secs, now_secs) {
            return;
        }

        let all = self.all_items();
        let ids = select_check_batch(&all, dtrpg_core::data::constants::ITEM_CHECK_BATCH_SIZE);
        if let Err(e) = save_check_batch_timestamp(&root, now_secs) {
            tracing::warn!(error = %e, "failed to persist check-batch cooldown timestamp");
        }
        self.enqueue_checks(ids.into_iter(), priority);
    }
}
