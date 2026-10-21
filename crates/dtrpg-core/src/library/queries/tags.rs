//! Plain storage for the library's tags.
//!
//! The async load-from-index-and-replace mechanism (reading tags off the
//! index in the background, its own generation counter) stays on the UI
//! controller for now — it's a `gpui`-adjacent async loop, shaped like ADR
//! 0002 Slice 2's catalog-sync move, not a filtered-query cache. This just
//! gives the controller somewhere other than its own field to put the
//! result.

use std::sync::{Arc, PoisonError};

use crate::data::tag_library::TagLibrary;
use crate::library::Library;

impl Library {
    /// The library's tags as they were last set via [`Self::set_tags`].
    #[must_use]
    pub fn tags(&self) -> Arc<TagLibrary> {
        Arc::clone(&self.tags.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Replaces the library's tags.
    pub fn set_tags(&self, tags: TagLibrary) {
        *self.tags.lock().unwrap_or_else(PoisonError::into_inner) = Arc::new(tags);
    }
}
