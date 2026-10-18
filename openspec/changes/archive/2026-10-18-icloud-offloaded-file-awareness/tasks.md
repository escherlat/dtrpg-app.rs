## 1. iCloud Detection Module

Reconciled on 2026-10-18 with the platform integration layer (stage 4 of `libri-v2-architecture`).
Detection lives in `dtrpg_core::platform` (`FileState`), not in a macOS-only module. `icloud_status::is_offloaded`
in `dtrpg-ui` is a thin wrapper over it, so tasks 1.1 to 1.3 are met by that layer: it reads the `SF_DATALESS`
file flag instead of `NSURLUbiquitousItemDownloadingStatusKey`, and Windows placeholders count as well.
Task 5.6 was done by hand on a real Mac on 2026-10-18: the offloaded glyph and tooltip appeared after a
verification pass, opening the item downloaded it first, and the glyph cleared afterwards.

- [x] 1.1 Create `crates/dtrpg-ui/src/util/icloud_status.rs` with
      `pub fn is_offloaded(path: &Path) -> bool`, `#[cfg(target_os =
      "macos")]` implementation using `objc2-foundation`'s file URL
      resource-values API to read
      `NSURLUbiquitousItemDownloadingStatusKey`; returns `true` only when
      the key resolves to "not downloaded," `false` for "downloaded,"
      "current," a missing/inapplicable key, or any query error
- [x] 1.2 Add a `#[cfg(not(target_os = "macos"))]` stub `is_offloaded`
      returning `false` unconditionally
- [x] 1.3 Register the new module in `crates/dtrpg-ui/src/util/mod.rs`

## 2. Data Model

- [x] 2.1 Add `pub offloaded: bool` to `LibraryItemFile`
      (`crates/dtrpg-ui/src/data/library.rs`) with `#[serde(default)]` so
      existing on-disk catalog cache entries deserialize as `false`
- [x] 2.2 Add `LibraryItem::is_offloaded(&self) -> bool`, `true` when
      `self.status == ItemStatus::Downloaded` and any file has
      `offloaded == true`
- [x] 2.3 Update every `LibraryItemFile { .. }` struct literal in tests and
      fixtures (`file_presence.rs`, `library.rs`, `catalog_cache.rs`,
      `util/stubs.rs`, others found via `find_references`) to include the
      new field

## 3. Verification Pass Integration

- [x] 3.1 In `verify_item_downloads`
      (`crates/dtrpg-ui/src/util/file_presence.rs`), when a file's resolved
      path exists, call `icloud_status::is_offloaded` and set
      `file.offloaded`; when the path does not exist, set `file.offloaded =
      false`
- [x] 3.2 Include a file's `offloaded` flag changing in the function's
      `changed` return value, alongside the existing `downloaded`/status
      change detection

## 4. UI: Status Glyph and Tooltip

- [x] 4.1 Add an `offloaded: bool` parameter to `render_status`
      (`crates/dtrpg-ui/src/ui/views/catalog_view.rs`) and update its three
      call sites to pass `item.is_offloaded()` (or the equivalent for the
      grouped/thumbs render paths)
- [x] 4.2 In `render_status`, when `status == ItemStatus::Downloaded &&
      offloaded`, render a distinct glyph variant (e.g. reuse
      `cloud-sync.svg` or `download-cloud.svg` from `assets/icons/`) with a
      tooltip distinct from both the plain-Downloaded dot and the Cloud
      glyph, indicating the file has been offloaded to iCloud to save disk
      space
- [x] 4.3 Add the same offloaded badge variant to the thumbs/grid card
      badge overlay (`crates/dtrpg-ui/src/ui/library/cover.rs` or wherever
      `item-download-badge`'s `Badge` overlay is rendered)
- [x] 4.4 In `item_popover_view.rs`, alongside the existing `is_downloaded`
      computation, compute `is_offloaded` and show the offloaded variant on
      the download button/status indicator with an updated tooltip
- [x] 4.5 In `detail_panel_view.rs`, alongside the existing `is_downloaded`
      computation, compute `is_offloaded` and show the offloaded variant on
      the download button/status glyph with an updated tooltip

## 5. Tests

- [x] 5.1 Unit test: `icloud_status::is_offloaded` returns `false` for a
      plain local file created in a tempdir (not in any ubiquitous
      container)
- [x] 5.2 Unit test: `icloud_status::is_offloaded` returns `false` for a
      nonexistent path (query against a path that doesn't exist should not
      panic or misreport)
- [x] 5.3 Unit test: `verify_item_downloads` sets `offloaded: false` on a
      file whose path doesn't exist, even if the flag was previously `true`
      (stale state from a prior offloaded observation)
- [x] 5.4 Unit test: `verify_item_downloads`'s `changed` return value is
      `true` when only `offloaded` differs from the prior value (downloaded
      flag and status unchanged)
- [x] 5.5 Unit test: `LibraryItem::is_offloaded` returns `false` when status
      is `Cloud` regardless of any file's `offloaded` flag (offloaded is
      only meaningful for files actually marked downloaded)
- [x] 5.6 Manual verification (call out in PR description): on a real
      Mac signed into iCloud, force-evict a downloaded file in the app's
      storage folder via Finder ("Remove Download") and confirm the app
      shows the offloaded glyph/tooltip after the next verification pass,
      per design.md's noted CI limitation

## 6. Build and Quality

- [x] 6.1 `cargo check --workspace`
- [x] 6.2 `cargo clippy --all-targets --all-features -- -D warnings`
- [x] 6.3 `cargo fmt --all -- --check`
- [x] 6.4 `cargo test --workspace`
