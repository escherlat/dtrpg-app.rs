//! Archive operations exposed by the library facade.

use std::path::Path;

use crate::api::LibraryApi;
use crate::util::container::{ContainerEntry, ContainerError};

impl LibraryApi {
    /// Lists entries in a local archive without requiring callers to know its
    /// concrete format.
    pub fn list_archive_entries(&self, path: &Path) -> Result<Vec<ContainerEntry>, ContainerError> {
        crate::util::container::list_entries(path)
    }

    /// Extracts selected archive entries below `destination`.
    pub fn extract_archive_entries(&self, path: &Path, names: &[&str], destination: &Path)
                                   -> Result<(), ContainerError> {
        crate::util::container::open(path)?.extract_entries(names, destination)
    }
}
