//! Activity controller: tracks in-progress and recently-completed background
//! operations.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui::Context;

use crate::data::activity::{
    ActivityItem, ActivitySnapshot, ActivityStatus, AlertEntry, AlertHistorySnapshot,
};
use crate::data::constants::{ALERT_LOG_CAP, ERROR_EXPIRY_SECS, EXPIRY_SECS, RECENT_CAP};
use crate::data::events::{ActivityChanged, DownloadComplete, DownloadError};

/// Owns the activity item list and panel open/close state.
pub struct ActivityController {
    next_id:          u64,
    in_progress:      Vec<ActivityItem>,
    recent:           VecDeque<ActivityItem>,
    panel_open:       bool,
    /// The id of the item row currently expanded in the panel, if any.
    selected_id:      Option<u64>,
    /// Durable, non-expiring log of error-status activity items.
    ///
    /// Never cleared by [`clear`](Self::clear) — this is a session-long record
    /// independent of the active service, capped at [`ALERT_LOG_CAP`].
    alert_log:        VecDeque<AlertEntry>,
    /// Whether the alert history panel overlay is open.
    alert_panel_open: bool,
    /// Whether an alert has been logged since the panel was last opened.
    ///
    /// Drives the status bar notification button's unread badge — set on
    /// every [`push_alert`](Self::push_alert), cleared when the panel is
    /// opened.
    has_unread_alert: bool,
}

impl ActivityController {
    /// Creates a new controller with no items and the panel closed.
    pub fn new() -> Self {
        Self { next_id:          0,
               in_progress:      Vec::new(),
               recent:           VecDeque::with_capacity(RECENT_CAP),
               panel_open:       false,
               selected_id:      None,
               alert_log:        VecDeque::with_capacity(ALERT_LOG_CAP),
               alert_panel_open: false,
               has_unread_alert: false, }
    }

