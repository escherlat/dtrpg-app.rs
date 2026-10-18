use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use super::{ContainerEntry, ContainerError, ContainerReader, write_extracted_entry};

pub(super) struct ZipReader {
    path: PathBuf,
}

impl ZipReader {
    pub(super) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn archive(&self) -> Result<zip::ZipArchive<File>, ContainerError> {
        let file = File::open(&self.path).map_err(|error| ContainerError::Io(error.to_string()))?;
        zip::ZipArchive::new(file).map_err(|_| ContainerError::InvalidArchive)
    }
}

impl ContainerReader for ZipReader {
    fn visit_entries(&self, visitor: &mut dyn FnMut(ContainerEntry)) -> Result<(), ContainerError> {
        let mut archive = self.archive()?;
        for index in 0..archive.len() {
            let entry = archive
                .by_index_raw(index)
                .map_err(|_| ContainerError::InvalidArchive)?;
            let name = entry
                .name()
                .map(|name| name.to_string())
                .unwrap_or_else(|_| String::from_utf8_lossy(entry.name_raw()).into_owned());
            visitor(ContainerEntry {
                is_directory: entry.is_dir(),
                name,
                size_bytes: entry.size(),
            });
        }
        Ok(())
    }

    fn read_entry(&self, name: &str) -> Result<Vec<u8>, ContainerError> {
        let mut archive = self.archive()?;
        let mut entry = archive
            .by_name(name)
            .map_err(|_| ContainerError::EntryNotFound(name.to_string()))?;
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| ContainerError::Io(error.to_string()))?;
        Ok(bytes)
    }

    fn extract_entries(
        &self, names: &[&str], destination: &std::path::Path,
    ) -> Result<(), ContainerError> {
        for name in names {
            let bytes = self.read_entry(name)?;
            write_extracted_entry(destination, name, &bytes)?;
        }
        Ok(())
    }
}
