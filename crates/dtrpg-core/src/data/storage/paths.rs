//! Filesystem validation and path derivation for storage roots.

use std::path::{Path, PathBuf};

use super::config::{StorageConfig, StorageError};

/// Validates `path` for writability by attempting a probe write.
///
/// Creates a temporary file inside `path` and immediately removes it.
/// This catches permission issues, read-only mounts, and full volumes.
pub fn validate_writable(path: &Path) -> Result<(), StorageError> {
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return Err(StorageError::PathDoesNotExist(path.to_path_buf())),
        Err(_) => return Err(StorageError::VolumeUnavailable(path.to_path_buf())),
    }
    let probe = path.join(".dtrpg_write_probe");
    std::fs::write(&probe, b"probe").map_err(|_| StorageError::NotWritable(path.to_path_buf()))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Returns free disk space at the current storage root, in megabytes.
#[must_use]
pub fn available_space_mb() -> Option<f64> {
    fs4::available_space(StorageConfig::load().root_path())
        .ok()
        .map(|bytes| bytes as f64 / 1_048_576.0)
}

/// Derives the directory a downloaded item's files live in under `root`.
pub fn publisher_dir(root: &Path, publisher: &str) -> PathBuf {
    root.join("items").join(sanitize_path_component(publisher))
}

/// Derives the directory a collection's symlinked files live in under `root`.
pub fn collection_dir(root: &Path, collection_name: &str) -> PathBuf {
    root.join("collections")
        .join(sanitize_path_component(collection_name))
}

/// Converts untrusted display text into one filesystem path component.
pub(crate) fn sanitize_path_component(value: &str) -> String {
    value.trim_start_matches('/').replace(['/', ' '], "_")
}
