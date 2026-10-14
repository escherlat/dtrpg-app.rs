//! Platform-native reveal-in-file-manager helper.

use std::io;
use std::path::Path;

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
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").args(["-R", &path.to_string_lossy()])
                                          .status()
                                          .map(|_| ())
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(format!("/select,{}", path.display()))
                                              .status()
                                              .map(|_| ())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let parent = path.parent().unwrap_or(path);
        std::process::Command::new("xdg-open").arg(parent)
                                              .status()
                                              .map(|_| ())
    }
}
