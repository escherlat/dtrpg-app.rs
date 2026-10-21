//! Headless library session.
//!
//! `Library` owns the long-lived state that the desktop UI's
//! `LibraryController` used to hold directly: the SQLite index, the work
//! queue, the cover fetcher, the network monitor, the catalog-sync state
//! machine (Slice 2), the filtered/grouped query cache (Slice 3), the
//! thumbnail/download/check job orchestration (Slice 4), and the catalog
//! mutations (Slice 5). It is the core side of the ADR 0002 controller
//! decomposition.

mod jobs;
mod mutations;
mod queries;
mod session;
mod sync;

pub use jobs::{CheckJob, CheckResult, DownloadJob, DownloadKey, DownloadSubmission, ThumbnailJob};
pub use mutations::{
    RemovedMember, add_member, apply_check_result, remove_member, restore_removed_member,
    revert_added_member,
};
pub use queries::ViewState;
pub use session::Library;
pub use sync::{LibraryEvent, LibraryEvents, SyncStage, reload_cooldown_active};
