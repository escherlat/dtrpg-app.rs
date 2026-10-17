//! On-disk file size lookup for the detail view's file size fields.
//!
//! Resolves a file's real byte size from the filesystem at render time, so a
//! `Downloaded` item's detail view can show its actual size instead of the
//! catalog-reported estimate. See `entry_dir` resolution convention in
//! `util::item_opener::ItemOpener::open_item`.

use std::path::Path;

use rust_i18n::t;

/// Returns the on-disk size in bytes of `entry_dir.join(file_name)`, or
/// `None` if the file doesn't exist, `file_name` is empty, or the path can't
/// be read.
pub fn on_disk_file_size(entry_dir: &Path, file_name: &str) -> Option<u64> {
    if file_name.is_empty() {
        return None;
    }
    std::fs::metadata(entry_dir.join(file_name)).ok()
                                                .map(|m| m.len())
}

/// Formats a byte count as a `"{:.1} MB"`-style string, matching the
/// existing catalog-size formatting used elsewhere in the detail view. The
/// unit label is localized via the `size.mb` translation key.
pub fn format_bytes(bytes: u64) -> String {
    format!("{:.1} {}", bytes as f64 / (1024.0 * 1024.0), t!("size.mb"))
}

/// Appends a localized `"(X.X MB on disk)"` suffix to `catalog_size` when
/// `on_disk_bytes` is known, otherwise returns `catalog_size` unchanged.
///
/// The catalog-reported size is always shown; the on-disk size is
/// supplementary detail, not a replacement (see `detail-file-size-on-disk`).
pub fn with_on_disk_suffix(catalog_size: String, on_disk_bytes: Option<u64>) -> String {
    match on_disk_bytes {
        Some(bytes) => format!("{catalog_size} {}",
                               t!("detail.on_disk_suffix", size = format_bytes(bytes))),
        None => catalog_size,
    }
}

/// Returns the on-disk size when known, otherwise `catalog_size` unchanged.
///
/// Unlike [`with_on_disk_suffix`], this replaces rather than supplements —
/// for a single file (not a multi-file aggregate) there's no "partially
/// downloaded" state to show both sizes for: it's either on disk (show the
/// real size) or still only in the cloud (show the catalog estimate).
pub fn prefer_on_disk_size(catalog_size: String, on_disk_bytes: Option<u64>) -> String {
    match on_disk_bytes {
        Some(bytes) => format_bytes(bytes),
        None => catalog_size,
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use super::*;

    #[test]
    fn on_disk_file_size_returns_size_for_existing_file() {
        let dir = std::env::temp_dir().join("dtrpg_on_disk_file_size_present_test");
        let _ = std::fs::create_dir_all(&dir);
        let file_path = dir.join("book.pdf");
        let Ok(mut file) = File::create(&file_path)
        else {
            return;
        };
        let _ = file.write_all(b"0123456789");
        drop(file);

        let result = on_disk_file_size(&dir, "book.pdf");

        let _ = std::fs::remove_file(&file_path);
        let _ = std::fs::remove_dir(&dir);

        assert_eq!(result, Some(10));
    }

    #[test]
    fn on_disk_file_size_returns_none_for_missing_file() {
        let dir = std::env::temp_dir().join("dtrpg_on_disk_file_size_missing_test");
        let result = on_disk_file_size(&dir, "does-not-exist.pdf");
        assert_eq!(result, None);
    }

    #[test]
    fn on_disk_file_size_returns_none_for_empty_file_name() {
        let dir = std::env::temp_dir();
        let result = on_disk_file_size(&dir, "");
        assert_eq!(result, None);
    }

    #[test]
    fn format_bytes_matches_existing_mb_style() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1_572_864), "1.5 MB");
    }

    #[test]
    fn with_on_disk_suffix_appends_suffix_when_bytes_known() {
        let result = with_on_disk_suffix("12.0 MB".to_string(), Some(1_572_864));
        assert_eq!(result, "12.0 MB (1.5 MB on disk)");
    }

    #[test]
    fn with_on_disk_suffix_returns_catalog_size_unchanged_when_unknown() {
        let result = with_on_disk_suffix("12.0 MB".to_string(), None);
        assert_eq!(result, "12.0 MB");
    }

    #[test]
    fn prefer_on_disk_size_returns_local_size_when_known() {
        let result = prefer_on_disk_size("12.0 MB".to_string(), Some(1_572_864));
        assert_eq!(result, "1.5 MB");
    }

    #[test]
    fn prefer_on_disk_size_returns_catalog_size_when_unknown() {
        let result = prefer_on_disk_size("12.0 MB".to_string(), None);
        assert_eq!(result, "12.0 MB");
    }
}
