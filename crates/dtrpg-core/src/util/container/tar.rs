use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use xz2::read::XzDecoder;

use super::{
    ContainerEntry, ContainerError, ContainerFormat, ContainerReader, write_extracted_entry,
};

pub(super) struct TarReader {
    path:   PathBuf,
    format: ContainerFormat,
}

enum TarInput {
    Plain(File),
    Gzip(GzDecoder<File>),
    Bzip2(BzDecoder<File>),
    Xz(XzDecoder<File>),
}

impl Read for TarInput {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(reader) => reader.read(buffer),
            Self::Gzip(reader) => reader.read(buffer),
            Self::Bzip2(reader) => reader.read(buffer),
            Self::Xz(reader) => reader.read(buffer),
        }
    }
}

impl TarReader {
    pub(super) fn new(path: PathBuf, format: ContainerFormat) -> Self {
        Self { path, format }
    }

    fn input(&self) -> Result<TarInput, ContainerError> {
        let file = File::open(&self.path).map_err(|error| ContainerError::Io(error.to_string()))?;
        Ok(match self.format {
            ContainerFormat::Tar => TarInput::Plain(file),
            ContainerFormat::TarGz => TarInput::Gzip(GzDecoder::new(file)),
            ContainerFormat::TarBz2 => TarInput::Bzip2(BzDecoder::new(file)),
            ContainerFormat::TarXz => TarInput::Xz(XzDecoder::new(file)),
            ContainerFormat::Zip => return Err(ContainerError::InvalidArchive),
        })
    }

    fn archive(&self) -> Result<tar::Archive<TarInput>, ContainerError> {
        Ok(tar::Archive::new(self.input()?))
    }
}

impl ContainerReader for TarReader {
    fn visit_entries(&self, visitor: &mut dyn FnMut(ContainerEntry)) -> Result<(), ContainerError> {
        let mut archive = self.archive()?;
        let entries = archive.entries()
                             .map_err(|error| ContainerError::Io(error.to_string()))?;
        for entry in entries {
            let entry = entry.map_err(|error| ContainerError::Io(error.to_string()))?;
            let path = entry.path()
                            .map_err(|error| ContainerError::Io(error.to_string()))?;
            visitor(ContainerEntry { name:         path.to_string_lossy().into_owned(),
                                     size_bytes:   entry.size(),
                                     is_directory: entry.header().entry_type().is_dir(), });
        }
        Ok(())
    }

    fn read_entry(&self, name: &str) -> Result<Vec<u8>, ContainerError> {
        let mut archive = self.archive()?;
        let entries = archive.entries()
                             .map_err(|error| ContainerError::Io(error.to_string()))?;
        for entry in entries {
            let mut entry = entry.map_err(|error| ContainerError::Io(error.to_string()))?;
            let path = entry.path()
                            .map_err(|error| ContainerError::Io(error.to_string()))?;
            if path == Path::new(name) {
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes)
                     .map_err(|error| ContainerError::Io(error.to_string()))?;
                return Ok(bytes);
            }
        }
        Err(ContainerError::EntryNotFound(name.to_string()))
    }

    fn extract_entries(&self, names: &[&str], destination: &Path) -> Result<(), ContainerError> {
        for name in names {
            let bytes = self.read_entry(name)?;
            write_extracted_entry(destination, name, &bytes)?;
        }
        Ok(())
    }
}
