//! Disk-backed cache for downloaded cover thumbnail image bytes.
//!
//! Without this, every launch re-downloads every catalog item's cover thumbnail
//! from the network, since [`crate::ui::library::cover::CoverCache`] (the
//! in-memory decoded image cache) always starts empty. This module persists the
//! raw fetched bytes to disk so a warm cache is available on the very first
//! thumbnail request of a new session, before any network round trip.
//!
//! Each cached image is stored as a single file under
//! [`crate::data::paths::covers_dir`], named `{hash}.{ext}` where `{hash}` is
//! a stable hash of the owning `LibraryItem` id — the id is a URL path (e.g.
//! `/api/vBeta/order_products/12345`) and unsafe to use directly as a
//! filename — and `{ext}` is the real extension for the sniffed image format
//! (`jpg`, `png`, `webp`, `gif`, `bmp`), via the shared
//! [`crate::util::image_format`] helper. Lookups check each known extension
//! in a fixed order and return the first hit, so no directory scan or
//! out-of-band format record is needed.
//!
//! Unlike [`crate::data::catalog_cache`], writes are not atomic (no
//! `.tmp`-then-rename): a torn write here only means that one cover
//! re-downloads on the next launch, not a corrupted multi-item dataset, so the
//! extra write complexity isn't warranted.

use std::fs;
use std::path::Path;

use crate::util::hash::fnv1a_32;
use crate::util::image_format::sniff;

/// Known cover file extensions, checked in this order on lookup.
const KNOWN_EXTENSIONS: [&str; 5] = ["jpg", "png", "webp", "gif", "bmp"];

/// Returns the on-disk filename for `item_id`'s cached cover with the given
/// extension.
fn cover_filename(item_id: &str, ext: &str) -> String {
    format!("{:08x}.{ext}", fnv1a_32(item_id))
}

/// Reads the cached cover image bytes for `item_id`, checking each known
/// extension in turn and returning the first hit.
///
/// Returns `None` on any error (missing file, unreadable, empty) so callers
/// fall through to a live fetch without surfacing errors to the user.
pub fn load_cached_cover(root: &Path, item_id: &str) -> Option<Vec<u8>> {
    KNOWN_EXTENSIONS.iter().find_map(|ext| {
        let path = root.join(cover_filename(item_id, ext));
        let bytes = fs::read(&path).ok()?;
        if bytes.is_empty() { None } else { Some(bytes) }
    })
}

