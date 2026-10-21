//! The set of entries whose files are being checked against disk.

use std::collections::HashSet;
use std::sync::{Arc, PoisonError};

use crate::library::Library;

impl Library {
    /// Marks `ids` as having their files checked against disk (see
    /// `verify-downloaded-status-against-disk`).
    ///
    /// Distinct from the checking-items set, which tracks the network-bound
    /// availability check, so that the views can show the two differently.
    pub fn begin_verifying(&self, ids: impl IntoIterator<Item = Arc<str>>) {
        self.verifying
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(ids);
    }

    /// Clears the mark [`Self::begin_verifying`] set on `ids`.
    pub fn end_verifying<'id>(&self, ids: impl IntoIterator<Item = &'id Arc<str>>) {
        let mut verifying = self.verifying
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner);
        for id in ids {
            verifying.remove(id);
        }
    }

    /// Whether the files of entry `id` are being checked against disk.
    #[must_use]
    pub fn is_verifying(&self, id: &str) -> bool {
        self.verifying
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(id)
    }

    /// The entries whose files are being checked against disk right now.
    #[must_use]
    pub fn verifying_snapshot(&self) -> HashSet<Arc<str>> {
        self.verifying
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::data::storage::StorageConfig;

    fn open_library() -> Library {
        let dir = tempfile::tempdir().expect("tempdir");
        Library::open(dir.keep(), &StorageConfig::for_test(std::env::temp_dir())).expect("open")
                                                                                 .0
    }

    #[test]
    fn an_entry_is_verifying_between_begin_and_end() {
        let library = open_library();
        let id: Arc<str> = "a".into();

        library.begin_verifying([Arc::clone(&id)]);
        assert!(library.is_verifying("a"));
        assert_eq!(library.verifying_snapshot().len(), 1);

        library.end_verifying([&id]);
        assert!(!library.is_verifying("a"));
    }
}
