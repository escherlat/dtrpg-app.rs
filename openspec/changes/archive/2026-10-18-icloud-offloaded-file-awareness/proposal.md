## Why

When the app's storage root sits inside an iCloud Drive-synced folder, macOS
can evict a downloaded file's local data to reclaim disk space while leaving
the file's entry in place ("dataless file"). The app currently derives
`downloaded` purely from `Path::exists()`
(`crates/dtrpg-ui/src/util/file_presence.rs`), which reports an evicted file
as present and downloaded even though its bytes aren't on disk and reading
it would trigger an on-demand iCloud re-download. Users see "Downloaded" for
files that are actually sitting in the cloud, which is misleading about
local disk usage and about whether opening the file will be instant or will
require a network fetch.

## What Changes

- Detect, on macOS, when a file at its resolved on-disk path is an
  iCloud "dataless" (offloaded) placeholder rather than fully materialized
  local data, using the `NSURLUbiquitousItemDownloadingStatusKey` resource
  value (via the existing `objc2`/`objc2-foundation` dependencies).
- Add an `offloaded: bool` field to `LibraryItemFile`, set during the
  existing file-presence verification pass (catalog-wide on load, on-demand
  on item selection) alongside the existing `downloaded` flag.
- Surface a distinct "Downloaded (offloaded)" visual state — status glyph
  variant and tooltip — in the catalog list/grid/thumbs views, the item
  popover, and the detail panel, wherever the current Downloaded state is
  shown, without introducing a new `ItemStatus` variant: an offloaded item
  is still `ItemStatus::Downloaded` (the user owns/has downloaded it) with
  `offloaded` as additional per-file/per-item metadata.
- No change to sidebar filtering, section counts, or download/removal
  actions — an offloaded item continues to count as "On This Device" and
  its context-menu actions (remove, reveal in Finder) are unaffected;
  reveal-in-Finder on an offloaded file still opens Finder to it (macOS
  will show its cloud/eviction badge natively).
- On non-macOS platforms, `offloaded` is always `false` (no-op detection).

## Capabilities

### New Capabilities
- `icloud-offloaded-file-detection`: macOS-specific detection of whether a
  file at a given path is an iCloud dataless/offloaded placeholder versus
  fully materialized local data.

### Modified Capabilities
- `verify-downloaded-status-against-disk`: the file-presence verification
  pass also sets each file's new `offloaded` flag, in addition to the
  existing `downloaded` flag, using the same catalog-load and on-demand
  triggers.
- `item-download-badge`: the Downloaded status glyph gains a distinct
  visual variant and tooltip when the item's downloaded files are offloaded,
  shown in list/grid/thumbs, the item popover, and the detail panel.

## Impact

- `crates/dtrpg-ui/src/util/file_presence.rs`: extend `verify_item_downloads`
  to also resolve and set `offloaded`.
- `crates/dtrpg-ui/src/data/library.rs`: add `offloaded` to
  `LibraryItemFile`; add a helper to derive an item's offloaded state from
  its files.
- New macOS-only module (e.g.
  `crates/dtrpg-ui/src/util/icloud_status.rs`) wrapping
  `NSURLUbiquitousItemDownloadingStatusKey` via `objc2-foundation`, gated
  behind `#[cfg(target_os = "macos")]` with a stub returning `false`
  elsewhere.
- `crates/dtrpg-ui/src/ui/views/catalog_view.rs`,
  `item_popover_view.rs`, `detail_panel_view.rs`: render the offloaded
  glyph/tooltip variant where the Downloaded glyph is currently rendered.
- `crates/dtrpg-ui/Cargo.toml`: no new dependencies — reuses `objc2` and
  `objc2-foundation`, already present.
- `crates/dtrpg-ui/src/data/catalog_cache.rs`: persist the `offloaded` field
  through the on-disk catalog cache serialization (default `false` on
  deserialize of older cache entries missing the field).
