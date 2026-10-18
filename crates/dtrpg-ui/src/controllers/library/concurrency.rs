//! Shared thumbnail/download concurrency limits.

use super::*;

impl LibraryController {
    // ── Shared thumbnail/download concurrency ────────────────────────────────

    /// Returns how many more file downloads can be dispatched right now
    /// without exceeding `max_concurrent_downloads`.
    ///
    /// Thumbnail fetches no longer count here: they run on the work queue,
    /// which applies the same limit to them separately. Downloads move onto
    /// that queue next, which makes the limit shared again.
    pub(super) fn available_slots(&self) -> usize {
        remaining_slots(self.max_concurrent_downloads, 0, self.active_downloads)
    }

    /// Updates the concurrency limit for thumbnails and downloads, and starts
    /// waiting work in case the new limit freed up slots.
    ///
    /// Called when [`crate::controllers::settings::SettingsController`]
    /// reports a `SettingsChanged` event, so a change made on the Storage
    /// settings page takes effect immediately.
    pub fn set_max_concurrent_downloads(&mut self, n: usize, cx: &mut Context<Self>) {
        if self.max_concurrent_downloads == n {
            return;
        }
        self.max_concurrent_downloads = n;
        self.work.set_limit(n);
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
