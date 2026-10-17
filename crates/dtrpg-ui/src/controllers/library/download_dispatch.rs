//! Dequeues and performs a single download transfer.

use super::*;

impl LibraryController {
    /// Dispatches downloads for as many queued files as there are free
    /// concurrency slots (see [`Self::available_slots`]).
    pub(super) fn drain_download_queue(&mut self, cx: &mut Context<Self>) {
        while self.available_slots() > 0 {
            let Some((item_id, index, title)) = self.download_queue.pop_front()
            else {
                break;
            };
            self.active_downloads += 1;
            self.dispatch_download(item_id, index, title, cx);
        }
    }

    /// Starts a single file's download task. Assumes the caller has already
    /// reserved a concurrency slot by incrementing `active_downloads`.
    ///
    /// Resolves `item.files[index]` to a destination under the configured
    /// storage root, then calls [`LibraryService::download_item`] on the
    /// background executor.
    pub(super) fn dispatch_download(&mut self, item_id: Arc<str>, index: u32, title: String,
                                    cx: &mut Context<Self>) {
        let key = (Arc::clone(&item_id), index);
        let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.download_cancel_flags
            .insert(key.clone(), Arc::clone(&cancel_flag));

        let cancel_fn: Arc<dyn Fn() + Send + Sync> = {
            let flag = Arc::clone(&cancel_flag);
            Arc::new(move || flag.store(true, std::sync::atomic::Ordering::SeqCst))
        };
        // Catalog data isn't `Send` and must stay on this thread; resolve
        // what to fetch (and the file name for the activity label) into
        // owned, Send-safe values before spawning.
        let found = self.catalog
                        .iter()
                        .find(|i| i.id == item_id)
                        .and_then(|item| {
                            item.files.get(index as usize).map(|file| {
                    let collection_ids = collection_ids_for_item(item, &self.collections);
                    let dest = resolved_file_path_in_collections(
                        &StorageConfig::load(),
                        item,
                        file,
                        &collection_ids,
                    );
                    (item.order_product_id, file.name.to_string(), dest)
                })
                        });

        let label = match &found {
            Some((_, file_name, _)) => t!("activity.downloading_file_entry",
                                          title = title,
                                          file_name = file_name).to_string(),
            None => t!("activity.downloading_file", title = title).to_string(),
        };
        let activity_id = self.activity
                              .update(cx, |a, cx| a.start(&label, Some(cancel_fn), cx));
        self.download_activity_ids.insert(key.clone(), activity_id);

        let weak_activity = self.activity.downgrade();
        let task_item_id = Arc::clone(&item_id);
        let fetch_target = found.map(|(order_product_id, _, dest)| (order_product_id, dest));
        // Captured up front for the failure log below — `fetch_target` itself
        // is moved into the spawned task and consumed by `download_item`.
        let order_product_id_for_log = fetch_target.as_ref().map(|(id, _)| *id);
        let dest_for_log = fetch_target.as_ref()
                                       .map(|(_, dest)| dest.display().to_string());
        // Captured up front for symlinking into `collections/` on success,
        // for the same reason as `dest_for_log` above.
        let dest_for_symlink = fetch_target.as_ref().map(|(_, dest)| dest.clone());
        let service_arc = self.vm.service_arc();
        let title_for_retry = title.clone();

        cx.spawn(async move |this, async_cx| {
              let outcome: Result<(), String> = match fetch_target {
                  Some((order_product_id, dest)) => {
                      let cancel_for_fetch = Arc::clone(&cancel_flag);
                      let (retry_tx, retry_rx) =
                          std::sync::mpsc::channel::<(u32, std::time::Duration)>();
                      let fetch = async_cx.background_executor().spawn(async move {
                                                                    service_arc
                            .download_item(
                                order_product_id,
                                index,
                                &dest,
                                &cancel_for_fetch,
                                Some(&mut |attempt, delay| {
                                    retry_tx.send((attempt, delay)).ok();
                                }),
                            )
                            .map_err(|e| e.to_string())
                                                                });

                      // Drain retry-progress notifications and update the
                      // activity label until `download_item` returns and drops
                      // `retry_tx`, disconnecting the channel — mirroring the
                      // page-progress channel loop used for catalog fetches.
                      let mut retry_rx = Some(retry_rx);
                      loop {
                          let Some(receiver) = retry_rx.take()
                          else {
                              break;
                          };
                          let (msg, returned_rx) = async_cx.background_executor()
                                                           .spawn(async move {
                                                               let msg = receiver.recv();
                                                               (msg, receiver)
                                                           })
                                                           .await;
                          match msg {
                              Ok((attempt, delay)) => {
                                  let label = t!("activity.downloading_retry",
                                                 title = title_for_retry,
                                                 attempt = attempt,
                                                 delay_secs = delay.as_secs()).to_string();
                                  weak_activity.update(async_cx, |a, cx| {
                                                   a.update_label(activity_id, label, cx)
                                               })
                                               .ok();
                                  retry_rx = Some(returned_rx);
                              }
                              Err(_) => break,
                          }
                      }

                      fetch.await
                  }
                  None => Err("no downloadable file found for this item".to_string()),
              };
              let cancelled = cancel_flag.load(std::sync::atomic::Ordering::SeqCst);

              let activity_id_after =
                  this.update(async_cx, |ctrl, cx| {
                          ctrl.active_downloads = ctrl.active_downloads.saturating_sub(1);
                          let key = (task_item_id.clone(), index);
                          ctrl.download_cancel_flags.remove(&key);
                          let activity_id = ctrl.download_activity_ids.remove(&key);
                          if !cancelled {
                              if let Some(item) =
                                  ctrl.catalog.iter_mut().find(|i| i.id == task_item_id)
                              {
                                  if outcome.is_ok() {
                                      if let Some(file) = item.files.get_mut(index as usize) {
                                          file.downloaded = true;
                                          file.offloaded = false;
                                      }
                                      ctrl.file_info_cache
                                          .remove(&(task_item_id.clone(), index as usize));
                                      if let Some(dest) = &dest_for_symlink {
                                          create_collections_symlinks(dest,
                                                                      item.order_product_id,
                                                                      item.product_id,
                                                                      &ctrl.collections);
                                      }
                                  }
                                  item.recompute_status();
                              }
                              ctrl.section_counts =
                                  section_counts(&ctrl.catalog,
                                                 now_secs(),
                                                 ctrl.recently_updated_window_days);
                              ctrl.invalidate_cache();
                              cx.emit(LibraryChanged);
                          }
                          ctrl.drain_download_queue(cx);
                          ctrl.drain_thumbnail_queue(cx);
                          activity_id
                      })
                      .ok()
                      .flatten();

              // Persist the download to the on-disk catalog cache immediately —
              // otherwise a restart before the next live-fetch cache write (see
              // `start_load_inner`) loads a cache that never recorded this
              // download, showing `Cloud` status regardless of what
              // `reconcile_catalog` does (see `catalog-live-data-swap`).
              if !cancelled && outcome.is_ok() {
                  let items_to_save = this.update(async_cx, |ctrl, _cx| ctrl.catalog.clone())
                                          .unwrap_or_default();
                  let save_root = cache_dir();
                  async_cx.background_executor()
                          .spawn(async move {
                              if let Err(e) = save_catalog_cache(&save_root, &items_to_save) {
                                  tracing::warn!(error = %e, "failed to save catalog cache");
                              }
                          })
                          .await;
              }

              if let Err(e) = &outcome
                 && !cancelled
              {
                  tracing::warn!(item_id = %task_item_id,
                                 index,
                                 order_product_id = order_product_id_for_log,
                                 dest = dest_for_log.as_deref().unwrap_or("<unresolved>"),
                                 label = %label,
                                 error = %e,
                                 "download failed");
              }

              if !cancelled && let Some(id) = activity_id_after {
                  match &outcome {
                      Ok(()) => {
                          weak_activity.update(async_cx, |a, cx| a.complete(id, cx))
                                       .ok();
                      }
                      Err(e) => {
                          weak_activity.update(async_cx, |a, cx| a.error(id, e.clone(), cx))
                                       .ok();
                      }
                  }
              }
          })
          .detach();
    }
}
