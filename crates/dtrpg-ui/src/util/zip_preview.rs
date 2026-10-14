//! Reads a Zip archive's central directory (entry name + size) without
//! extracting it, for the detail tab's Zip content preview popover.

use std::fs::File;
use std::path::Path;

use thiserror::Error;

/// A single internal entry of a previewed Zip archive.
#[derive(Debug, Clone, PartialEq)]
pub struct ZipEntry {
    pub name:       String,
    pub size_bytes: u64,
}

/// Errors that can occur when reading a Zip archive's entry list.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ZipPreviewError {
    /// The file was not found at the specified path.
    #[error("File not found: {0}")]
    NotFound(String),

    /// The file could not be read.
    #[error("Failed to read file: {0}")]
    Io(String),

    /// The file is not a valid Zip archive.
    #[error("Not a valid Zip archive")]
    InvalidArchive,
}

/// Reads `path`'s central directory and returns its entries (name and
/// uncompressed size), without extracting any file contents.
pub fn list_entries(path: &Path) -> Result<Vec<ZipEntry>, ZipPreviewError> {
    if !path.is_file() {
        return Err(ZipPreviewError::NotFound(path.display().to_string()));
    }
    let file = File::open(path).map_err(|e| ZipPreviewError::Io(e.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|_| ZipPreviewError::InvalidArchive)?;

    let mut entries = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i)
                           .map_err(|_| ZipPreviewError::InvalidArchive)?;
        entries.push(ZipEntry { name:       entry.name().to_string(),
                                size_bytes: entry.size(), });
    }
    Ok(entries)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::io::Write;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn fixture_zip(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("fixture.zip");
        let file = File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("Player's Handbook.pdf", options).unwrap();
        writer.write_all(b"pdf bytes").unwrap();
        writer.start_file("maps/world.jpg", options).unwrap();
        writer.write_all(b"jpg bytes").unwrap();
        writer.finish().unwrap();
        path
    }

    #[test]
    fn list_entries_returns_expected_entries_for_a_valid_zip() {
        let dir = tempfile::tempdir().unwrap();
        let path = fixture_zip(dir.path());

        let entries = list_entries(&path).unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "Player's Handbook.pdf");
        assert_eq!(entries[0].size_bytes, 9);
        assert_eq!(entries[1].name, "maps/world.jpg");
    }

    #[test]
    fn list_entries_fails_with_not_found_for_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.zip");

        assert_eq!(list_entries(&path),
                   Err(ZipPreviewError::NotFound(path.display().to_string())));
    }

    #[test]
    fn list_entries_fails_with_invalid_archive_for_an_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.zip");
        File::create(&path).unwrap();

        assert_eq!(list_entries(&path), Err(ZipPreviewError::InvalidArchive));
    }

    #[test]
    fn list_entries_fails_with_invalid_archive_for_a_non_zip_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-zip.txt");
        std::fs::write(&path, b"just some text, not a zip archive").unwrap();

        assert_eq!(list_entries(&path), Err(ZipPreviewError::InvalidArchive));
    }
}
