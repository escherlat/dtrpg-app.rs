//! Keeps the tags on files in step with the library index.
//!
//! As soon as the catalog saved on disk is in memory, once per launch and
//! again when the user reloads the library, the tags of every file that is on
//! disk are compared with the library's, in case a tag was edited outside
//! Libri while it was closed (ADR 0008). Nothing here waits for the network or
//! for the downloads to be verified, so a launch that is signed out, offline or
//! failing to fetch still shows its tags and still reads Finder's. Files that
//! were changed, in either direction, are counted in the activity panel and
//! logged one by one. The Tags switches in Native Integration settings turn the
//! launch check, and each direction, on and off.

use dtrpg_core::data::index::IndexError;
use dtrpg_core::data::native_preferences::NativePreferences;
use dtrpg_core::platform::{Platform, PlatformIntegration};
use dtrpg_core::services::tag_projection::{
    ReconcileReport, SyncSettings, TaggedFile, project_indexed_tags, reconcile_with_index,
};

use super::*;

/// Which way tags may move, as the Native Integration settings say.
pub(super) fn sync_settings(preferences: &NativePreferences) -> SyncSettings {
    SyncSettings { write_to_files:  preferences.write_tags_to_files(),
                   read_from_files: preferences.read_tags_from_files(), }
}

/// Returns every file of the items in `catalog` that is on disk, with the item
/// it belongs to.
///
/// Looks at the disk, so it must run in the background. It does not trust the
/// `downloaded` flags, which are only as current as the last verification.
pub(super) fn files_on_disk(catalog: &[LibraryItem], collections: &[CollectionEntry],
                            storage: &StorageConfig)
                            -> Vec<TaggedFile> {
    let mut files = Vec::new();
    for item in catalog {
        let collection_ids = collection_ids_for_item(item, collections);
        for file in &item.files {
            let path = resolved_file_path_in_collections(storage, item, file, &collection_ids);
            if path.is_file() {
                files.push(TaggedFile { item_id: item.id.to_string(),
                                        path });
            }
        }
    }
    files
}

impl LibraryController {
    /// Shows the library's tags, and compares the tags of the files on disk
    /// with them if that has not been done this launch, or `force` says to.
    ///
    /// Called as soon as the catalog saved on disk is in memory, and when the
    /// user reloads the library. One comparison runs at a time, and none runs
    /// before there is a catalog to look at.
    pub(super) fn begin_tag_sync(&mut self, force: bool, cx: &mut Context<Self>) {
        self.load_tags(cx);
        let catalog = self.all_items();
        if catalog.is_empty()
           || self.file_tag_sync_running
           || (self.file_tags_reconciled && !force)
           || !Platform.capabilities().filesystem_tags
        {
            return;
        }
        self.file_tags_reconciled = true;
        self.file_tag_sync_running = true;
        let collections = self.collections.clone();
        let cache_root = self.cache_root.clone();
        cx.spawn(async move |this, async_cx| {
              let outcome = async_cx.background_executor()
                                    .spawn(async move {
                                        let preferences = NativePreferences::load();
                                        if !preferences.reconcile_on_launch() {
                                            return Ok(ReconcileReport::default());
                                        }
                                        let files = files_on_disk(&catalog,
                                                                  &collections,
                                                                  &StorageConfig::load());
                                        reconcile_with_index(&Platform,
                                                             &cache_root,
                                                             &files,
                                                             sync_settings(&preferences))
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
              match project_indexed_tags(&Platform,
                                         &cache_root,
                                         &item_id,
                                         &[path],
                                         sync_settings(&NativePreferences::load()))
              {
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

    /// Adds a finished entry with `label` to the activity panel.
    fn finish_note(&self, label: &str, cx: &mut Context<Self>) {
        self.activity.update(cx, |activity, cx| {
                         let id = activity.start(label, None, cx);
                         activity.complete(id, cx);
                     });
    }

    /// Logs what a reconcile did and tells the user how many files it changed
    /// or could not change. Says nothing if every file already matched.
    pub(super) fn report_file_tags(&mut self, outcome: Result<ReconcileReport, IndexError>,
                                   cx: &mut Context<Self>) {
        self.file_tag_sync_running = false;
        let report = match outcome {
            Ok(report) => report,
            Err(error) => {
                tracing::warn!(%error, "could not read tags from the library index");
                return;
            }
        };
        for path in &report.written {
            tracing::info!(path = %path.display(), "restored a file's tags from the library");
        }
        for path in &report.imported {
            tracing::info!(path = %path.display(), "read a file's tags into the library");
        }
        for failure in &report.failed {
            tracing::warn!(path = %failure.path.display(),
                           error = %failure.error,
                           "could not reconcile a file's tags");
        }
        if !report.written.is_empty() {
            let label = t!("activity.tags_restored", count = report.written.len()).to_string();
            self.finish_note(&label, cx);
        }
        if !report.imported.is_empty() {
            let label = t!("activity.tags_imported", count = report.imported.len()).to_string();
            self.finish_note(&label, cx);
            // Tags read from files changed the index, so show them.
            self.load_tags(cx);
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
