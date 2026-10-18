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
    pub file_state:      bool,
    /// Whether a cloud-backed file can be materialized on demand.
    pub materialize:     bool,
    /// Whether filesystem tags can be projected from the local index.
    pub filesystem_tags: bool,
    /// Whether the platform can reveal a path in its file manager.
    pub reveal:          bool,
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

/// `FILE_ATTRIBUTE_OFFLINE`: the file's data is not available locally.
const WINDOWS_OFFLINE: u32 = 0x0000_1000;
/// `FILE_ATTRIBUTE_RECALL_ON_OPEN`: opening the file recalls its data.
const WINDOWS_RECALL_ON_OPEN: u32 = 0x0004_0000;
/// `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`: reading the file recalls its data.
const WINDOWS_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
/// `SF_DATALESS`: the file's data is held by a file provider, not on disk.
const MACOS_DATALESS: u32 = 0x4000_0000;

impl FileState {
    /// Reads the state of an existing Windows file from its attribute bits.
    ///
    /// Files On-Demand placeholders carry one of the recall or offline
    /// attributes. Windows does not report an in-progress download through
    /// the attributes, so this never returns [`FileState::Downloading`].
    ///
    /// # Examples
    ///
    /// ```
    /// use dtrpg_core::platform::FileState;
    ///
    /// assert_eq!(FileState::from_windows_attributes(0x20), FileState::Local);
    /// assert_eq!(FileState::from_windows_attributes(0x0040_0020),
    ///            FileState::CloudOnly);
    /// ```
    #[must_use]
    pub const fn from_windows_attributes(attributes: u32) -> Self {
        let placeholder = WINDOWS_OFFLINE | WINDOWS_RECALL_ON_OPEN | WINDOWS_RECALL_ON_DATA_ACCESS;
        if attributes & placeholder != 0 {
            Self::CloudOnly
        }
        else {
            Self::Local
        }
    }

    /// Reads the state of an existing macOS file from its `st_flags`.
    ///
    /// A "dataless" file is the placeholder a file provider (iCloud Drive)
    /// leaves when it offloads a file.
    ///
    /// # Examples
    ///
    /// ```
    /// use dtrpg_core::platform::FileState;
    ///
    /// assert_eq!(FileState::from_macos_flags(0), FileState::Local);
    /// assert_eq!(FileState::from_macos_flags(0x4000_0000),
    ///            FileState::CloudOnly);
    /// ```
    #[must_use]
    pub const fn from_macos_flags(flags: u32) -> Self {
        if flags & MACOS_DATALESS != 0 {
            Self::CloudOnly
        }
        else {
            Self::Local
        }
    }
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
        Err(io::Error::new(io::ErrorKind::Unsupported,
                           "this platform cannot materialize cloud-backed files"))
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
    Capabilities { file_state:      true,
                   materialize:     true,
                   filesystem_tags: true,
                   reveal:          true, }
}

/// Returns the native capabilities for the current target.
#[cfg(target_os = "windows")]
fn capabilities() -> Capabilities {
    Capabilities { file_state:      true,
                   materialize:     true,
                   filesystem_tags: false,
                   reveal:          true, }
}

/// Returns the native capabilities for Linux and other Unix targets.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capabilities() -> Capabilities {
    Capabilities { file_state:      false,
                   materialize:     false,
                   filesystem_tags: true,
                   reveal:          true, }
}

#[cfg(target_os = "macos")]
fn reveal(path: &Path) -> io::Result<()> {
    run_command("open", ["-R", &path.to_string_lossy()])
}

#[cfg(target_os = "macos")]
fn file_state(path: &Path) -> io::Result<FileState> {
    use std::os::macos::fs::MetadataExt;

    match std::fs::metadata(path) {
        Ok(metadata) => Ok(FileState::from_macos_flags(metadata.st_flags())),
        // Before macOS 14 an offloaded file was replaced by a hidden stub.
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if icloud_stub_path(path).is_some_and(|stub| stub.exists()) {
                Ok(FileState::CloudOnly)
            }
            else {
                Ok(FileState::Missing)
            }
        }
        Err(error) => Err(error),
    }
}

