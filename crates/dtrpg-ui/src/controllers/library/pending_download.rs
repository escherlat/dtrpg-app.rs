//! Pending download confirmation state.

use super::*;

/// A user-initiated download-queuing action stashed while a
/// [`LowDiskSpaceWarning`] confirmation is pending.
///
/// One variant per `request_*` gating wrapper — see `LibraryController`'s
/// `request_download`, `request_item_download`,
/// `request_download_all_for_collection`, `request_download_all_for_publisher`,
/// and `request_download_all_for_filter`.
pub(super) enum PendingDownloadRequest {
    /// Stashed by [`LibraryController::request_download`].
    Item {
        /// Entry id to pass to [`LibraryController::enqueue_download`].
        id:    Arc<str>,
        /// Display title to pass to [`LibraryController::enqueue_download`].
        title: String,
    },
    /// Stashed by [`LibraryController::request_item_download`].
    ItemFile {
        /// Entry id to pass to [`LibraryController::enqueue_item_download`].
        id:    Arc<str>,
        /// File index to pass to [`LibraryController::enqueue_item_download`].
        index: u32,
        /// Display title to pass to
        /// [`LibraryController::enqueue_item_download`].
        title: String,
    },
    /// Stashed by [`LibraryController::request_download_all_for_collection`].
    Collection {
        /// Collection id to pass to
        /// [`LibraryController::download_all_for_collection`].
        collection_id: u64,
    },
    /// Stashed by [`LibraryController::request_download_all_for_publisher`].
    Publisher {
        /// Publisher name to pass to
        /// [`LibraryController::download_all_for_publisher`].
        publisher: Arc<str>,
    },
    /// Stashed by [`LibraryController::request_download_all_for_filter`].
    Filter {
        /// Sidebar filter to pass to
        /// [`LibraryController::download_all_for_filter`].
        filter: SidebarFilter,
    },
}
