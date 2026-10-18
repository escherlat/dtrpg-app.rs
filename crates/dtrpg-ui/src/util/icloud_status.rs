//! Cloud-provider file state used by catalog presence verification.

use std::path::Path;

use dtrpg_core::platform::{FileState, Platform, PlatformIntegration};

/// Returns whether `path` is present as a cloud-only/offloaded placeholder.
///
/// The platform implementation returns `false` on targets without a native
/// cloud-file state API. Errors and unknown states are deliberately treated as
/// not offloaded so a transient metadata failure cannot change catalog state.
#[must_use]
pub fn is_offloaded(path: &Path) -> bool {
    matches!(Platform.file_state(path), Ok(FileState::CloudOnly))
}

#[cfg(test)]
mod tests {
    use super::is_offloaded;

    #[test]
    fn plain_local_file_is_not_offloaded() {
        let path = std::env::temp_dir().join("libri-not-offloaded");
        std::fs::write(&path, b"local").ok();
        assert!(!is_offloaded(&path));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn missing_file_is_not_offloaded() {
        assert!(!is_offloaded(std::path::Path::new("/definitely/not/a/libri-file",)));
    }
}