/// Returns the hidden `.name.icloud` stub that stands in for `path` when
/// iCloud Drive has offloaded it, or `None` for a path with no file name.
#[cfg(any(target_os = "macos", test))]
fn icloud_stub_path(path: &Path) -> Option<std::path::PathBuf> {
    let name = path.file_name()?.to_string_lossy();
    Some(path.with_file_name(format!(".{name}.icloud")))
}

#[cfg(target_os = "windows")]
fn file_state(path: &Path) -> io::Result<FileState> {
    use std::os::windows::fs::MetadataExt;

    match std::fs::metadata(path) {
        Ok(metadata) => Ok(FileState::from_windows_attributes(metadata.file_attributes())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(FileState::Missing),
        Err(error) => Err(error),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn file_state(path: &Path) -> io::Result<FileState> {
    if path.exists() {
        Ok(FileState::Local)
    }
    else {
        Ok(FileState::Missing)
    }
}

#[cfg(target_os = "windows")]
fn reveal(path: &Path) -> io::Result<()> {
    // `explorer` exits with status 1 even when it opens the window, so only a
    // failure to start it is an error.
    std::process::Command::new("explorer").arg(format!("/select,{}", path.display()))
                                          .status()
                                          .map(|_| ())
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
    Err(io::Error::new(io::ErrorKind::Unsupported,
                       "this platform cannot materialize cloud-backed files"))
}

fn run_command<I, S>(program: &str, args: I) -> io::Result<()>
    where I: IntoIterator<Item = S>,
          S: AsRef<std::ffi::OsStr> {
    let status = std::process::Command::new(program).args(args).status()?;
    if status.success() {
        Ok(())
    }
    else {
        Err(io::Error::other(format!("{program} exited with status {status}")))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Capabilities, FileState, Platform, PlatformIntegration, icloud_stub_path};

    #[test]
    fn capabilities_match_the_target_contract() {
        let capabilities = Platform.capabilities();

        #[cfg(target_os = "macos")]
        assert_eq!(capabilities,
                   Capabilities { file_state:      true,
                                  materialize:     true,
                                  filesystem_tags: true,
                                  reveal:          true, });

        #[cfg(target_os = "windows")]
        assert_eq!(capabilities,
                   Capabilities { file_state:      true,
                                  materialize:     true,
                                  filesystem_tags: false,
                                  reveal:          true, });

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        assert_eq!(capabilities,
                   Capabilities { file_state:      false,
                                  materialize:     false,
                                  filesystem_tags: true,
                                  reveal:          true, });
    }

    #[test]
    fn missing_paths_are_reported_without_platform_commands() {
        assert!(matches!(Platform.file_state(Path::new("/definitely/not/a/libri-file")),
                         Ok(FileState::Missing)));
    }

    #[test]
    fn windows_placeholder_attributes_are_cloud_only() {
        const ARCHIVE: u32 = 0x20;
        for placeholder in [0x1000, 0x4_0000, 0x40_0000] {
            assert_eq!(FileState::from_windows_attributes(ARCHIVE | placeholder),
                       FileState::CloudOnly);
        }
        assert_eq!(FileState::from_windows_attributes(ARCHIVE),
                   FileState::Local);
    }

    #[test]
    fn macos_dataless_flag_is_cloud_only() {
        assert_eq!(FileState::from_macos_flags(0x4000_0000),
                   FileState::CloudOnly);
        assert_eq!(FileState::from_macos_flags(0x0000_8000), FileState::Local);
    }

    #[test]
    fn icloud_stub_is_a_hidden_sibling() {
        assert_eq!(icloud_stub_path(Path::new("/lib/Book.pdf")),
                   Some(Path::new("/lib/.Book.pdf.icloud").to_path_buf()));
        assert_eq!(icloud_stub_path(Path::new("/")), None);
    }

    #[test]
    fn present_files_are_local() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("book.pdf");
        std::fs::write(&file, b"data")?;

        assert!(matches!(Platform.file_state(&file), Ok(FileState::Local)));
        Ok(())
    }
}
