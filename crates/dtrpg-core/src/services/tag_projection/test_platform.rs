//! A platform that keeps file tags in memory, and an in-memory index to go with
//! it, for tests of the tag services.

#![allow(clippy::unwrap_used)]

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::data::enums::ItemStatus;
use crate::data::index::LibraryIndex;
use crate::data::library::LibraryItem;
use crate::platform::{Capabilities, FileState, FileTag, PlatformIntegration, PlatformKind};
use crate::services::tag_projection::TaggedFile;
use crate::util::datetime::now_secs;

/// What a fake file holds.
#[derive(Clone, Default)]
struct FakeFile {
    tags:       Vec<FileTag>,
    changed_at: Option<i64>,
}

/// A platform whose files hold their tags in a map.
pub(crate) struct InMemory {
    can_tag: bool,
    files:   Mutex<HashMap<PathBuf, FakeFile>>,
    broken:  HashSet<PathBuf>,
    writes:  Mutex<Vec<PathBuf>>,
}

impl InMemory {
    pub(crate) fn new() -> Self {
        Self { can_tag: true,
               files:   Mutex::new(HashMap::new()),
               broken:  HashSet::new(),
               writes:  Mutex::new(Vec::new()), }
    }

    pub(crate) fn without_tags_on_files() -> Self {
        Self { can_tag: false,
               ..Self::new() }
    }

    pub(crate) fn with_broken(mut self, path: &str) -> Self {
        self.broken.insert(PathBuf::from(path));
        self
    }

    /// Edits a file's tags as if outside Libri, at `changed_at`.
    pub(crate) fn edit(&self, path: &str, tags: &[FileTag], changed_at: Option<i64>) {
        self.files.lock().unwrap().insert(PathBuf::from(path),
                                          FakeFile { tags: tags.to_vec(),
                                                     changed_at });
    }

    pub(crate) fn tags_of(&self, path: &str) -> Vec<FileTag> {
        self.files
            .lock()
            .unwrap()
            .get(Path::new(path))
            .map(|file| file.tags.clone())
            .unwrap_or_default()
    }

    pub(crate) fn write_count(&self) -> usize {
        self.writes.lock().unwrap().len()
    }
}

impl PlatformIntegration for InMemory {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Linux
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { filesystem_tags: self.can_tag,
                       ..Capabilities::default() }
    }

    fn file_state(&self, _path: &Path) -> io::Result<FileState> {
        Ok(FileState::Local)
    }

    fn reveal(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }

    fn read_tags(&self, path: &Path) -> io::Result<Vec<FileTag>> {
        if self.broken.contains(path) {
            return Err(io::Error::other("read-only volume"));
        }
        Ok(self.tags_of(&path.to_string_lossy()))
    }

    fn write_tags(&self, path: &Path, tags: &[FileTag]) -> io::Result<()> {
        if self.broken.contains(path) {
            return Err(io::Error::other("read-only volume"));
        }
        self.writes.lock().unwrap().push(path.to_path_buf());
        self.files.lock().unwrap().insert(path.to_path_buf(),
                                          FakeFile { tags:       tags.to_vec(),
                                                     changed_at: Some(now_secs()), });
        Ok(())
    }

    fn tags_changed_at(&self, path: &Path) -> io::Result<Option<i64>> {
        Ok(self.files
               .lock()
               .unwrap()
               .get(path)
               .and_then(|file| file.changed_at))
    }
}

pub(crate) fn plain(names: &[&str]) -> Vec<FileTag> {
    names.iter().map(|name| FileTag::plain(*name)).collect()
}

pub(crate) fn file(item_id: &str, path: &str) -> TaggedFile {
    TaggedFile { item_id: item_id.to_owned(),
                 path:    PathBuf::from(path), }
}

pub(crate) fn item(id: &str) -> LibraryItem {
    LibraryItem::new(id,
                     &format!("Title {id}"),
                     "Free League",
                     "",
                     "Core",
                     "PDF",
                     100,
                     10.0,
                     2024,
                     0,
                     ItemStatus::Downloaded,
                     "#1C2A44",
                     "A game.",
                     Some(1_700_000_000))
}

/// An in-memory index holding the items `ids`.
pub(crate) fn index_with(ids: &[&str]) -> LibraryIndex {
    let mut index = LibraryIndex::open_in_memory().unwrap();
    let items: Vec<_> = ids.iter().map(|id| item(id)).collect();
    index.replace_all_items(&items).unwrap();
    index
}
