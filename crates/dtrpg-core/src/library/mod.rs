//! Headless library session.
//!
//! `Library` owns the long-lived state that the desktop UI's
//! `LibraryController` used to hold directly: the SQLite index, the work
//! queue, the cover fetcher, the network monitor, and — since ADR 0002
//! Slice 2 — the catalog-sync state machine. It is the core side of the ADR
//! 0002 controller decomposition; later slices will move more catalog/queue
//! logic here and thin the UI controller to a view-model.

mod session;
mod sync;

pub use session::Library;
pub use sync::{LibraryEvent, LibraryEvents, SyncStage, reload_cooldown_active};
