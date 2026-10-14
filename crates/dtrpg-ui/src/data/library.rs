//! Data model, filtering, sorting, and stub catalog for the Libri library view.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::data::constants::THUMBNAIL_COOLDOWN_SECS;
use crate::data::enums::ItemStatus;
use crate::util::matching::item_recently_updated;

// ── LibraryItem
// ───────────────────────────────────────────────────────────────

/// `serde(default)` helper for [`LibraryItem::is_available`] — serde's `bool`
/// default is `false`, but a missing field here means the cache predates the
/// flag and every item in it was current as of the last successful sync.
fn default_true() -> bool {
    true
}

/// A single item in the RPG catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryItem {
    /// Stable unique identifier (e.g. `"b1"`).
    pub id:                        Arc<str>,
    /// Numeric API identifier used for SDK detail fetches.
    pub numeric_id:                u64,
    /// The `orderProductId` from the API — used for collection membership
    /// matching.
    #[serde(default)]
    pub order_product_id:          u64,
    /// The `productId` from the API — used as a fallback for collection
    /// membership matching when product list items return `productId`
    /// rather than `orderProductId`.
    #[serde(default)]
    pub product_id:                u64,
    pub title:                     Arc<str>,
    pub publisher:                 Arc<str>,
    /// Game line / series name.
    pub line:                      Arc<str>,
    /// Category tag: Core, Supplement, Adventure, Bestiary, Setting …
    pub kind:                      Arc<str>,
    /// File format string, e.g. `"PDF"` or `"PDF + EPUB"`.
    pub format:                    Arc<str>,
    pub pages:                     u32,
    pub size_mb:                   f64,
    pub year:                      u32,
    /// Relative ordering — lower means more recently added.
    pub added_order:               u32,
    pub status:                    ItemStatus,
    /// Hex color string for the generative cover background, e.g. `"#1C2A44"`.
    pub color:                     Arc<str>,
    pub desc:                      Arc<str>,
    /// Optional URL for a small-context cover thumbnail (grid card, thumb
    /// row).
    #[serde(default)]
    pub cover_url:                 Option<Arc<str>>,
    /// Optional URL for a large-context cover image (detail panel), preferring
    /// the full-size/WebP source over the pre-generated thumbnails.
    #[serde(default)]
    pub detail_cover_url:          Option<Arc<str>>,
    /// Unix timestamp (seconds since epoch) when the item was added to the
    /// library.
    #[serde(default)]
    pub date_added:                Option<i64>,
    /// Unix timestamp (seconds since epoch) when the item's files were last
    /// updated by the publisher, if known.
    #[serde(default)]
    pub date_updated:              Option<i64>,
    /// Last time a thumbnail fetch was attempted for this item; not persisted
    /// to cache.
    #[serde(skip)]
    pub thumbnail_last_attempted:  Option<std::time::SystemTime>,
    /// Whether this item was present in the most recent successful live
    /// catalog fetch. `false` means the server no longer lists it, but the
    /// item is kept (not deleted) so previously-downloaded files stay
    /// reachable. Defaults to `true` for cache files written before this
    /// field existed and for items newly added from a live fetch.
    #[serde(default = "default_true")]
    pub is_available:              bool,
    /// Last time an individual server check
    /// (`catalog-item-level-reconciliation`) ran for this item; not
    /// persisted to cache.
    #[serde(skip)]
    pub availability_last_checked: Option<std::time::SystemTime>,
    /// Per-item files bundled in this catalog entry, mapped from the SDK's
    /// `OrderProductFile` array. More than one entry means this is a
    /// multi-item catalog entry (see the `catalog-entry-detail-view`
    /// capability). Defaults to empty for cache entries written before this
    /// field existed.
    #[serde(default)]
    pub files:                     Vec<LibraryItemFile>,
}

