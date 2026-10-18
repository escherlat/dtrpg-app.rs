//! Per-format file metadata (PDF page count, Zip contained-file count, image
//! pixel dimensions) for the detail tab's file list Info column. Each
//! extractor reads only what it needs (a PDF's page tree, a Zip's central
//! directory, an image's header) rather than decoding full file content.

use std::path::Path;

use crate::util::zip_preview;

/// A file's Info column value. Formats without a defined extraction, and
/// files whose content can't be parsed, are represented by [`Self::None`]
/// rather than propagating an error to the render path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileInfo {
    PageCount(u32),
    ContainedFiles(u32),
    Dimensions(u32, u32),
    None,
}

impl FileInfo {
    /// Renders this value as the Info column's cell text, or `None` for
    /// [`Self::None`] so the caller can apply its own em-dash fallback
    /// (matching `value_or_dash`'s convention elsewhere in the file list).
    #[must_use]
    pub fn display(&self) -> Option<String> {
        match self {
            Self::PageCount(n) => Some(format!("{n} pages")),
            Self::ContainedFiles(n) => Some(format!("{n} files")),
            Self::Dimensions(w, h) => Some(format!("{w}x{h}")),
            Self::None => None,
        }
    }
}

/// Returns `path`'s PDF page count, or `None` if the file can't be opened or
/// parsed as a PDF.
#[must_use]
pub fn extract_pdf_page_count(path: &Path) -> Option<u32> {
    let doc = lopdf::Document::load(path).ok()?;
    u32::try_from(doc.get_pages().len()).ok()
}

/// Returns the number of entries in `path`'s Zip central directory, or
/// `None` if the file can't be opened or parsed as a Zip archive. Reuses
/// the same archive-reading path as the zip-content-preview hover.
#[must_use]
pub fn extract_zip_file_count(path: &Path) -> Option<u32> {
    let entries = zip_preview::list_entries(path).ok()?;
    u32::try_from(entries.len()).ok()
}

/// Returns `path`'s pixel dimensions, or `None` if the file can't be opened
/// or its header can't be parsed as a supported image format. Reads only
/// the image header, not the full pixel content.
#[must_use]
pub fn extract_image_dimensions(path: &Path) -> Option<(u32, u32)> {
    image::image_dimensions(path).ok()
}

/// Dispatches to the extractor matching `format` (the uppercase format
/// label from `LibraryItemFile::format`), returning [`FileInfo::None`] for
/// unrecognized formats or any extraction failure.
#[must_use]
pub fn compute_file_info(path: &Path, format: &str) -> FileInfo {
    match format.to_ascii_uppercase().as_str() {
        "PDF" => extract_pdf_page_count(path).map_or(FileInfo::None, FileInfo::PageCount),
        "ZIP" => extract_zip_file_count(path).map_or(FileInfo::None, FileInfo::ContainedFiles),
        "JPG" | "JPEG" | "PNG" | "GIF" | "WEBP" | "BMP" => {
            extract_image_dimensions(path).map_or(FileInfo::None, |(w, h)| {
                                              FileInfo::Dimensions(w, h)
                                          })
        }
        _ => FileInfo::None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    #[test]
    fn extract_pdf_page_count_returns_none_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.pdf");

        assert_eq!(extract_pdf_page_count(&path), None);
    }

    #[test]
    fn extract_pdf_page_count_returns_none_for_truncated_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("truncated.pdf");
        std::fs::write(&path, b"%PDF-1.4\nnot a real pdf body").unwrap();

        assert_eq!(extract_pdf_page_count(&path), None);
    }

    #[test]
    fn extract_zip_file_count_returns_count_for_valid_archive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.zip");
        let file = File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("a.txt", options).unwrap();
        writer.write_all(b"a").unwrap();
        writer.start_file("b.txt", options).unwrap();
        writer.write_all(b"b").unwrap();
        writer.finish().unwrap();

        assert_eq!(extract_zip_file_count(&path), Some(2));
    }

    #[test]
    fn extract_zip_file_count_returns_none_for_corrupt_archive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.zip");
        std::fs::write(&path, b"not a zip archive").unwrap();

        assert_eq!(extract_zip_file_count(&path), None);
    }

    #[test]
    fn extract_image_dimensions_returns_dimensions_for_valid_png() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.png");
        let img = image::RgbImage::new(4, 3);
        img.save(&path).unwrap();

        assert_eq!(extract_image_dimensions(&path), Some((4, 3)));
    }

    #[test]
    fn extract_image_dimensions_returns_none_for_corrupt_image() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.png");
        std::fs::write(&path, b"not an image").unwrap();

        assert_eq!(extract_image_dimensions(&path), None);
    }

    #[test]
    fn compute_file_info_dispatches_by_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.png");
        let img = image::RgbImage::new(2, 2);
        img.save(&path).unwrap();

        assert_eq!(compute_file_info(&path, "PNG"), FileInfo::Dimensions(2, 2));
    }

    #[test]
    fn compute_file_info_returns_none_for_unsupported_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.txt");
        std::fs::write(&path, b"text").unwrap();

        assert_eq!(compute_file_info(&path, "TXT"), FileInfo::None);
    }

    #[test]
    fn compute_file_info_returns_none_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.pdf");

        assert_eq!(compute_file_info(&path, "PDF"), FileInfo::None);
    }
}
