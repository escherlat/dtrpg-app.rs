//! Extracts plain text from a catalog file, for summary generation
//! ([`crate::data::summary`]). Lives in `dtrpg-core`, not `dtrpg-ui`, so
//! `LibraryApi` and the MCP server can generate summaries headlessly.

use std::path::Path;

/// Maximum number of PDF pages read for summary generation.
///
/// Representative text for "what is this book about" doesn't need the
/// whole book; capping keeps extraction fast and memory-bounded for very
/// large PDFs (art books, bestiaries running 500+ pages). 40 pages is
/// comfortably past the front matter (cover, credits, table of contents,
/// introduction) on a typical RPG book, so the extracted text is
/// representative narrative/rules content, not just boilerplate.
pub const MAX_SUMMARY_PDF_PAGES: usize = 40;

/// Extracts plain text from `path` for summary generation, given its
/// catalog format label (e.g. `"PDF"`, matching
/// [`crate::data::library::LibraryItemFile::format`]).
///
/// Returns `None` for an unsupported format, a missing/unreadable file, or
/// an extraction that yields no usable text -- callers should treat `None`
/// identically to "cannot summarize this file" rather than an error.
#[must_use]
pub fn extract_text_for_summary(path: &Path, format: &str) -> Option<String> {
    match format {
        "PDF" => extract_pdf_text(path),
        _ => None,
    }
}

fn extract_pdf_text(path: &Path) -> Option<String> {
    let doc = lopdf::Document::load(path).ok()?;
    let mut pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    pages.sort_unstable();
    pages.truncate(MAX_SUMMARY_PDF_PAGES);
    let text = doc.extract_text(&pages).ok()?;
    if text.trim().is_empty() { None } else { Some(text) }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_format_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        std::fs::write(&path, b"not a pdf").unwrap();
        assert!(extract_text_for_summary(&path, "EPUB").is_none());
    }

    #[test]
    fn missing_pdf_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.pdf");
        assert!(extract_text_for_summary(&path, "PDF").is_none());
    }

    #[test]
    fn truncated_pdf_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("truncated.pdf");
        std::fs::write(&path, b"%PDF-1.4\nnot a real pdf body").unwrap();
        assert!(extract_text_for_summary(&path, "PDF").is_none());
    }
}