    /// Clears all activity items (in-progress and recent) and closes the panel.
    ///
    /// Used when replacing the library service so stale error messages don't
    /// persist. The durable alert history log is intentionally left intact.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.in_progress.clear();
        self.recent.clear();
        self.selected_id = None;
        cx.emit(ActivityChanged);
    }

    /// Registers a new in-progress operation with the given label.
    ///
    /// `cancel_fn` is an optional callback the UI invokes when the user clicks
    /// the cancel button. Pass `None` for operations that do not support
    /// cancellation.
    ///
    /// Returns the assigned activity id, which the caller must pass to
    /// [`complete`] or [`error`].
    pub fn start(&mut self, label: &str,
                 cancel_fn: Option<Arc<dyn Fn() + Send + Sync + 'static>>,
                 cx: &mut Context<Self>)
                 -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.in_progress.push(ActivityItem { id,
                                             label: Arc::from(label),
                                             status: ActivityStatus::InProgress,
                                             started_at: Instant::now(),
                                             elapsed_secs: None,
                                             progress: None,
                                             cancel_fn });
        cx.emit(ActivityChanged);
        id
    }

    /// Updates the display label of an in-progress item.
    ///
    /// No-op if `id` is not found in the in-progress list.
    pub fn update_label(&mut self, id: u64, label: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(item) = self.in_progress.iter_mut().find(|i| i.id == id) {
            let s: String = label.into();
            item.label = Arc::from(s.as_str());
            cx.emit(ActivityChanged);
        }
    }

    /// Resolves an in-progress item as successfully completed and schedules its
    /// 15-second expiry.
    ///
    /// No-op if `id` is not found in the in-progress list.
    pub fn complete(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(pos) = self.in_progress.iter().position(|i| i.id == id) {
            let mut item = self.in_progress.remove(pos);
            item.elapsed_secs = Some(item.started_at.elapsed().as_secs());
            item.status = ActivityStatus::Complete;
            let title = item.label.clone();
            self.push_recent(item);
            cx.emit(ActivityChanged);
            cx.emit(DownloadComplete { title });
            cx.spawn(async move |this, async_cx| {
                  async_cx.background_executor()
                          .timer(Duration::from_secs(EXPIRY_SECS))
                          .await;
                  this.update(async_cx, |a, cx| a.expire_item(id, cx)).ok();
              })
              .detach();
        }
    }

    /// Resolves an in-progress item as failed and schedules its 2-minute
    /// expiry.
    ///
    /// No-op if `id` is not found in the in-progress list.
    pub fn error(&mut self, id: u64, message: String, cx: &mut Context<Self>) {
        if let Some(pos) = self.in_progress.iter().position(|i| i.id == id) {
            let mut item = self.in_progress.remove(pos);
            item.elapsed_secs = Some(item.started_at.elapsed().as_secs());
            let title = item.label.clone();
            item.status = ActivityStatus::Error(message.clone());
            self.push_alert(AlertEntry { id:          item.id,
                                         label:       title.clone(),
                                         message:     message.clone(),
                                         occurred_at: SystemTime::now(), });
            self.push_recent(item);
            cx.emit(ActivityChanged);
            cx.emit(DownloadError { title, message });
            cx.spawn(async move |this, async_cx| {
                  async_cx.background_executor()
                          .timer(Duration::from_secs(ERROR_EXPIRY_SECS))
                          .await;
                  this.update(async_cx, |a, cx| a.expire_item(id, cx)).ok();
              })
              .detach();
        }
    }

    /// Appends a failure directly to the durable alert log, without a
    /// corresponding in-progress entry.
    ///
    /// For failures in optimistic-update flows that never call [`start`]
    /// (e.g. collection membership add/remove) — unlike [`error`], this does
    /// not touch `in_progress` or `recent`, since there is no in-progress
    /// item to resolve and registering one for an already-applied optimistic
    /// change would misrepresent it as a pending background operation.
    ///
    /// [`start`]: Self::start
    /// [`error`]: Self::error
    pub fn log_alert(&mut self, label: impl Into<Arc<str>>, message: String,
                     cx: &mut Context<Self>) {
        let id = self.next_id;
        self.next_id += 1;
        self.push_alert(AlertEntry { id,
                                     label: label.into(),
                                     message,
                                     occurred_at: SystemTime::now() });
        cx.emit(ActivityChanged);
    }

    /// Removes an in-progress item without resolving it to complete or error.
    ///
    /// Used when a cancelled operation must disappear from the panel entirely
    /// rather than linger as a completed/errored entry (see the
    /// `download-queue` capability's in-progress cancellation requirement).
    /// No-op if `id` is not found in the in-progress list.
    pub fn remove_in_progress(&mut self, id: u64, cx: &mut Context<Self>) {
        let before = self.in_progress.len();
        self.in_progress.retain(|i| i.id != id);
        if self.in_progress.len() != before {
            if self.selected_id == Some(id) {
                self.selected_id = None;
            }
            cx.emit(ActivityChanged);
        }
    }

    /// Removes an expired item from the recent list by id.
    ///
    /// No-op if the item was already evicted by the cap or a prior expiry.
    /// Emits [`ActivityChanged`] only when an item is actually removed.
    pub fn expire_item(&mut self, id: u64, cx: &mut Context<Self>) {
        let before = self.recent.len();
        self.recent.retain(|i| i.id != id);
        if self.recent.len() != before {
            if self.selected_id == Some(id) {
                self.selected_id = None;
            }
            cx.emit(ActivityChanged);
        }
    }

    /// Toggles the expanded row in the activity panel.
    ///
    /// Selects `id` if it differs from the current selection, or collapses if
    /// the same id.
    pub fn select_activity(&mut self, id: u64, cx: &mut Context<Self>) {
        self.selected_id = if self.selected_id == Some(id) {
            None
        }
        else {
            Some(id)
        };
        cx.emit(ActivityChanged);
    }

    /// Updates the progress value for an in-progress item, clamped to [0.0,
    /// 1.0].
    ///
    /// No-op if `id` is not found in the in-progress list.
    pub fn update_progress(&mut self, id: u64, progress: f32, cx: &mut Context<Self>) {
        if let Some(item) = self.in_progress.iter_mut().find(|i| i.id == id) {
            item.progress = Some(progress.clamp(0.0, 1.0));
            cx.emit(ActivityChanged);
        }
    }

    /// Calls the stored cancel function (if any) and transitions the item to
    /// Error("Cancelled").
    ///
    /// No-op if `id` is not found in the in-progress list.
    pub fn cancel_activity(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(item) = self.in_progress.iter().find(|i| i.id == id)
           && let Some(cancel_fn) = item.cancel_fn.clone()
        {
            cancel_fn();
        }
        self.error(id, "Cancelled".to_string(), cx);
    }

    /// Toggles the activity panel overlay open or closed.
    pub fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        self.panel_open = !self.panel_open;
        cx.emit(ActivityChanged);
    }

    /// Sets the activity panel overlay's open state directly.
    ///
    /// Used by the status bar's anchored `Popover`, which reports its own
    /// resolved open state on each toggle rather than expecting the caller
    /// to flip a bool.
    pub fn set_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.panel_open != open {
            self.panel_open = open;
            cx.emit(ActivityChanged);
        }
    }

    /// Toggles the alert history panel overlay open or closed.
    pub fn toggle_alert_panel(&mut self, cx: &mut Context<Self>) {
        self.alert_panel_open = !self.alert_panel_open;
        if self.alert_panel_open {
            self.has_unread_alert = false;
        }
        cx.emit(ActivityChanged);
    }

    /// Sets the alert history panel overlay's open state directly.
    ///
    /// Used by the status bar's anchored `Popover`, which reports its own
    /// resolved open state on each toggle rather than expecting the caller
    /// to flip a bool.
    pub fn set_alert_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.alert_panel_open != open {
            self.alert_panel_open = open;
            if open {
                self.has_unread_alert = false;
            }
            cx.emit(ActivityChanged);
        }
    }

    /// Removes all entries from the durable alert history log.
    pub fn clear_alert_log(&mut self, cx: &mut Context<Self>) {
        self.alert_log.clear();
        self.has_unread_alert = false;
        cx.emit(ActivityChanged);
    }

    /// Returns a snapshot of the alert history log for rendering.
    #[must_use]
    pub fn alert_snapshot(&self) -> AlertHistorySnapshot {
        AlertHistorySnapshot { open:       self.alert_panel_open,
                               entries:    self.alert_log.iter().cloned().collect(),
                               has_unread: self.has_unread_alert, }
    }

    /// Returns a snapshot of current activity state for rendering.
    pub fn snapshot(&self) -> ActivitySnapshot {
        let mut items: Vec<ActivityItem> = self.in_progress.clone();
        items.extend(self.recent.iter().cloned());

        ActivitySnapshot { in_progress_count: self.in_progress.len(),
                           alert_count: self.alert_log.len(),
                           recent_count: self.recent.len(),
                           panel_open: self.panel_open,
                           items,
                           selected_id: self.selected_id,
                           aggregate_progress: self.aggregate_progress() }
    }

    /// Computes the mean of known `progress` values among in-progress items.
    ///
    /// Returns `None` (indeterminate) when there are no in-progress items, or
    /// when none of them report a known progress value.
    fn aggregate_progress(&self) -> Option<f32> {
        let known: Vec<f32> = self.in_progress.iter().filter_map(|i| i.progress).collect();
        if known.is_empty() {
            return None;
        }
        Some(known.iter().sum::<f32>() / known.len() as f32)
    }

    fn push_recent(&mut self, item: ActivityItem) {
        if self.recent.len() == RECENT_CAP {
            self.recent.pop_back();
        }
        self.recent.push_front(item);
    }

    /// Appends an entry to the durable alert log, evicting the oldest entry if
    /// full, and marks the alert log as having an unread entry.
    fn push_alert(&mut self, entry: AlertEntry) {
        if self.alert_log.len() == ALERT_LOG_CAP {
            self.alert_log.pop_back();
        }
        self.alert_log.push_front(entry);
        self.has_unread_alert = true;
    }
}

