//! Plain storage for the library's tags.
//!
//! The caller reads the tags off the index on a background thread and puts
//! the result here. A generation counter, handed out when a read starts, drops
//! a result that arrives after a newer read was started.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, PoisonError};

use crate::data::index::{IndexError, LibraryIndex};
use crate::data::tag_library::TagLibrary;
use crate::library::Library;

impl Library {
    /// The library's tags as they were last set via [`Self::set_tags`].
    #[must_use]
    pub fn tags(&self) -> Arc<TagLibrary> {
        Arc::clone(&self.tags.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Reads the tags from the library index under `cache_root`. Blocking:
    /// call it off the UI thread.
    ///
    /// # Errors
    ///
    /// Returns [`IndexError`] if the index cannot be opened or read.
    pub fn read_tags(cache_root: &Path) -> Result<TagLibrary, IndexError> {
        TagLibrary::load(&LibraryIndex::open_and_migrate(cache_root)?)
    }

    /// Starts a read or an edit of the tags and returns its generation, to
    /// pass to [`Self::set_tags_if_current`] with the result.
    pub fn begin_tag_read(&self) -> u64 {
        self.tag_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Replaces the library's tags with `tags` unless a newer read was
    /// started since `generation`. Returns whether it did.
    pub fn set_tags_if_current(&self, tags: TagLibrary, generation: u64) -> bool {
        if generation < self.tag_generation.load(Ordering::SeqCst) {
            return false;
        }
        self.set_tags(tags);
        true
    }

    /// Replaces the library's tags.
    pub fn set_tags(&self, tags: TagLibrary) {
        *self.tags.lock().unwrap_or_else(PoisonError::into_inner) = Arc::new(tags);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::data::storage::StorageConfig;
    use crate::data::tag_library::TagLibrary;
    use crate::library::Library;

    fn open_library() -> Library {
        let dir = tempfile::tempdir().expect("tempdir");
        Library::open(dir.keep(), &StorageConfig::for_test(std::env::temp_dir())).expect("open")
                                                                                 .0
    }

    fn tags_of_a(tag: &str) -> TagLibrary {
        TagLibrary::new([("a".to_owned(), vec![tag.to_owned()])].into(),
                        Default::default())
    }

    #[test]
    fn the_latest_read_is_kept() {
        let library = open_library();
        let generation = library.begin_tag_read();

        assert!(library.set_tags_if_current(tags_of_a("new"), generation));
        assert_eq!(library.tags().tags_of("a").len(), 1);
    }

    #[test]
    fn a_result_that_arrives_after_a_newer_read_began_is_dropped() {
        let library = open_library();
        let older = library.begin_tag_read();
        library.begin_tag_read();

        assert!(!library.set_tags_if_current(tags_of_a("old"), older));
        assert!(library.tags().tags_of("a").is_empty());
    }
}
