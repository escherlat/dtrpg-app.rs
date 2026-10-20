//! Controller construction, periodic timers, and catalog load entry points.

use super::*;

impl LibraryController {
    /// Creates a controller and immediately schedules catalog loading on a
    /// background thread.
    ///
    /// The controller starts in the `Loading` pane state with an empty catalog.
    /// When the background fetch completes, [`Self::handle_sync_event`]
    /// reacts to `Library`'s `FullyLoaded`/`Failed` events and emits
    /// [`LibraryChanged`].
    ///
    /// # Panics
    ///
    /// Does not panic; service errors are reflected in [`pane_state`].
    pub fn new(service: Box<dyn LibraryService>,
               collections_service: Box<dyn CollectionsService>,
               activity: Entity<ActivityController>, cx: &mut Context<Self>)
               -> Self {
        let (mut ctrl, work_events, sync_events) =
            Self::build(service, collections_service, activity);
        ctrl.start_work_event_pump(work_events, cx);
        ctrl.start_catalog_sync_event_pump(sync_events, cx);
        ctrl.start_load(cx);
        ctrl.start_periodic_check_batch_timer(cx);
        ctrl.start_periodic_catalog_refresh_timer(cx);
        ctrl
    }

    /// Builds the controller's initial state, along with the receivers for
    /// its work queue's and catalog-sync's events, without starting
    /// anything.
    ///
    /// Reads saved preferences from disk, but loads no catalog and starts no
    /// timers or event loop, so tests can build a controller and drive it
    /// themselves.
    pub(super) fn build(service: Box<dyn LibraryService>,
                        collections_service: Box<dyn CollectionsService>,
                        activity: Entity<ActivityController>)
                        -> (Self, JobEvents, LibraryEvents) {
        Self::build_with_cache_root(service, collections_service, activity, cache_dir())
    }

    /// Builds the controller against `cache_root` instead of the real cache
    /// directory, so tests get an isolated library index instead of the
    /// user's real one.
    pub(super) fn build_with_cache_root(service: Box<dyn LibraryService>,
                                        collections_service: Box<dyn CollectionsService>,
                                        activity: Entity<ActivityController>,
                                        cache_root: PathBuf)
                                        -> (Self, JobEvents, LibraryEvents) {
        let vm = LibraryViewModel::new(service);
        let prefs = dtrpg_core::data::ui_preferences::UiPreferences::load();
        let view_prefs = dtrpg_core::data::file_openers::CatalogViewPrefs::load();
        let storage = StorageConfig::load();
        // The library session is the catalog's only storage; there is no
        // in-memory fallback to degrade to, so a controller cannot exist
        // without one.
        #[allow(clippy::expect_used)]
        let (library, work_events, sync_events) =
            Library::open(cache_root, &storage).expect("failed to open the library session");
        let library = Arc::new(library);

        let ctrl = Self { vm,
                          activity,
                          thumbnail_attempts: HashMap::new(),
                          availability_checks: HashMap::new(),
                          filter: view_prefs.to_filter(),
                          search_query: String::new(),
                          sort: view_prefs.to_sort(),
                          sort_direction: SortDirection::default(),
                          collection_sort: prefs.collection_sort(),
                          collection_sort_direction: prefs.collection_sort_direction(),
                          sidebar_open: SidebarOpen::saved(),
                          group_by: view_prefs.to_group_by(),
                          list_columns: view_prefs.to_list_columns(),
                          presentation: view_prefs.to_presentation(),
                          selection: Selection::default(),
                          section_counts: SectionCounts::default(),
                          publishers: Vec::new(),
                          collections: Vec::new(),
                          collections_loaded: false,
                          collections_service: Arc::from(collections_service),
                          collection_members: HashSet::new(),
                          library,
                          thumbnail_jobs: ThumbnailJobs::default(),
                          thumbnail_activity_id: None,
                          thumbnail_processed: 0,
                          thumbnail_refresh_active: false,
                          thumbnail_refresh_pending: HashSet::new(),
                          thumbnail_refresh_succeeded: 0,
                          thumbnail_refresh_failed: 0,
                          catalog_loading: true,
                          catalog_activity_id: None,
                          catalog_progress_total: None,
                          items_cache: None,
                          grouped_items_cache: None,
                          filter_count_cache: 0,
                          catalog_ids_cache: Arc::default(),
                          publisher_search_open: false,
                          publisher_search_query: String::new(),
                          collection_search_open: false,
                          collection_search_query: String::new(),
                          detail_panel_width:
                              dtrpg_core::data::constants::DETAIL_PANEL_DEFAULT_WIDTH,
                          entry_bounds: HashMap::new(),
                          selected_item_file: HashMap::new(),
                          inspector_tabs: HashMap::new(),
                          inspector_contents: HashMap::new(),
                          inspector_selected_entries: HashMap::new(),
                          inspector_extractions: HashMap::new(),
                          inspector_preview: HashMap::new(),
                          inspector_selected_archive: HashMap::new(),
                          inspector_collapsed_dirs: HashMap::new(),
                          inspector_filter_inputs: HashMap::new(),
                          other_details_open: HashMap::new(),
                          file_other_details_open: HashMap::new(),
                          file_info_cache: HashMap::new(),
                          checking_items: HashSet::new(),
                          verifying_downloads: HashSet::new(),
                          check_jobs: CheckJobs::default(),
                          download_jobs: DownloadJobs::default(),
                          tags: Arc::new(TagLibrary::default()),
                          tag_generation: 0,
                          file_tag_sync_running: false,
                          storage_override: None,
                          file_tags_reconciled: false,
                          max_concurrent_downloads: storage.max_concurrent_downloads(),
                          recently_updated_window_days: storage.recently_updated_window_days(),
                          pending_download: None,
                          bulk_selection_mode: false,
                          bulk_selected_ids: HashSet::new() };
        (ctrl, work_events, sync_events)
    }

