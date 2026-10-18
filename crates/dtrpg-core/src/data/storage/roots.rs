//! Named storage roots: each download location the user has configured,
//! with on-demand capacity queries (used/total megabytes).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One configured download location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageRoot {
    /// Stable identifier, referenced by [`super::placement::PlacementRule`]
    /// and by `StorageConfig::default_root_id`. Not reused after a root is
    /// removed, so a stale rule pointing at a removed root's id resolves to
    /// "no match" rather than silently matching whatever root is later
    /// assigned the same id.
    pub id:   u32,
    /// Display name shown in the Settings roots list, e.g. "Internal SSD"
    /// or "External Archive".
    pub name: String,
    /// The root directory itself.
    pub path: PathBuf,
}

/// Disk capacity for a [`StorageRoot`], in megabytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RootCapacity {
    /// Space already used by this root's own contents (`items/` and
    /// `collections/` combined), if it could be measured.
    pub used_mb:      Option<f64>,
    /// Free space remaining on the volume the root lives on, if it could
    /// be measured (e.g. an unmounted volume reports `None`).
    pub available_mb: Option<f64>,
}

/// Operational state of a configured storage root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootHealth {
    /// The root exists as a directory and its volume reports free space.
    Ready,
    /// The configured path does not exist.
    Missing,
    /// The path exists but is not a directory.
    NotDirectory,
    /// The path or its volume could not be queried.
    Unavailable,
}

/// Returns the total size, in megabytes, of every regular file under
/// `path`, recursing into subdirectories.
///
/// Returns `None` if `path` does not exist or cannot be read at all;
/// a partially-readable tree (e.g. one unreadable subdirectory) still
/// returns the sum of what could be read rather than `None`, since a
/// capacity estimate that's slightly low is more useful than no estimate.
#[must_use]
pub fn used_space_mb(path: &std::path::Path) -> Option<f64> {
    if !path.exists() {
        return None;
    }
    let mut total_bytes: u64 = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir)
        else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata()
            else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(entry.path());
            }
            else {
                total_bytes += metadata.len();
            }
        }
    }
    Some(total_bytes as f64 / 1_048_576.0)
}

/// Returns `root`'s capacity: used space under its own directory tree, and
/// free space on the volume it lives on.
#[must_use]
pub fn root_capacity(root: &StorageRoot) -> RootCapacity {
    RootCapacity { used_mb:      used_space_mb(&root.path),
                   available_mb:
                       fs4::available_space(&root.path).ok()
                                                       .map(|bytes| bytes as f64 / 1_048_576.0), }
}

/// Checks whether a root is usable without creating files or changing it.
#[must_use]
pub fn root_health(root: &StorageRoot) -> RootHealth {
    match root.path.try_exists() {
        Ok(false) => RootHealth::Missing,
        Err(_) => RootHealth::Unavailable,
        Ok(true) => match std::fs::metadata(&root.path) {
            Ok(metadata) if !metadata.is_dir() => RootHealth::NotDirectory,
            Ok(_) if fs4::available_space(&root.path).is_ok() => RootHealth::Ready,
            Ok(_) => RootHealth::Unavailable,
            Err(_) => RootHealth::Unavailable,
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dtrpg_storage_roots_test_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn used_space_mb_is_none_for_a_missing_directory() {
        let dir = test_dir("missing");
        assert!(used_space_mb(&dir).is_none());
    }

    #[test]
    fn used_space_mb_sums_files_recursively() {
        let dir = test_dir("sums_recursively");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), vec![0u8; 1_048_576]).unwrap(); // 1 MB
        std::fs::write(dir.join("sub/b.txt"), vec![0u8; 2_097_152]).unwrap(); // 2 MB

        let mb = used_space_mb(&dir).unwrap();

        std::fs::remove_dir_all(&dir).unwrap();
        assert!((mb - 3.0).abs() < 0.01, "expected ~3 MB, got {mb}");
    }

    #[test]
    fn root_capacity_reports_used_and_available() {
        let dir = test_dir("capacity");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), vec![0u8; 1_048_576]).unwrap();
        let root = StorageRoot { id:   1,
                                 name: "Test".to_string(),
                                 path: dir.clone(), };

        let capacity = root_capacity(&root);

        std::fs::remove_dir_all(&dir).unwrap();
        assert!((capacity.used_mb.unwrap() - 1.0).abs() < 0.01);
        assert!(capacity.available_mb.is_some());
    }

    #[test]
    fn root_health_reports_missing_and_ready_roots() {
        let missing = StorageRoot { id:   1,
                                    name: "Missing".to_string(),
                                    path: test_dir("health-missing"), };
        assert_eq!(root_health(&missing), RootHealth::Missing);

        let ready_path = test_dir("health-ready");
        std::fs::create_dir_all(&ready_path).unwrap();
        let ready = StorageRoot { id:   2,
                                  name: "Ready".to_string(),
                                  path: ready_path.clone(), };
        assert_eq!(root_health(&ready), RootHealth::Ready);
        std::fs::remove_dir_all(ready_path).unwrap();
    }
}
