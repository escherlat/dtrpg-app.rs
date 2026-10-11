//! Shared thumbnail/download concurrency limits.

use super::*;

impl LibraryController {
    // ── Shared thumbnail/download concurrency ────────────────────────────────

    /// Returns how many more thumbnail fetches and/or file downloads can be
    /// dispatched right now without exceeding `max_concurrent_downloads`.
    ///
    /// Thumbnail and download slots draw from the same aggregate limit (see
    /// `thumbnail-queue-concurrency`'s "shared limit" requirement).
    pub(super) fn available_slots(&self) -> usize {
        remaining_slots(self.max_concurrent_downloads,
                        self.active_thumbnail_fetches,
                        self.active_downloads)
    }

    /// Updates the shared thumbnail/download concurrency limit and drains
    /// both queues in case the new limit freed up slots.
    ///
    /// Called when [`crate::controllers::settings::SettingsController`]
    /// reports a `SettingsChanged` event, so a change made on the Storage
    /// settings page takes effect immediately.
    pub fn set_max_concurrent_downloads(&mut self, n: usize, cx: &mut Context<Self>) {
        if self.max_concurrent_downloads == n {
            return;
        }
        self.max_concurrent_downloads = n;
        self.drain_thumbnail_queue(cx);
        self.drain_download_queue(cx);
    }

    /// Updates the "Recently Updated" sidebar window and recomputes the
    /// section counts and cached filtered items so the change is reflected
    /// immediately.
    ///
    /// Called when [`crate::controllers::settings::SettingsController`]
    /// reports a `SettingsChanged` event, so a change made on the Storage
    /// settings page takes effect immediately.
    pub fn set_recently_updated_window_days(&mut self, days: u32, cx: &mut Context<Self>) {
        if self.recently_updated_window_days == days {
            return;
        }
        self.recently_updated_window_days = days;
        self.section_counts = section_counts(&self.catalog, now_secs(), days);
        self.invalidate_cache();
        cx.emit(LibraryChanged);
    }
}
