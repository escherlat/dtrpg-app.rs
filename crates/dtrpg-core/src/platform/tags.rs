//! A file's tags, stored as extended attributes.
//!
//! macOS keeps a file's tags in the `com.apple.metadata:_kMDItemUserTags`
//! attribute, as a binary property list holding an array of strings. Each
//! string is a tag name, optionally followed by a newline and the number of the
//! tag's color ("Red\n6"). Linux keeps them in `user.xdg.tags`, as a
//! comma-separated list of names.
//!
//! The encoding and decoding are plain functions over bytes, so they run and
//! are tested on both platforms; only the choice of attribute differs.

use std::io::{self, Cursor};
use std::path::Path;

use plist::Value;

use super::FileTag;

/// The attribute macOS keeps a file's tags in.
#[cfg_attr(target_os = "linux", allow(dead_code))] // Compiled on Linux so its tests run there.
pub(super) const MACOS_ATTRIBUTE: &str = "com.apple.metadata:_kMDItemUserTags";

/// The attribute the freedesktop.org specification keeps a file's tags in.
#[cfg_attr(target_os = "macos", allow(dead_code))] // Compiled on macOS so its tests run there.
pub(super) const LINUX_ATTRIBUTE: &str = "user.xdg.tags";

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

/// Reads the tag names out of the bytes of a macOS tag attribute.
///
/// An entry is a name, optionally followed by a newline and a color number
/// ("Red\n6"). The number is dropped. It is the color the tag had when it was
/// put on the file, not the color Finder shows now: Finder keeps a color for
/// each tag name and shows that, and does not rewrite the files when the color
/// changes. A tag recolored in Finder after it was applied would be read with
/// its old color.
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidData`] if the bytes are not a property list
/// holding an array of strings.
#[cfg_attr(target_os = "linux", allow(dead_code))] // Compiled on Linux so its tests run there.
pub(super) fn decode_macos(bytes: &[u8]) -> io::Result<Vec<String>> {
    let value = Value::from_reader(Cursor::new(bytes)).map_err(invalid_data)?;
    let Value::Array(entries) = value
    else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "the tags are not an array"));
    };
    entries.into_iter()
           .map(|entry| match entry {
               Value::String(text) => Ok(text.split_once('\n')
                                             .map_or(text.clone(), |(name, _)| name.to_owned())),
               _ => Err(io::Error::new(io::ErrorKind::InvalidData, "a tag is not a string")),
           })
           .collect()
}

/// Writes `tags` as the bytes of a macOS tag attribute.
///
/// A tag with a color is written with that color's Finder number after its
/// name, and a tag with none is written with its name alone.
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidData`] if the property list cannot be
/// written.
#[cfg_attr(target_os = "linux", allow(dead_code))] // Compiled on Linux so its tests run there.
pub(super) fn encode_macos(tags: &[FileTag]) -> io::Result<Vec<u8>> {
    let entries =
        tags.iter()
            .map(|tag| {
                Value::String(match tag.color {
                                  Some(color) => format!("{}\n{}", tag.name, color.finder_number()),
                                  None => tag.name.clone(),
                              })
            })
            .collect();
    let mut bytes = Vec::new();
    Value::Array(entries).to_writer_binary(&mut bytes)
                         .map_err(invalid_data)?;
    Ok(bytes)
}

/// Reads the tags out of the bytes of a Linux tag attribute.
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidData`] if the bytes are not UTF-8.
#[cfg_attr(target_os = "macos", allow(dead_code))] // Compiled on macOS so its tests run there.
pub(super) fn decode_linux(bytes: &[u8]) -> io::Result<Vec<FileTag>> {
    let text = std::str::from_utf8(bytes).map_err(invalid_data)?;
    Ok(text.split(',')
           .map(str::trim)
           .filter(|name| !name.is_empty())
           .map(FileTag::plain)
           .collect())
}

/// Writes `tags` as the bytes of a Linux tag attribute.
#[cfg_attr(target_os = "macos", allow(dead_code))] // Compiled on macOS so its tests run there.
pub(super) fn encode_linux(tags: &[FileTag]) -> Vec<u8> {
    tags.iter()
        .map(|tag| tag.name.as_str())
        .collect::<Vec<_>>()
        .join(",")
        .into_bytes()
}

/// Reads the tags of the file at `path` from the platform's attribute, with no
/// colors: see [`decode_macos`] for why a file's color is not read.
///
/// A file with no tag attribute has no tags.
pub(super) fn read(path: &Path) -> io::Result<Vec<FileTag>> {
    #[cfg(target_os = "macos")]
    {
        match xattr::get(path, MACOS_ATTRIBUTE)? {
            Some(bytes) => Ok(decode_macos(&bytes)?.into_iter()
                                                   .map(FileTag::plain)
                                                   .collect()),
            None => Ok(Vec::new()),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        match xattr::get(path, LINUX_ATTRIBUTE)? {
            Some(bytes) => decode_linux(&bytes),
            None => Ok(Vec::new()),
        }
    }
}

/// Makes the tags of the file at `path` exactly `tags`, in the platform's
/// attribute. An empty list removes the attribute.
pub(super) fn write(path: &Path, tags: &[FileTag]) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let (attribute, bytes) = (MACOS_ATTRIBUTE, encode_macos(tags)?);
    #[cfg(not(target_os = "macos"))]
    let (attribute, bytes) = (LINUX_ATTRIBUTE, encode_linux(tags));

    if tags.is_empty() {
        xattr::remove(path, attribute).or_else(|error| {
                                          // Nothing to remove is the goal
                                          // reached.
                                          if xattr::get(path, attribute)?.is_none() {
                                              Ok(())
                                          }
                                          else {
                                              Err(error)
                                          }
                                      })
    }
    else {
        xattr::set(path, attribute, &bytes)
    }
}

/// When the file's metadata, which an attribute write changes, last changed,
/// in Unix seconds.
pub(super) fn changed_at(path: &Path) -> io::Result<i64> {
    use std::os::unix::fs::MetadataExt;

    Ok(std::fs::metadata(path)?.ctime())
}

#[cfg(test)]
mod tests;
