use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::ContainerError;

/// Archive formats supported by the container abstraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerFormat {
    Zip,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
}

/// Metadata exposed for one archive entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContainerEntry {
    pub name: String,
    pub size_bytes: u64,
    pub is_directory: bool,
}

pub fn detect_format(path: &Path) -> Result<ContainerFormat, ContainerError> {
    if !path.is_file() {
        return Err(ContainerError::NotFound(path.display().to_string()));
    }
    let extension_hint = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let mut file = File::open(path).map_err(|error| ContainerError::Io(error.to_string()))?;
    let mut header = [0_u8; 512];
    let length = file
        .read(&mut header)
        .map_err(|error| ContainerError::Io(error.to_string()))?;
    let bytes = &header[..length];

    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        return Ok(ContainerFormat::Zip);
    }
    if bytes.starts_with(&[0x1F, 0x8B]) {
        return Ok(ContainerFormat::TarGz);
    }
    if bytes.starts_with(b"BZh") {
        return Ok(ContainerFormat::TarBz2);
    }
    if bytes.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        return Ok(ContainerFormat::TarXz);
    }
    if bytes.len() > 262 && &bytes[257..262] == b"ustar" {
        return Ok(ContainerFormat::Tar);
    }

    if extension_hint.ends_with(".zip") {
        return Ok(ContainerFormat::Zip);
    }
    if extension_hint.ends_with(".tar.gz") || extension_hint.ends_with(".tgz") {
        return Ok(ContainerFormat::TarGz);
    }
    if extension_hint.ends_with(".tar.bz2") || extension_hint.ends_with(".tbz2") {
        return Ok(ContainerFormat::TarBz2);
    }
    if extension_hint.ends_with(".tar.xz") || extension_hint.ends_with(".txz") {
        return Ok(ContainerFormat::TarXz);
    }
    if extension_hint.ends_with(".tar") {
        return Ok(ContainerFormat::Tar);
    }
    Err(ContainerError::InvalidArchive)
}
