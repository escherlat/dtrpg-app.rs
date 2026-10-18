//! A place for a job to leave the value it produced.

use std::sync::{Arc, Mutex, PoisonError};

/// Holds one value produced by a job until its submitter collects it.
///
/// A job's closure returns only success or failure, so a job that produces a
/// value (fetched bytes, say) puts it in a slot that the submitter also holds.
/// The value is put before the closure returns, so it is there by the time the
/// job's `Finished` event arrives. Cloning a slot gives another handle to the
/// same value.
///
/// # Examples
///
/// ```
/// use dtrpg_core::work::ResultSlot;
///
/// let slot = ResultSlot::new();
/// slot.put(42);
/// assert_eq!(slot.take(), Some(42));
/// assert_eq!(slot.take(), None);
/// ```
pub struct ResultSlot<T> {
    value: Arc<Mutex<Option<T>>>,
}

impl<T> ResultSlot<T> {
    /// Creates an empty slot.
    #[must_use]
    pub fn new() -> Self {
        Self { value: Arc::new(Mutex::new(None)), }
    }

    /// Stores `value`, replacing any value already there.
    pub fn put(&self, value: T) {
        *self.lock() = Some(value);
    }

    /// Removes and returns the value, or `None` if there is none.
    #[must_use]
    pub fn take(&self) -> Option<T> {
        self.lock().take()
    }

    /// A poisoned lock only means a holder panicked; the `Option` inside is
    /// always valid, so it is safe to keep using.
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<T>> {
        self.value.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl<T> Clone for ResultSlot<T> {
    fn clone(&self) -> Self {
        Self { value: Arc::clone(&self.value), }
    }
}

impl<T> Default for ResultSlot<T> {
    fn default() -> Self {
        Self::new()
    }
}
