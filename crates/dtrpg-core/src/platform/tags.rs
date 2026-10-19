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
use crate::data::tag_color::TagColor;

/// The attribute macOS keeps a file's tags in.
#[cfg_attr(target_os = "linux", allow(dead_code))] // Compiled on Linux so its tests run there.
pub(super) const MACOS_ATTRIBUTE: &str = "com.apple.metadata:_kMDItemUserTags";

/// The attribute the freedesktop.org specification keeps a file's tags in.
#[cfg_attr(target_os = "macos", allow(dead_code))] // Compiled on macOS so its tests run there.
pub(super) const LINUX_ATTRIBUTE: &str = "user.xdg.tags";

/// A tag read from a macOS file: its name, and the color suffix it carried.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(target_os = "linux", allow(dead_code))] // Compiled on Linux so its tests run there.
pub(super) struct MacosTag {
    /// The tag's name.
    pub(super) name:   String,
    /// The text after the newline ("6" in "Red\n6"), if there was any.
    pub(super) suffix: Option<String>,
}

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

/// Reads the tags out of the bytes of a macOS tag attribute.
///
/// # Errors
///
/// Returns [`io::ErrorKind::InvalidData`] if the bytes are not a property list
/// holding an array of strings.
#[cfg_attr(target_os = "linux", allow(dead_code))] // Compiled on Linux so its tests run there.
pub(super) fn decode_macos(bytes: &[u8]) -> io::Result<Vec<MacosTag>> {
    let value = Value::from_reader(Cursor::new(bytes)).map_err(invalid_data)?;
    let Value::Array(entries) = value
    else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "the tags are not an array"));
    };
    entries.into_iter()
           .map(|entry| match entry {
               Value::String(text) => Ok(match text.split_once('\n') {
                   Some((name, suffix)) => MacosTag { name:   name.to_owned(),
                                                      suffix: Some(suffix.to_owned()), },
                   None => MacosTag { name:   text,
                                      suffix: None, },
               }),
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

/// The color a macOS tag's suffix names, if it names one.
#[cfg_attr(target_os = "linux", allow(dead_code))] // Compiled on Linux so its tests run there.
fn macos_color(tag: &MacosTag) -> Option<TagColor> {
    tag.suffix
       .as_deref()?
       .trim()
       .parse::<u8>()
       .ok()
       .and_then(TagColor::from_finder_number)
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

/// Reads the tags of the file at `path` from the platform's attribute.
///
/// A file with no tag attribute has no tags.
pub(super) fn read(path: &Path) -> io::Result<Vec<FileTag>> {
    #[cfg(target_os = "macos")]
    {
        match xattr::get(path, MACOS_ATTRIBUTE)? {
            Some(bytes) => Ok(decode_macos(&bytes)?.into_iter()
                                                   .map(|tag| FileTag { color: macos_color(&tag),
                                                                        name:  tag.name, })
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