impl LibraryItem {
    #[allow(clippy::too_many_arguments)]
    pub fn new(id: &str, title: &str, publisher: &str, line: &str, kind: &str, format: &str,
               pages: u32, size_mb: f64, year: u32, added_order: u32, status: ItemStatus,
               color: &str, desc: &str, date_added: Option<i64>)
               -> Self {
        Self { id: id.into(),
               numeric_id: 0,
               order_product_id: 0,
               product_id: 0,
               title: title.into(),
               publisher: publisher.into(),
               line: line.into(),
               kind: kind.into(),
               format: format.into(),
               pages,
               size_mb,
               year,
               added_order,
               status,
               color: color.into(),
               desc: desc.into(),
               cover_url: None,
               detail_cover_url: None,
               date_added,
               date_updated: None,
               thumbnail_last_attempted: None,
               is_available: true,
               availability_last_checked: None,
               files: Vec::new() }
    }

    /// Returns `true` if this catalog entry bundles more than one
    /// downloadable file (a "multi-item" entry per
    /// `catalog-entry-detail-view`).
    #[must_use]
    pub fn is_multi_item(&self) -> bool {
        self.files.len() > 1
    }

    /// Removes duplicate entries from `files`, keeping the first occurrence
    /// of each unique `(id, name)` pair.
    ///
    /// The DriveThruRPG API has been observed to repeat a download record
    /// (identical `id` *and* `name`) verbatim across `files` for what is
    /// genuinely a single file; the SDK mapping layer (`map_order_product`)
    /// already dedupes those exact repeats on ingest, but catalog data
    /// cached to disk before that fix still has them. Without this, every
    /// row in the detail tab's item list compares equal by `id`, so
    /// selecting one row highlights all of them and further clicks appear to
    /// do nothing.
    ///
    /// `id` alone is deliberately NOT used as the key: the API has also been
    /// observed to reuse the same download id across genuinely distinct
    /// files within a multi-file bundle, so deduplicating on `id` alone
    /// would collapse a real bundle down to one file and hide its
    /// item-count badge (`catalog-entry-detail-view`). Requiring `name` to
    /// match too means two files are only merged when they're truly
    /// identical repeats.
    ///
    /// Call this on any `LibraryItem` loaded from a source this crate does
    /// not fully control (e.g. the on-disk catalog cache).
    pub fn dedupe_files(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.files
            .retain(|f| seen.insert((Arc::clone(&f.id), Arc::clone(&f.name))));
    }

    /// Sets `status` from the aggregate of `files[*].downloaded`: `Downloaded`
    /// only once every file is downloaded, `Cloud` otherwise.
    ///
    /// `status` is never set directly by download completion/removal — callers
    /// update the relevant file's `downloaded` flag first, then call this to
    /// keep the entry-level status in sync. An entry with no files (should not
    /// occur in practice) is treated as `Cloud`.
    pub fn recompute_status(&mut self) {
        self.status = if !self.files.is_empty() && self.files.iter().all(|f| f.downloaded) {
            ItemStatus::Downloaded
        }
        else {
            ItemStatus::Cloud
        };
    }
}

// ── LibraryItemFile
// ───────────────────────────────────────────────────────

/// A single downloadable file within a catalog entry, e.g. the book or the
/// map sheet inside a bundled product like Moria.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryItemFile {
    /// Stable identifier for this file within its entry (the SDK's
    /// `orderProductDownloadId`).
    pub id:         Arc<str>,
    /// This file's position within the entry's file list (the SDK's
    /// `OrderProductFile::index`). Required by the download-preparation API
    /// alongside the entry's `order_product_id` — see
    /// `real-file-download-transfer`.
    pub index:      u32,
    /// Display name of the file (the SDK file's `title`, e.g.
    /// `"Player's Handbook.pdf"`).
    pub name:       Arc<str>,
    /// Uppercase format label derived from the filename extension (e.g.
    /// `"PDF"`).
    pub format:     Arc<str>,
    /// File size in megabytes.
    pub size_mb:    f64,
    /// Whether this specific file has been downloaded to disk.
    ///
    /// The entry's aggregate `LibraryItem::status` is derived from this
    /// per-file flag (see `recompute_entry_status`) rather than being set
    /// independently — an entry is `Downloaded` only once every one of its
    /// files is.
    #[serde(default)]
    pub downloaded: bool,
}

// ── Smart section counts
// ──────────────────────────────────────────────────────

/// Counts for each smart sidebar section given the full catalog.
#[derive(Debug, Clone, Copy, Default)]
pub struct SectionCounts {
    pub all:              usize,
    pub recently_updated: usize,
    pub on_device:        usize,
    pub in_cloud:         usize,
}

