//! Platform-native, best-effort symlink creation.
//!
//! Used to mirror downloaded files under `{root}/collections/{collection
//! name}/` — see [`dtrpg_core::data::storage::collection_dir`]. Failures are
//! logged by the caller and never propagate into the download's own
//! success/failure outcome.

use std::io;
use std::path::Path;

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

/// Creates a symlink at `link` pointing at `target`, using the OS-native
/// primitive (`std::os::unix::fs::symlink` on macOS/Linux,
/// `std::os::windows::fs::symlink_file` on Windows).
///
/// Skips creation without error if `link` already exists (e.g.
/// re-downloading a previously downloaded item).
///
/// # Errors
///
/// Returns the underlying I/O error if symlink creation fails (permissions,
/// unsupported filesystem, missing OS privilege). Callers should log this
/// and continue rather than fail the calling operation.
pub fn ensure_symlink(target: &Path, link: &Path) -> io::Result<()> {
    if link.symlink_metadata().is_ok() {
        return Ok(());
    }
    create_symlink(target, link)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    #[cfg(unix)]
    use super::*;

    #[cfg(unix)]
    #[test]
    fn ensure_symlink_creates_a_resolvable_link() {
        let dir = std::env::temp_dir().join(format!("dtrpg-test-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.txt");
        std::fs::write(&target, b"hello").unwrap();
        let link = dir.join("link.txt");

        ensure_symlink(&target, &link).unwrap();

        assert_eq!(std::fs::read_to_string(&link).unwrap(), "hello");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ensure_symlink_skips_without_error_when_link_already_exists() {
        let dir =
            std::env::temp_dir().join(format!("dtrpg-test-symlink-exists-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.txt");
        std::fs::write(&target, b"hello").unwrap();
        let link = dir.join("link.txt");
        std::fs::write(&link, b"already here").unwrap();

        assert!(ensure_symlink(&target, &link).is_ok());
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "already here");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
