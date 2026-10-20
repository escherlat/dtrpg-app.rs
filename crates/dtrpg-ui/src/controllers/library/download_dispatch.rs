//! Submits file downloads to the work queue and applies their results.
//!
//! A download runs as a job on the core work queue. This module submits it and
//! turns the queue's events back into activity entries and catalog updates.

use std::time::Duration;

use dtrpg_core::work::{JobId, JobKind, JobSpec, Outcome, Priority, ResultSlot};

use super::{download_jobs::DownloadJob, *};

impl LibraryController {
    /// Submits a download of `item`'s file number `index` to the work queue.
    ///
    /// The destination is worked out by the job itself, when it starts, from
    /// the storage settings of that moment. The job gets the entry and its
    /// collection memberships as they are now.
    ///
    /// Downloads are user-initiated, so they run ahead of any background work
    /// still waiting in the queue.
    pub(super) fn submit_download(&mut self, item: LibraryItem, index: u32, title: String) {
        let Some(file) = item.files.get(index as usize).cloned()
        else {
            return;
        };
        let collection_ids = collection_ids_for_item(&item, &self.collections);
        let service = self.vm.service_arc();
        let order_product_id = item.order_product_id;
        let key = (Arc::clone(&item.id), index);
        let file_name = file.name.to_string();

        let dest = ResultSlot::new();
        let job_dest = dest.clone();
        let spec = JobSpec { kind:     JobKind::Download,
                             priority: Priority::Interactive,
                             label:    format!("{title}: {file_name}"), };
        let id = self.work.submit(spec, move |context| {
                              let path = resolved_file_path_in_collections(&StorageConfig::load(),
                                                                           &item,
                                                                           &file,
                                                                           &collection_ids);
                              job_dest.put(path.clone());
                              let mut on_retry =
                                  |attempt, delay| context.report_retry(attempt, delay);
                              let mut on_progress =
                                  |received, total| context.report_progress(received, total);
                              let control = DownloadControl::new(context.cancel_flag())
                .on_retry(&mut on_retry)
                .on_progress(&mut on_progress);
                              service.download_item(order_product_id, index, &path, control)
                                     .map_err(|error| error.to_string())
                          });
        self.download_jobs.add(id,
                               DownloadJob { key,
                                             title,
                                             file_name,
                                             order_product_id,
                                             dest,
                                             activity_id: None });
    }

    /// Shows a download in the activity panel once it starts, with a cancel
    /// action that cancels its job.
    pub(super) fn download_started(&mut self, id: JobId, cx: &mut Context<Self>) {
        let Some(job) = self.download_jobs.get(id)
        else {
            return;
        };
        let label = t!("activity.downloading_file_entry",
                       title = job.title,
                       file_name = job.file_name).to_string();
        let work = self.work.clone();
        let cancel: Arc<dyn Fn() + Send + Sync> = Arc::new(move || work.cancel(id));
        let activity_id = self.activity
                              .update(cx, |a, cx| a.start(&label, Some(cancel), cx));
        self.download_jobs.set_activity(id, activity_id);
    }

    /// Moves a download's progress bar to the fraction of the file received.
    /// A download whose size is unknown keeps its indeterminate bar.
    pub(super) fn download_progress(&mut self, id: JobId, received: u64, total: Option<u64>,
                                    cx: &mut Context<Self>) {
        let Some(activity_id) = self.download_jobs.get(id).and_then(|job| job.activity_id)
        else {
            return;
        };
        let Some(total) = total.filter(|total| *total > 0)
        else {
            return;
        };
        // The activity entry keeps the value within 0 to 1, should the server
        // send more than it said it would.
        let fraction = (received as f64 / total as f64) as f32;
        self.activity
            .update(cx, |a, cx| a.update_progress(activity_id, fraction, cx));
    }

    /// Updates a download's activity label to say it is retrying.
    pub(super) fn download_retrying(&mut self, id: JobId, attempt: u32, delay: Duration,
                                    cx: &mut Context<Self>) {
        let Some(job) = self.download_jobs.get(id)
        else {
            return;
        };
        let Some(activity_id) = job.activity_id
        else {
            return;
        };
        let label = t!("activity.downloading_retry",
                       title = job.title,
                       attempt = attempt,
                       delay_secs = delay.as_secs()).to_string();
        self.activity
            .update(cx, |a, cx| a.update_label(activity_id, label, cx));
    }

    /// Applies a finished download: marks the file downloaded on success,
    /// saves the catalog, and completes or fails the activity entry. A
    /// cancelled download changes nothing.
    pub(super) fn download_finished(&mut self, id: JobId, outcome: Outcome,
                                    cx: &mut Context<Self>) {
        let Some(job) = self.download_jobs.finish(id)
        else {
            // Cancelled before it started, and already forgotten.
            return;
        };
        if outcome == Outcome::Cancelled {
            return;
        }

        let (item_id, index) = job.key.clone();
        let dest = job.dest.take();
        let succeeded = outcome == Outcome::Succeeded;
        if let Some(mut item) = self.item_by_id(&item_id) {
            if succeeded {
                if let Some(file) = item.files.get_mut(index as usize) {
                    file.downloaded = true;
                    file.offloaded = false;
                }
                self.file_info_cache
                    .remove(&(Arc::clone(&item_id), index as usize));
                if let Some(dest) = &dest {
                    create_collections_symlinks(dest,
                                                item.order_product_id,
                                                item.product_id,
                                                &self.collections);
                }
            }
            item.recompute_status();
            // Persist the download to the library index immediately —
            // otherwise a restart before the next catalog reload shows
            // `Cloud` status regardless of what `reconcile_catalog` does
            // (see `catalog-live-data-swap`).
            if let Err(e) = self.index.replace_item(&item) {
                tracing::error!(error = %e, "failed to write download status to the library index");
            }
        }
        self.refresh_section_counts();
        self.invalidate_cache();
        cx.emit(LibraryChanged);

        if succeeded && let Some(dest) = dest.clone() {
            self.project_tags_after_download(&item_id, dest, cx);
        }

        let activity_id = job.activity_id;
        match outcome {
            Outcome::Failed(error) => {
                tracing::warn!(item_id = %item_id,
                               index,
                               order_product_id = job.order_product_id,
                               dest = dest.as_deref()
                                          .map_or_else(|| "<unresolved>".to_owned(),
                                                       |dest| dest.display().to_string()),
                               title = %job.title,
                               error = %error,
                               "download failed");
                if let Some(activity_id) = activity_id {
                    self.activity
                        .update(cx, |a, cx| a.error(activity_id, error, cx));
                }
            }
            _ => {
                if let Some(activity_id) = activity_id {
                    self.activity
                        .update(cx, |a, cx| a.complete(activity_id, cx));
                }
            }
        }
    }
}