/// Computes smart section counts from the full catalog.
///
/// `window_days` is the user-configured "Recently Updated window" applied to
/// the `recently_updated` count.
#[must_use]
pub fn section_counts(catalog: &[LibraryItem], now_secs: i64, window_days: u32) -> SectionCounts {
    SectionCounts { all:              catalog.len(),
                    recently_updated:
                        catalog.iter()
                               .filter(|i| item_recently_updated(i, now_secs, window_days))
                               .count(),
                    on_device:        catalog.iter()
                                             .filter(|i| i.status == ItemStatus::Downloaded)
                                             .count(),
                    in_cloud:         catalog.iter()
                                             .filter(|i| i.status == ItemStatus::Cloud)
                                             .count(), }
}

// ── Thumbnail cooldown
// ────────────────────────────────────────────────────────

/// Returns `true` if no thumbnail fetch has been attempted, or the last attempt
/// was more than 5 minutes ago.
#[must_use]
pub fn thumbnail_cooldown_elapsed(item: &LibraryItem) -> bool {
    let Some(last) = item.thumbnail_last_attempted
    else {
        return true;
    };
    std::time::SystemTime::now().duration_since(last)
                                .is_ok_and(|d| d.as_secs() >= THUMBNAIL_COOLDOWN_SECS)
}

// ── Footer totals
// ─────────────────────────────────────────────────────────────

