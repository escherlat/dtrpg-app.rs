//! Platform-native integration boundary.
//!
//! The desktop UI and the future MCP service should query this boundary rather
//! than branching on `target_os` themselves. Keeping the capability model here
//! makes unsupported actions explicit before they reach a platform command.

use std::io;
use std::path::Path;

/// Native capabilities available on the current operating system.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities {
    /// Whether the platform can report cloud/file-provider state.
    pub file_state: bool,
    /// Whether a cloud-backed file can be materialized on demand.
    pub materialize: bool,
    /// Whether filesystem tags can be projected from the local index.
    pub filesystem_tags: bool,
    /// Whether the platform can reveal a path in its file manager.
    pub reveal: bool,
}

/// Operating-system family used for platform-specific presentation text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformKind {
    /// Apple macOS and Finder.
    MacOs,
    /// Microsoft Windows and Explorer.
    Windows,
    /// Linux and other Unix-like desktop targets.
    Linux,
}

/// Availability state reported for a local library file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileState {
    /// The path is not present on the local filesystem.
    Missing,
    /// The path is available for immediate use.
    Local,
    /// The path exists as a cloud placeholder and is not yet local.
    CloudOnly,
    /// The platform is currently materializing the path.
    Downloading,
    /// The platform could not determine a more specific state.
    Unknown,
}

/// Platform operations used by application services and presentation layers.
pub trait PlatformIntegration: Send + Sync {
    /// Returns the operating-system family for this implementation.
    fn kind(&self) -> PlatformKind;

    /// Returns the operations supported by this platform implementation.
    fn capabilities(&self) -> Capabilities;

    /// Returns the platform's current availability state for `path`.
    fn file_state(&self, path: &Path) -> io::Result<FileState>;

    /// Reveals `path` in the native file manager.
    fn reveal(&self, path: &Path) -> io::Result<()>;

    /// Requests that a cloud-backed file become available locally.
    fn materialize(&self, _path: &Path) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this platform cannot materialize cloud-backed files",
        ))
    }
}

/// The platform implementation selected at compile time for this process.
#[derive(Debug, Default)]
pub struct Platform;

impl PlatformIntegration for Platform {
    fn kind(&self) -> PlatformKind {
        kind()
    }

    fn capabilities(&self) -> Capabilities {
        capabilities()
    }

    fn reveal(&self, path: &Path) -> io::Result<()> {
        reveal(path)
    }

    fn file_state(&self, path: &Path) -> io::Result<FileState> {
        file_state(path)
    }

    fn materialize(&self, path: &Path) -> io::Result<()> {
        materialize(path)
    }
}

#[cfg(target_os = "macos")]
fn kind() -> PlatformKind {
    PlatformKind::MacOs
}

#[cfg(target_os = "windows")]
fn kind() -> PlatformKind {
    PlatformKind::Windows
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn kind() -> PlatformKind {
    PlatformKind::Linux
}

/// Returns the native capabilities for the current target.
#[cfg(target_os = "macos")]
fn capabilities() -> Capabilities {
    Capabilities {
        file_state: true,
        materialize: true,
        filesystem_tags: true,
        reveal: true,
    }
}

/// Returns the native capabilities for the current target.
#[cfg(target_os = "windows")]
fn capabilities() -> Capabilities {
    Capabilities {
        file_state: true,
        materialize: true,
        filesystem_tags: false,
        reveal: true,
    }
}

/// Returns the native capabilities for Linux and other Unix targets.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capabilities() -> Capabilities {
    Capabilities {
        file_state: false,
        materialize: false,
        filesystem_tags: true,
        reveal: true,
    }
}

#[cfg(target_os = "macos")]
fn reveal(path: &Path) -> io::Result<()> {
    run_command("open", ["-R", &path.to_string_lossy()])
}

#[cfg(target_os = "macos")]
fn file_state(path: &Path) -> io::Result<FileState> {
    if !path.exists() {
        return Ok(FileState::Missing);
    }

    let output = std::process::Command::new("mdls")
        .args(["-raw", "-name", "NSURLUbiquitousItemDownloadingStatusKey"])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Ok(FileState::Unknown);
    }

    match String::from_utf8_lossy(&output.stdout).trim() {
        "NSURLUbiquitousItemDownloadingStatusNotDownloaded" => Ok(FileState::CloudOnly),
        "NSURLUbiquitousItemDownloadingStatusDownloaded"
        | "NSURLUbiquitousItemDownloadingStatusCurrent" => Ok(FileState::Local),
        "(null)" => Ok(FileState::Local),
        _ => Ok(FileState::Unknown),
    }
}

#[cfg(target_os = "windows")]
fn reveal(path: &Path) -> io::Result<()> {
    run_command("explorer", [format!("/select,{}", path.display())])
}

#[cfg(not(target_os = "macos"))]
fn file_state(path: &Path) -> io::Result<FileState> {
    if path.exists() {
        Ok(FileState::Local)
    } else {
        Ok(FileState::Missing)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn reveal(path: &Path) -> io::Result<()> {
    let parent = path.parent().unwrap_or(path);
    run_command("xdg-open", [parent.to_string_lossy().into_owned()])
}

#[cfg(target_os = "macos")]
fn materialize(path: &Path) -> io::Result<()> {
    // `brctl download` asks File Provider/iCloud to make the item local.
    run_command("brctl", ["download", &path.to_string_lossy()])
}

#[cfg(target_os = "windows")]
fn materialize(path: &Path) -> io::Result<()> {
    // Opening a Files On-Demand placeholder triggers Windows recall.
    std::fs::File::open(path).map(|_| ())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn materialize(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this platform cannot materialize cloud-backed files",
    ))
}

fn run_command<I, S>(program: &str, args: I) -> io::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let status = std::process::Command::new(program).args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{program} exited with status {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Capabilities, FileState, Platform, PlatformIntegration};

    #[test]
    fn capabilities_match_the_target_contract() {
        let capabilities = Platform.capabilities();

        #[cfg(target_os = "macos")]
        assert_eq!(
            capabilities,
            Capabilities {
                file_state: true,
                materialize: true,
                filesystem_tags: true,
                reveal: true,
            }
        );

        #[cfg(target_os = "windows")]
        assert_eq!(
            capabilities,
            Capabilities {
                file_state: true,
                materialize: true,
                filesystem_tags: false,
                reveal: true,
            }
        );

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        assert_eq!(
            capabilities,
            Capabilities {
                file_state: false,
                materialize: false,
                filesystem_tags: true,
                reveal: true,
            }
        );
    }

    #[test]
    fn missing_paths_are_reported_without_platform_commands() {
        assert!(matches!(
            Platform.file_state(Path::new("/definitely/not/a/libri-file")),
            Ok(FileState::Missing)
        ));
    }
}
