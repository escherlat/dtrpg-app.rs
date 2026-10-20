//! File-opener overrides: per-extension application associations, and the
//! in-place "add file opener" flow driven from the File Openers list.

use super::*;

impl SettingsController {
    /// Returns a shared reference to the file-opener config.
    pub fn file_openers(&self) -> &FileOpenerConfig {
        &self.file_openers
    }

    /// Adds or replaces a file-opener entry and persists the change.
    pub fn add_file_opener(&mut self, entry: FileOpenerEntry, cx: &mut Context<Self>)
                           -> AddOutcome {
        let outcome = self.file_openers.add(entry);
        self.file_openers.save();
        cx.emit(SettingsChanged);
        outcome
    }

    /// Removes the file-opener entry for `extension` and persists the change.
    pub fn remove_file_opener(&mut self, extension: &str, cx: &mut Context<Self>) {
        self.file_openers.remove(extension);
        self.file_openers.save();
        cx.emit(SettingsChanged);
    }

    /// Begins an in-place "add file opener" flow after the user has picked an
    /// application via the native file dialog.
    ///
    /// The extension is entered inline in the File Openers list rather than in
    /// a separate modal; the list renders a pending row for `app_path`
    /// until the flow is committed or cancelled.
    pub fn begin_add_file_opener(&mut self, app_path: PathBuf, cx: &mut Context<Self>) {
        self.pending_file_opener = Some(app_path);
        cx.emit(SettingsChanged);
    }

    /// Cancels an in-progress "add file opener" flow without persisting
    /// anything.
    pub fn cancel_pending_file_opener(&mut self, cx: &mut Context<Self>) {
        if self.pending_file_opener.take().is_some() {
            cx.emit(SettingsChanged);
        }
    }

    /// Commits the in-progress "add file opener" flow using `extension`.
    ///
    /// No-op if no add is in progress or `extension` is empty after trimming
    /// (the caller should
    /// [`cancel_pending_file_opener`](Self::cancel_pending_file_opener)
    /// explicitly in that case, e.g. on blur).
    pub fn commit_pending_file_opener(&mut self, extension: &str, cx: &mut Context<Self>) {
        let trimmed = extension.trim();
        if trimmed.is_empty() {
            return;
        }
        let Some(app_path) = self.pending_file_opener.take()
        else {
            return;
        };
        self.add_file_opener(FileOpenerEntry { extension: trimmed.to_string(),
                                               app_path },
                             cx);
    }
}
