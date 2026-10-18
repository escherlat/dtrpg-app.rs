//! Filesystem validation and path derivation for storage roots.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::config::{StorageConfig, StorageError};

static PROBE_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    let probe = path.join(format!(".dtrpg_write_probe-{}-{}",
                                  std::process::id(),
                                  PROBE_COUNTER.fetch_add(1, Ordering::Relaxed),));
    std::fs::OpenOptions::new().write(true)
                               .create_new(true)
                               .open(&probe)
                               .and_then(|file| {
                                   use std::io::Write;
                                   let mut file = file;
                                   file.write_all(b"probe")
                               })
                               .map_err(|_| StorageError::NotWritable(path.to_path_buf()))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

/// Returns free disk space at the current storage root, in megabytes.
#[must_use]
pub fn available_space_mb() -> Option<f64> {
    available_space_mb_at(&StorageConfig::load().root_path())
}

/// Returns free disk space at an explicit storage path, in megabytes.
#[must_use]
pub fn available_space_mb_at(path: &Path) -> Option<f64> {
    fs4::available_space(path).ok()
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
pub fn sanitize_path_component(value: &str) -> String {
    let sanitized = value.trim_start_matches(['/', '\\'])
                         .replace(['/', '\\', ' '], "_");
    if sanitized == "." || sanitized == ".." {
        "_".to_string()
    }
    else {
        sanitized.trim_start_matches(['/', '\\']).to_string()
    }
}
