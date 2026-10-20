//! Starts a catalog-sync run on the core `Library` session and reports it
//! through a single activity item.
//!
//! The sync sequence itself (disk cache, collections, auto-load policy,
//! live fetch) lives in `dtrpg_core::library::Library::start_load` since ADR
//! 0002 Slice 2 — this file only starts the activity item and hands off to
//! it. The activity item's label/progress is then driven from
//! `Self::handle_sync_event` (see `lifecycle.rs`) as `Library`'s events
//! arrive.

use super::*;

impl LibraryController {
    pub(super) fn start_load_inner(&mut self, cx: &mut Context<Self>, force_reload: bool) {
        let service_arc = self.vm.service_arc();
        let collections_service = Arc::clone(&self.collections_service);

        let started = self.library
                          .start_load(service_arc, collections_service, force_reload);
        // When a sync was already in flight, `Library` recorded this call as
        // a pending follow-up (see `Library::start_load`'s single-flight
        // gate) — no activity item for a run that hasn't started yet; its
        // `Finished { reload_requested: true }` event will call `start_load`
        // again once the in-flight run completes.
        if started {
            let id = self.activity.update(cx, |a, cx| {
                                      a.start(&t!("activity.loading_library"), None, cx)
                                  });
            self.catalog_activity_id = Some(id);
        }
    }
}