/// Formats `bytes` as a human-readable size string (GB or MB).
#[must_use]
pub fn format_total_size(items: &[LibraryItem]) -> String {
    let total_mb: f64 = items.iter().map(|i| i.size_mb).sum();
    if total_mb >= 1024.0 {
        format!("{:.1} GB", total_mb / 1024.0)
    }
    else {
        format!("{:.0} MB", total_mb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str, name: &str) -> LibraryItemFile {
        LibraryItemFile { id:         id.into(),
                          index:      0,
                          name:       name.into(),
                          format:     "PDF".into(),
                          size_mb:    1.0,
                          downloaded: false, }
    }

    #[test]
    fn dedupe_files_removes_repeated_ids() {
        let mut item = LibraryItem::new("e1",
                                        "Moria",
                                        "Free League",
                                        "",
                                        "",
                                        "",
                                        0,
                                        0.0,
                                        0,
                                        0,
                                        ItemStatus::Cloud,
                                        "#000000",
                                        "",
                                        None);
        item.files = vec![file("1234", "Moria Rulebook"),
                          file("1234", "Moria Rulebook"),];

        item.dedupe_files();

        assert_eq!(item.files.len(), 1);
        assert!(!item.is_multi_item());
    }

    fn item_with_files(files: Vec<LibraryItemFile>) -> LibraryItem {
        let mut item = LibraryItem::new("e1",
                                        "Moria",
                                        "Free League",
                                        "",
                                        "",
                                        "",
                                        0,
                                        0.0,
                                        0,
                                        0,
                                        ItemStatus::Cloud,
                                        "#000000",
                                        "",
                                        None);
        item.files = files;
        item
    }

    fn downloaded_file(id: &str, name: &str) -> LibraryItemFile {
        let mut f = file(id, name);
        f.downloaded = true;
        f
    }

    #[test]
    fn recompute_status_is_cloud_when_no_files_downloaded() {
        let mut item = item_with_files(vec![file("1", "A"), file("2", "B")]);

        item.recompute_status();

        assert_eq!(item.status, ItemStatus::Cloud);
    }

    #[test]
    fn recompute_status_is_cloud_when_only_some_files_downloaded() {
        let mut item = item_with_files(vec![downloaded_file("1", "A"), file("2", "B")]);

        item.recompute_status();

        assert_eq!(item.status, ItemStatus::Cloud);
    }

    #[test]
    fn recompute_status_is_downloaded_once_every_file_is_downloaded() {
        let mut item = item_with_files(vec![downloaded_file("1", "A"), downloaded_file("2", "B")]);

        item.recompute_status();

        assert_eq!(item.status, ItemStatus::Downloaded);
    }

    #[test]
    fn recompute_status_completing_the_last_file_flips_cloud_to_downloaded() {
        let mut item = item_with_files(vec![downloaded_file("1", "A"), file("2", "B")]);
        item.recompute_status();
        assert_eq!(item.status, ItemStatus::Cloud);

        item.files[1].downloaded = true;
        item.recompute_status();

        assert_eq!(item.status, ItemStatus::Downloaded);
    }

    #[test]
    fn recompute_status_is_cloud_for_an_entry_with_no_files() {
        let mut item = item_with_files(vec![]);

        item.recompute_status();

        assert_eq!(item.status, ItemStatus::Cloud);
    }

    #[test]
    fn is_available_defaults_to_true_when_field_is_missing() {
        let json = serde_json::json!({
            "id": "e1",
            "numeric_id": 0,
            "title": "Moria",
            "publisher": "Free League",
            "line": "",
            "kind": "",
            "format": "",
            "pages": 0,
            "size_mb": 0.0,
            "year": 0,
            "added_order": 0,
            "status": "Cloud",
            "color": "#000000",
            "desc": "",
        });

        let item: LibraryItem = match serde_json::from_value(json) {
            Ok(item) => item,
            Err(e) => panic!("deserializes: {e}"),
        };

        assert!(item.is_available);
    }

    #[test]
    fn dedupe_files_keeps_distinct_ids() {
        let mut item = LibraryItem::new("e1",
                                        "Moria",
                                        "Free League",
                                        "",
                                        "",
                                        "",
                                        0,
                                        0.0,
                                        0,
                                        0,
                                        ItemStatus::Cloud,
                                        "#000000",
                                        "",
                                        None);
        item.files = vec![file("1234", "Moria Rulebook"),
                          file("1235", "Moria Map Sheet"),];

        item.dedupe_files();

        assert_eq!(item.files.len(), 2);
        assert!(item.is_multi_item());
    }

    #[test]
    fn dedupe_files_keeps_distinct_files_that_share_an_id() {
        // Regression: the API has been observed to reuse the same download
        // id across genuinely distinct files within a bundle. Deduping on
        // `id` alone would collapse this real 2-file bundle down to 1 and
        // hide its item-count badge — the `name` must differ, not just the
        // id, before two entries are treated as duplicates.
        let mut item = LibraryItem::new("e1",
                                        "Moria",
                                        "Free League",
                                        "",
                                        "",
                                        "",
                                        0,
                                        0.0,
                                        0,
                                        0,
                                        ItemStatus::Cloud,
                                        "#000000",
                                        "",
                                        None);
        item.files = vec![file("1234", "Moria Rulebook"),
                          file("1234", "Moria Map Sheet"),];

        item.dedupe_files();

        assert_eq!(item.files.len(), 2);
        assert!(item.is_multi_item());
    }

    fn item_with_dates(status: ItemStatus, date_updated: Option<i64>) -> LibraryItem {
        let mut item = LibraryItem::new("e1",
                                        "Moria",
                                        "Free League",
                                        "",
                                        "",
                                        "",
                                        0,
                                        0.0,
                                        0,
                                        0,
                                        status,
                                        "#000000",
                                        "",
                                        None);
        item.date_updated = date_updated;
        item
    }

    const DAY_SECS: i64 = 24 * 60 * 60;

    #[test]
    fn section_counts_recently_updated_reflects_only_items_in_window() {
        let now = 1_800_000_000;
        let catalog = vec![item_with_dates(ItemStatus::Downloaded, Some(now - 10 * DAY_SECS)),
                           item_with_dates(ItemStatus::Cloud, Some(now - 40 * DAY_SECS)),];

        let counts = section_counts(&catalog, now, 30);

        assert_eq!(counts.all, 2);
        assert_eq!(counts.recently_updated, 1);
    }

    #[test]
    fn section_counts_recently_updated_respects_a_non_default_window() {
        let now = 1_800_000_000;
        let catalog = vec![item_with_dates(ItemStatus::Downloaded, Some(now - 10 * DAY_SECS)),
                           item_with_dates(ItemStatus::Cloud, Some(now - 40 * DAY_SECS)),];

        // A 7-day window excludes the item that a 30-day default includes.
        let counts = section_counts(&catalog, now, 7);

        assert_eq!(counts.recently_updated, 0);
    }
}