    /// Applies a work-queue event. Each handler ignores jobs that are not its
    /// own, so an event goes to both.
    pub(super) fn handle_work_event(&mut self, event: JobEvent, cx: &mut Context<Self>) {
        match event {
            JobEvent::Started { id } => {
                self.thumbnail_started(id, cx);
                self.download_started(id, cx);
                self.check_started(id, cx);
            }
            JobEvent::Retrying { id, attempt, delay } => {
                self.download_retrying(id, attempt, delay, cx);
            }
            JobEvent::Progress { id, done, total } => {
                self.download_progress(id, done, total, cx);
            }
            JobEvent::Finished { id, outcome } => {
                self.thumbnail_finished(id, outcome.clone(), cx);
                self.download_finished(id, outcome.clone(), cx);
                self.check_finished(id, outcome, cx);
            }
        }
    }

    /// Starts a loop that applies each work-queue event to the controller, for
    /// as long as the controller lives.
    ///
    /// Events arrive on the foreground executor, one at a time, so handlers
    /// can update controller state without further synchronization. The loop
    /// ends when the controller is dropped: the queue's sender goes with it,
    /// which closes the channel.
    pub(super) fn start_work_event_pump(&self, mut events: JobEvents, cx: &mut Context<Self>) {
        cx.spawn(async move |this, async_cx| {
              while let Some(event) = events.recv().await {
                  let alive = this.update(async_cx, |ctrl, cx| ctrl.handle_work_event(event, cx))
                                  .is_ok();
                  if !alive {
                      break;
                  }
              }
          })
          .detach();
    }

