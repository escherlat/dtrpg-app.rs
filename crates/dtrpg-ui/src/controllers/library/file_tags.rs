//! Makes the tags on downloaded files match the library index.
//!
//! The index owns tags (ADR 0004). Once per launch, after the catalog is
//! loaded and its files checked against disk, the files of every tagged item
//! are made to match it, in case a tag was edited outside Libri while it was
//! closed. Files that were changed are counted in the activity panel and
//! logged one by one.

use dtrpg_core::data::index::IndexError;
use dtrpg_core::platform::{Platform, PlatformIntegration};
use dtrpg_core::services::tag_projection::{
    ReconcileReport, TaggedFile, project_indexed_tags, reconcile_with_index,
};

use super::*;

/// Returns every downloaded file in `catalog` with the item it belongs to.
pub(super) fn downloaded_files(catalog: &[LibraryItem], collections: &[CollectionEntry],
                               storage: &StorageConfig)
                               -> Vec<TaggedFile> {
    let mut files = Vec::new();
    for item in catalog {
        let collection_ids = collection_ids_for_item(item, collections);
        for file in item.files.iter().filter(|file| file.downloaded) {
            files.push(TaggedFile { item_id: item.id.to_string(),
                                    path:    resolved_file_path_in_collections(storage,
                                                                               item,
                                                                               file,
                                                                               &collection_ids), });
        }
    }
    files
}

impl LibraryController {
    /// Asks for the files' tags to be reconciled once the next full
    /// verification of downloads finishes.
    ///
    /// Reconciling needs both the whole catalog and the verified `downloaded`
    /// flags, so it waits for the verification that follows a full load, not
    /// the one that follows a partial page.
    pub(super) fn request_file_tag_reconcile(&mut self) {
        self.reconcile_tags_after_verify = true;
    }

    /// Runs the reconcile if one was requested and none has run this launch.
    pub(super) fn reconcile_file_tags_if_requested(&mut self, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.reconcile_tags_after_verify) || self.file_tags_reconciled {
            return;
        }
        if !Platform.capabilities().filesystem_tags {
            return;
        }
        self.file_tags_reconciled = true;

        let catalog = self.catalog.clone();
        let collections = self.collections.clone();
        let cache_root = self.cache_root.clone();
        cx.spawn(async move |this, async_cx| {
              let outcome = async_cx.background_executor()
                                    .spawn(async move {
                                        let files = downloaded_files(&catalog,
                                                                     &collections,
                                                                     &StorageConfig::load());
                                        reconcile_with_index(&Platform, &cache_root, &files)
                                    })
                                    .await;
              this.update(async_cx, |ctrl, cx| ctrl.report_file_tags(outcome, cx))
                  .ok();
          })
          .detach();
    }

    /// Gives a file that has just finished downloading the tags its item has in
    /// the index, in the background. A file that cannot be written is logged
    /// and does not fail the download.
    pub(super) fn project_tags_after_download(&self, item_id: &Arc<str>, path: PathBuf,
                                              cx: &mut Context<Self>) {
        if !Platform.capabilities().filesystem_tags {
            return;
        }
        let item_id = Arc::clone(item_id);
        let cache_root = self.cache_root.clone();
        cx.background_executor()
          .spawn(async move {
              match project_indexed_tags(&Platform, &cache_root, &item_id, &[path]) {
                  Ok(failures) => {
                      for failure in failures {
                          tracing::warn!(path = %failure.path.display(),
                                         error = %failure.error,
                                         "could not give a downloaded file its tags");
                      }
                  }
                  Err(error) => {
                      tracing::warn!(%error, "could not read tags from the library index");
                  }
              }
          })
          .detach();
    }

    /// Logs what a reconcile did and tells the user how many files it changed
    /// or could not change. Says nothing if every file already matched.
    pub(super) fn report_file_tags(&mut self, outcome: Result<ReconcileReport, IndexError>,
                                   cx: &mut Context<Self>) {
        let report = match outcome {
            Ok(report) => report,
            Err(error) => {
                tracing::warn!(%error, "could not read tags from the library index");
                return;
            }
        };
        for path in &report.restored {
            tracing::info!(path = %path.display(), "restored a file's tags from the library");
        }
        for failure in &report.failed {
            tracing::warn!(path = %failure.path.display(),
                           error = %failure.error,
                           "could not reconcile a file's tags");
        }
        if !report.restored.is_empty() {
            let label = t!("activity.tags_restored", count = report.restored.len()).to_string();
            self.activity.update(cx, |activity, cx| {
                             let id = activity.start(&label, None, cx);
                             activity.complete(id, cx);
                         });
        }
        if !report.failed.is_empty() {
            let label = t!("activity.tags_restore_failed", count = report.failed.len()).to_string();
            self.activity.update(cx, |activity, cx| {
                             let id = activity.start(&label, None, cx);
                             activity.error(id, label.clone(), cx);
                         });
        }
    }
}
