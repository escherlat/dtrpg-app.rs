//! Platform-native reveal-in-file-manager helper.

use std::io;
use std::path::Path;

use dtrpg_core::platform::{Platform, PlatformIntegration};

/// Opens the OS file manager with `path` selected or revealed.
///
/// On macOS, uses `open -R <path>` to select the item in Finder.
/// On Windows, uses `explorer /select,<path>` to highlight the item in
/// Explorer. On Linux, falls back to `xdg-open` on the parent directory
/// (selecting a specific file via DBus `org.freedesktop.FileManager1.ShowItems`
/// is not yet implemented).
///
/// # Errors
///
/// Returns an error if the OS command fails to launch. A missing file is NOT
/// an error at this layer; callers should check existence beforehand.
pub fn reveal_in_file_manager(path: &Path) -> io::Result<()> {
    Platform.reveal(path)
}