impl Default for ActivityController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Instant, SystemTime};

    use gpui::{AppContext, TestAppContext};

    use super::*;

    /// Directly seeds `alert_log`/`recent` rather than calling `error()`/
    /// `expire_item()` (which require `Context<Self>`, unavailable without a
    /// GPUI test app — no other controller in this crate exercises
    /// `cx`-requiring methods in unit tests either). This still exercises
    /// exactly the behavior in question: `snapshot().alert_count` must track
    /// `alert_log`, independent of `recent`.
    fn alert_entry(id: u64) -> AlertEntry {
        AlertEntry { id,
                     label: "Sync failed".into(),
                     message: "network error".to_string(),
                     occurred_at: SystemTime::now() }
    }

    fn error_item(id: u64) -> ActivityItem {
        ActivityItem { id,
                       label: "Sync".into(),
                       status: ActivityStatus::Error("network error".to_string()),
                       started_at: Instant::now(),
                       elapsed_secs: Some(1),
                       progress: None,
                       cancel_fn: None }
    }

    #[test]
    fn alert_count_reflects_alert_log_len_not_recent() {
        let mut ctrl = ActivityController::new();
        ctrl.alert_log.push_back(alert_entry(1));
        ctrl.alert_log.push_back(alert_entry(2));
        // `recent` has a different count of error items than `alert_log` —
        // `alert_count` must follow `alert_log`, not this.
        ctrl.recent.push_back(error_item(1));

        assert_eq!(ctrl.snapshot().alert_count, 2);
    }

    #[test]
    fn clearing_alert_log_zeroes_alert_count() {
        let mut ctrl = ActivityController::new();
        ctrl.push_alert(alert_entry(1));
        assert_eq!(ctrl.snapshot().alert_count, 1);

        ctrl.alert_log.clear();
        ctrl.has_unread_alert = false;

        assert_eq!(ctrl.snapshot().alert_count, 0);
    }

    #[test]
    fn recent_expiry_does_not_affect_alert_count() {
        let mut ctrl = ActivityController::new();
        ctrl.push_alert(alert_entry(1));
        ctrl.recent.push_back(error_item(1));
        assert_eq!(ctrl.snapshot().alert_count, 1);

        // Simulate the `recent` expiry timer removing the transient item —
        // the durable `alert_log` entry (and thus `alert_count`) is
        // unaffected.
        ctrl.recent.clear();

        assert_eq!(ctrl.snapshot().alert_count, 1);
    }

    // `log_alert` takes `Context<Self>`, so exercising it (rather than
    // seeding `alert_log` directly, as the tests above do) needs a real
    // entity via `gpui::TestAppContext` - the `test-support` feature is
    // enabled for `gpui` under `[dev-dependencies]` for this.
    #[gpui::test]
    fn log_alert_appends_entry_without_touching_in_progress_or_recent(cx: &mut TestAppContext) {
        let ctrl = cx.new(|_| ActivityController::new());

        ctrl.update(cx, |ctrl, cx| {
                ctrl.log_alert("Add to collection 'Favorites'",
                               "network error".to_string(),
                               cx);
            });

        let alerts = ctrl.read_with(cx, |ctrl, _| ctrl.alert_snapshot());
        assert_eq!(alerts.entries.len(), 1);
        assert_eq!(alerts.entries[0].label.as_ref(),
                   "Add to collection 'Favorites'");
        assert_eq!(alerts.entries[0].message, "network error");

        let snapshot = ctrl.read_with(cx, |ctrl, _| ctrl.snapshot());
        assert!(snapshot.items.is_empty());
        assert_eq!(snapshot.in_progress_count, 0);
        assert_eq!(snapshot.recent_count, 0);
    }
}