    /// Starts a background loop that calls
    /// [`Self::request_check_batch`] every `ITEM_CHECK_BATCH_TIMER_SECS`,
    /// for the lifetime of the controller.
    ///
    /// The timer interval is independent of `ITEM_CHECK_BATCH_COOLDOWN_SECS`
    /// — this only decides how often the loop *asks*; `request_check_batch`
    /// applies the real cooldown gate, so most wake-ups when nothing is due
    /// are a cheap no-op (a metadata read, no network call). The loop exits
    /// once the controller entity is dropped (`this.update` starts failing).
    pub(super) fn start_periodic_check_batch_timer(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, async_cx| {
              loop {
                  async_cx
                    .background_executor()
                    .timer(std::time::Duration::from_secs(
                        dtrpg_core::data::constants::ITEM_CHECK_BATCH_TIMER_SECS,
                    ))
                    .await;
                  let alive = this.update(async_cx, |ctrl, cx| ctrl.request_check_batch(cx))
                                  .is_ok();
                  if !alive {
                      break;
                  }
              }
          })
          .detach();
    }

    /// Starts a background loop that re-runs the catalog staleness check
    /// every `CATALOG_REFRESH_TIMER_INTERVAL_SECS`, independent of the
    /// startup sequence, for the lifetime of the controller — a
    /// long-running session (never restarted) still gets a refresh once its
    /// staleness threshold elapses, per
    /// `rust-resource-refresh-scheduling`'s recurring-timer requirement.
    ///
    /// Calls the same non-forced [`Self::start_load`] the startup sequence
    /// uses: `Library`'s own single-flight gate (see `Library::start_load`)
    /// makes an overlapping wake-up a no-op, and the existing
    /// auto-load-policy staleness check inside the catalog-sync run decides
    /// whether this wake-up actually triggers a fetch or is itself a no-op.
    /// The loop exits once the controller entity is dropped.
    pub(super) fn start_periodic_catalog_refresh_timer(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, async_cx| {
              loop {
                  async_cx
                    .background_executor()
                    .timer(std::time::Duration::from_secs(
                        dtrpg_core::data::constants::CATALOG_REFRESH_TIMER_INTERVAL_SECS,
                    ))
                    .await;
                  let alive = this.update(async_cx, |ctrl, cx| ctrl.start_load(cx))
                                  .is_ok();
                  if !alive {
                      break;
                  }
              }
          })
          .detach();
    }

    /// Spawns a background task to load collections and the catalog — from
    /// cache, then optionally from the live API.
    ///
    /// Progress is reported through a single activity item whose label
    /// advances through each stage in turn: collections, the cache-freshness
    /// count check, then one update per catalog page as it arrives.
    ///
    /// When `force_reload` is true the auto-load policy is bypassed and a full
    /// live fetch always runs. When false, the fetch is skipped if the
    /// cache is non-empty and was written within the last 7 days.
    ///
    /// Pages are delivered via an mpsc channel so each page triggers a UI
    /// update before the next page arrives.
    pub(super) fn start_load(&mut self, cx: &mut Context<Self>) {
        self.start_load_inner(cx, false);
    }

    /// Verifies every catalog item's files against disk and applies any
    /// resulting `downloaded`/`status` changes back to the catalog, per
    /// `verify-downloaded-status-against-disk`.
    ///
    /// Runs as one batched background-executor pass rather than a per-item
    /// spawn, matching `save_catalog_cache`'s single-round-trip shape —
    /// avoids spawning hundreds of tiny tasks for a large library. Marks
    /// every item as verifying for the duration of the pass (see
    /// [`Self::is_verifying_downloads`]) so catalog rendering can show a
    /// pending indicator.
    ///
    /// The pass works on a snapshot of the catalog, but only the differences
    /// it finds are applied back (see [`Self::apply_verifications`]), never
    /// the snapshot itself: a download that finishes while the pass is
    /// running must not be reverted to `Cloud` by a snapshot that predates it.
    pub(super) fn verify_catalog_downloads(&mut self, cx: &mut Context<Self>) {
        let catalog = self.index().list_items().unwrap_or_default();
        let collections = self.collections.clone();
        let ids: HashSet<Arc<str>> = catalog.iter().map(|i| Arc::clone(&i.id)).collect();
        self.verifying_downloads.extend(ids.iter().cloned());
        cx.emit(LibraryChanged);

        cx.spawn(async move |this, async_cx| {
              let verifications = async_cx.background_executor()
                                          .spawn(async move {
                                              let storage = StorageConfig::load();
                                              catalog
                        .iter()
                        .filter_map(|item| {
                            let collection_ids = collection_ids_for_item(item, &collections);
                            find_presence_changes_in_collections(item, &storage, &collection_ids)
                        })
                        .collect::<Vec<_>>()
                                          })
                                          .await;

              this.update(async_cx, |ctrl, cx| {
                      for id in &ids {
                          ctrl.verifying_downloads.remove(id);
                      }
                      ctrl.apply_verifications(&verifications);
                      cx.emit(LibraryChanged);
                  })
                  .ok();
          })
          .detach();
    }

    /// Applies one catalog-sync event from `Library`, updating only this
    /// controller's own view state — the catalog-sync state machine itself
    /// (generation tracking, single-flight, the on-disk cache, the live
    /// fetch) lives in `Library` since ADR 0002 Slice 2.
    pub(super) fn handle_sync_event(&mut self, event: LibraryEvent, cx: &mut Context<Self>) {
        match event {
            LibraryEvent::Stage(stage) => self.sync_stage_label(stage, cx),
            LibraryEvent::Total(total) => {
                self.catalog_progress_total = Some(total);
                if let Some(id) = self.catalog_activity_id {
                    self.activity
                        .update(cx, |a, cx| a.update_progress(id, 0.0, cx));
                }
            }
            LibraryEvent::Page(page) => {
                if let Some(id) = self.catalog_activity_id {
                    let label = t!("activity.loading_library_page", page = page).to_string();
                    self.activity
                        .update(cx, |a, cx| a.update_label(id, label, cx));
                }
            }
            LibraryEvent::CachePrepopulated { items,
                                              force_reload, } => {
                if !items.is_empty() {
                    self.react_to_prepopulated_items(&items, cx);
                }
                self.begin_tag_sync(force_reload, cx);
            }
            LibraryEvent::CollectionsChanged(entries) => self.apply_collections(entries, cx),
            LibraryEvent::PageAppended { items } => self.react_to_prepopulated_items(&items, cx),
            LibraryEvent::PartialMerged { items } => {
                self.react_to_new_catalog_items(&items, cx);
                self.catalog_loading = false;
                cx.emit(LibraryChanged);
                self.complete_catalog_activity(cx);
                self.verify_catalog_downloads(cx);
            }
            LibraryEvent::SkippedFreshCache => {
                self.catalog_loading = false;
                self.revalidate_publisher_filter();
                cx.emit(LibraryChanged);
                self.complete_catalog_activity(cx);
                self.verify_catalog_downloads(cx);
            }
            LibraryEvent::FullyLoaded { items } => {
                self.react_to_new_catalog_items(&items, cx);
                self.catalog_loading = false;
                cx.emit(LibraryChanged);
                self.complete_catalog_activity(cx);
                self.begin_tag_sync(false, cx);
                self.verify_catalog_downloads(cx);
            }
            LibraryEvent::Failed { message, quiet } => {
                self.catalog_loading = false;
                cx.emit(LibraryChanged);
                if quiet {
                    self.complete_catalog_activity(cx);
                }
                else if let Some(id) = self.catalog_activity_id.take() {
                    self.activity.update(cx, |a, cx| a.error(id, message, cx));
                }
            }
            LibraryEvent::Finished { reload_requested } => {
                self.catalog_progress_total = None;
                if reload_requested {
                    self.start_load(cx);
                }
            }
        }
    }

    /// Updates the catalog-sync activity item's label for `stage`.
    fn sync_stage_label(&mut self, stage: SyncStage, cx: &mut Context<Self>) {
        let Some(id) = self.catalog_activity_id
        else {
            return;
        };
        let label = match stage {
            SyncStage::Collections => t!("activity.loading_library_collections").to_string(),
            SyncStage::CountCheck => t!("activity.loading_library_count").to_string(),
            SyncStage::PartialFetch => t!("activity.loading_library_partial").to_string(),
            SyncStage::LiveFetch => t!("activity.loading_library").to_string(),
        };
        self.activity
            .update(cx, |a, cx| a.update_label(id, label, cx));
    }

    /// Dismisses the catalog-sync activity item, if one is in flight.
    fn complete_catalog_activity(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.catalog_activity_id.take() {
            self.activity.update(cx, |a, cx| a.complete(id, cx));
        }
    }

    /// Starts a loop that applies each catalog-sync event to the controller,
    /// for as long as the controller lives. See [`Self::start_work_event_pump`]
    /// for the same pattern applied to the work queue's events.
    pub(super) fn start_catalog_sync_event_pump(&self, mut events: LibraryEvents,
                                                cx: &mut Context<Self>) {
        cx.spawn(async move |this, async_cx| {
              while let Some(event) = events.recv().await {
                  let alive = this.update(async_cx, |ctrl, cx| ctrl.handle_sync_event(event, cx))
                                  .is_ok();
                  if !alive {
                      break;
                  }
              }
          })
          .detach();
    }
}