/// Writes `bytes` to `{root}/{hash}.{ext}`, creating `root` if needed, where
/// `{ext}` is derived by sniffing the actual image format from `bytes`.
///
/// Any previously cached file for the same `item_id` under a *different* known
/// extension is removed first. Without this, a cover whose format changes
/// between fetches (e.g. a `jpg` that is later served as a `png`) would leave
/// the stale `jpg` in place, and [`load_cached_cover`]'s fixed-order scan would
/// keep returning it instead of the freshly written file.
///
/// Failures are not surfaced as an error type — a failed cache write only costs
/// a repeat network fetch next launch, not correctness, so callers log a
/// warning and continue rather than threading a `Result` through the
/// thumbnail-fetch path.
pub fn save_cached_cover(root: &Path, item_id: &str, bytes: &[u8]) {
    if let Err(e) = fs::create_dir_all(root) {
        tracing::warn!(path = %root.display(), error = %e, "cover cache: failed to create dir");
        return;
    }
    let ext = sniff(bytes).extension();

    for known in KNOWN_EXTENSIONS {
        if known == ext {
            continue;
        }
        let stale = root.join(cover_filename(item_id, known));
        if let Err(e) = fs::remove_file(&stale)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(path = %stale.display(), error = %e,
                           "cover cache: failed to remove stale extension");
        }
    }

    let path = root.join(cover_filename(item_id, ext));
    if let Err(e) = fs::write(&path, bytes) {
        tracing::warn!(path = %path.display(), error = %e, "cover cache: failed to write");
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dtrpg_cover_cache_test_{name}"))
    }

    #[test]
    fn load_missing_file_returns_none() {
        let dir = std::path::PathBuf::from("/nonexistent/dtrpg_cover_cache_surely_missing");
        assert!(load_cached_cover(&dir, "/api/vBeta/order_products/1").is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = test_dir("roundtrip");
        let id = "/api/vBeta/order_products/515276";
        let bytes = vec![0xFFu8, 0xD8, 0xFF, 1, 2, 3];

        save_cached_cover(&dir, id, &bytes);
        let loaded = load_cached_cover(&dir, id);
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(loaded, Some(bytes));
    }

    #[test]
    fn different_ids_do_not_collide() {
        let dir = test_dir("distinct_ids");
        save_cached_cover(&dir, "/api/vBeta/order_products/1", &[1, 1, 1]);
        save_cached_cover(&dir, "/api/vBeta/order_products/2", &[2, 2, 2]);

        let a = load_cached_cover(&dir, "/api/vBeta/order_products/1");
        let b = load_cached_cover(&dir, "/api/vBeta/order_products/2");
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(a, Some(vec![1, 1, 1]));
        assert_eq!(b, Some(vec![2, 2, 2]));
    }

    #[test]
    fn empty_file_is_treated_as_missing() {
        let dir = test_dir("empty");
        fs::create_dir_all(&dir).unwrap();
        let id = "/api/vBeta/order_products/9";
        fs::write(dir.join(cover_filename(id, "jpg")), []).unwrap();

        let loaded = load_cached_cover(&dir, id);
        let _ = fs::remove_dir_all(&dir);

        assert!(loaded.is_none());
    }

    /// Table of (magic bytes, expected extension) covering every known
    /// format, so each write goes to the extension matching its content.
    fn format_cases() -> Vec<(&'static str, Vec<u8>, &'static str)> {
        vec![
            ("jpeg", vec![0xFF, 0xD8, 0xFF, 1, 2, 3], "jpg"),
            ("png", vec![0x89, b'P', b'N', b'G', 1, 2], "png"),
            (
                "webp",
                vec![b'R', b'I', b'F', b'F', 0, 0, 0, 0, b'W', b'E', b'B', b'P'],
                "webp",
            ),
            ("gif", vec![b'G', b'I', b'F', b'8', b'9', b'a'], "gif"),
            ("bmp", vec![b'B', b'M', 0, 0], "bmp"),
        ]
    }

    #[test]
    fn save_writes_extension_matching_sniffed_format() {
        for (name, bytes, ext) in format_cases() {
            let dir = test_dir(&format!("ext_{name}"));
            let id = "/api/vBeta/order_products/42";

            save_cached_cover(&dir, id, &bytes);
            let expected_path = dir.join(cover_filename(id, ext));
            let exists = expected_path.is_file();
            let _ = fs::remove_dir_all(&dir);

            assert!(
                exists,
                "expected {name} bytes to be written with .{ext} extension"
            );
        }
    }

    #[test]
    fn save_removes_stale_alternate_extension_on_format_change() {
        let dir = test_dir("format_change");
        let id = "/api/vBeta/order_products/88";

        // First fetch is a JPEG.
        let jpeg = vec![0xFFu8, 0xD8, 0xFF, 1, 2, 3];
        save_cached_cover(&dir, id, &jpeg);
        let jpg_path = dir.join(cover_filename(id, "jpg"));
        assert!(jpg_path.is_file(), "jpeg should be cached as .jpg");

        // The same item is later served as a PNG.
        let png = vec![0x89u8, b'P', b'N', b'G', 1, 2];
        save_cached_cover(&dir, id, &png);

        let jpg_still_there = jpg_path.is_file();
        let loaded = load_cached_cover(&dir, id);
        let _ = fs::remove_dir_all(&dir);

        assert!(
            !jpg_still_there,
            "stale .jpg should be removed when the format changes"
        );
        assert_eq!(loaded, Some(png), "lookup must return the fresh PNG bytes");
    }

    #[test]
    fn load_finds_file_regardless_of_which_known_extension_it_was_written_with() {
        for (name, bytes, ext) in format_cases() {
            let dir = test_dir(&format!("lookup_{name}"));
            let id = "/api/vBeta/order_products/7";

            save_cached_cover(&dir, id, &bytes);
            let loaded = load_cached_cover(&dir, id);
            let _ = fs::remove_dir_all(&dir);

            assert_eq!(
                loaded,
                Some(bytes),
                "lookup should find .{ext} file for {name}"
            );
        }
    }
}
