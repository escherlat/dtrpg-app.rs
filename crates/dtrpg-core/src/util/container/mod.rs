//! Format-independent access to archive contents.

mod error;
mod format;
mod tar;
mod zip;

pub use error::ContainerError;
pub use format::{ContainerEntry, ContainerFormat, detect_format};

use std::path::{Path, PathBuf};

/// A container reader that does not require callers to know the archive format.
pub trait ContainerReader {
    /// Visits entries in archive order without materializing the whole listing.
    fn visit_entries(&self, visitor: &mut dyn FnMut(ContainerEntry)) -> Result<(), ContainerError>;

    /// Reads one entry into memory.
    fn read_entry(&self, name: &str) -> Result<Vec<u8>, ContainerError>;

    /// Extracts selected entries below `destination`.
    fn extract_entries(&self, names: &[&str], destination: &Path) -> Result<(), ContainerError>;
}

/// Opens a supported archive after extension and magic-byte detection.
pub fn open(path: &Path) -> Result<Box<dyn ContainerReader>, ContainerError> {
    let format = detect_format(path)?;
    match format {
        ContainerFormat::Zip => Ok(Box::new(zip::ZipReader::new(path.to_path_buf()))),
        ContainerFormat::Tar
        | ContainerFormat::TarGz
        | ContainerFormat::TarBz2
        | ContainerFormat::TarXz => Ok(Box::new(tar::TarReader::new(path.to_path_buf(), format))),
    }
}

/// Collects a listing for callers that need a snapshot, such as a UI view.
pub fn list_entries(path: &Path) -> Result<Vec<ContainerEntry>, ContainerError> {
    let reader = open(path)?;
    let mut entries = Vec::new();
    reader.visit_entries(&mut |entry| entries.push(entry))?;
    Ok(entries)
}

fn safe_entry_path(destination: &Path, name: &str) -> Result<PathBuf, ContainerError> {
    let relative = Path::new(name);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(ContainerError::UnsafeEntry(name.to_string()));
    }
    Ok(destination.join(relative))
}

pub(crate) fn write_extracted_entry(
    destination: &Path, name: &str, bytes: &[u8],
) -> Result<(), ContainerError> {
    let path = safe_entry_path(destination, name)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| ContainerError::Io(error.to_string()))?;
    }
    std::fs::write(path, bytes).map_err(|error| ContainerError::Io(error.to_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use ::tar::Builder;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::fs::File;

    use super::*;

    fn tar_fixture(path: &Path) {
        let file = File::create(path).unwrap();
        let mut builder = Builder::new(file);
        let mut header = ::tar::Header::new_gnu();
        header.set_path("docs/readme.txt").unwrap();
        header.set_size(5);
        header.set_cksum();
        builder.append(&header, &b"hello"[..]).unwrap();
        builder.finish().unwrap();
    }

    fn gzip_tar_fixture(path: &Path) {
        let file = File::create(path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        let mut header = ::tar::Header::new_gnu();
        header.set_path("docs/readme.txt").unwrap();
        header.set_size(5);
        header.set_cksum();
        builder.append(&header, &b"hello"[..]).unwrap();
        let encoder = builder.into_inner().unwrap();
        encoder.finish().unwrap();
    }

    #[test]
    fn detects_and_reads_plain_tar() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.tar");
        tar_fixture(&path);

        assert_eq!(detect_format(&path), Ok(ContainerFormat::Tar));
        let reader = open(&path).unwrap();
        assert_eq!(reader.read_entry("docs/readme.txt").unwrap(), b"hello");
    }

    #[test]
    fn detects_and_lists_gzip_tar_lazily() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.tar.gz");
        gzip_tar_fixture(&path);
        let reader = open(&path).unwrap();
        let mut names = Vec::new();

        reader
            .visit_entries(&mut |entry| names.push(entry.name))
            .unwrap();

        assert_eq!(names, ["docs/readme.txt"]);
    }

    #[test]
    fn extraction_rejects_parent_directory_entries() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("out");

        assert_eq!(
            write_extracted_entry(&destination, "../escape.txt", b"nope"),
            Err(ContainerError::UnsafeEntry("../escape.txt".to_string()))
        );
    }

    #[test]
    fn missing_archive_returns_not_found() {
        let path = Path::new("missing.tar");

        assert_eq!(
            detect_format(path),
            Err(ContainerError::NotFound(path.display().to_string()))
        );
    }
}
