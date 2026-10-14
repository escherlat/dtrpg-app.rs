//! UI-independent image format sniffing shared by the data-layer disk cache
//! ([`crate::data::cover_cache`]) and the UI-layer decoded-image cache
//! ([`crate::ui::library::cover::CoverCache`]).
//!
//! Neither this module nor its callers in `data/` depend on `gpui` — the UI
//! layer maps [`ImageKind`] to `gpui::ImageFormat` locally.

/// A sniffed image format, detected from leading magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Jpeg,
    Png,
    Webp,
    Gif,
    Bmp,
}

impl ImageKind {
    /// Returns the file extension (without a leading dot) for this format.
    ///
    /// # Examples
    ///
    /// ```
    /// use dtrpg_ui::util::image_format::ImageKind;
    ///
    /// assert_eq!(ImageKind::Png.extension(), "png");
    /// ```
    #[must_use]
    pub const fn extension(&self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
        }
    }
}

/// Detects an image format from leading magic bytes; defaults to JPEG.
///
/// # Examples
///
/// ```
/// use dtrpg_ui::util::image_format::{ImageKind, sniff};
///
/// let png_bytes = [0x89, b'P', b'N', b'G', 0x0D, 0x0A];
/// assert_eq!(sniff(&png_bytes), ImageKind::Png);
/// ```
#[must_use]
pub fn sniff(bytes: &[u8]) -> ImageKind {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => ImageKind::Png,
        [0xFF, 0xD8, 0xFF, ..] => ImageKind::Jpeg,
        [b'R',
         b'I',
         b'F',
         b'F',
         _,
         _,
         _,
         _,
         b'W',
         b'E',
         b'B',
         b'P',
         ..] => ImageKind::Webp,
        [b'G', b'I', b'F', b'8', ..] => ImageKind::Gif,
        [b'B', b'M', ..] => ImageKind::Bmp,
        _ => ImageKind::Jpeg,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_png() {
        assert_eq!(sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A]), ImageKind::Png);
    }

    #[test]
    fn sniffs_jpeg() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), ImageKind::Jpeg);
    }

    #[test]
    fn sniffs_webp() {
        let bytes = [b'R', b'I', b'F', b'F', 0, 0, 0, 0, b'W', b'E', b'B', b'P'];
        assert_eq!(sniff(&bytes), ImageKind::Webp);
    }

    #[test]
    fn sniffs_gif() {
        assert_eq!(sniff(b"GIF89a"), ImageKind::Gif);
    }

    #[test]
    fn sniffs_bmp() {
        assert_eq!(sniff(&[b'B', b'M', 0, 0]), ImageKind::Bmp);
    }

    #[test]
    fn unknown_bytes_default_to_jpeg() {
        assert_eq!(sniff(&[0, 1, 2, 3]), ImageKind::Jpeg);
    }

    #[test]
    fn extension_matches_kind() {
        assert_eq!(ImageKind::Jpeg.extension(), "jpg");
        assert_eq!(ImageKind::Png.extension(), "png");
        assert_eq!(ImageKind::Webp.extension(), "webp");
        assert_eq!(ImageKind::Gif.extension(), "gif");
        assert_eq!(ImageKind::Bmp.extension(), "bmp");
    }
}
