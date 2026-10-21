//! Headless library session.
//!
//! `Library` owns the long-lived state that the desktop UI's
//! `LibraryController` used to hold directly: the SQLite index, the work
//! queue, the cover fetcher, the network monitor, the catalog-sync state
//! machine (Slice 2), the filtered/grouped query cache (Slice 3), and the
//! thumbnail/download/check job orchestration (Slice 4). It is the core
//! side of the ADR 0002 controller decomposition; later slices will move
//! more catalog-mutation logic here and thin the UI controller to a
//! view-model.

mod jobs;
mod queries;
mod session;
mod sync;

pub use jobs::{CheckJob, CheckResult, DownloadJob, DownloadKey, DownloadSubmission, ThumbnailJob};
pub use queries::ViewState;
pub use session::Library;
pub use sync::{LibraryEvent, LibraryEvents, SyncStage, reload_cooldown_active};
