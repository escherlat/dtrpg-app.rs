# thumbnail-loading-queue Specification

## Purpose
TBD - created by archiving change catalog-thumbnail-loading. Update Purpose after archive.
## Requirements
### Requirement: Auto-enqueue thumbnails on catalog page arrival
When a catalog page is appended, every item whose `cover_url` is `Some` AND whose cover is not already present in `CoverCache` AND is not already in-flight SHALL be added to the thumbnail loading queue.

#### Scenario: New items with cover URLs are enqueued
- **WHEN** `append_catalog_page` is called with items that have `cover_url` set
- **THEN** each such item whose cover is not cached and not in-flight is added to the queue

#### Scenario: Already-cached items are skipped
- **WHEN** `append_catalog_page` is called and `CoverCache` already contains the cover for an item
- **THEN** that item is NOT added to the queue

#### Scenario: In-flight items are skipped
- **WHEN** `append_catalog_page` is called and an item's cover is already marked in-flight in `CoverCache`
- **THEN** that item is NOT added to the queue

### Requirement: Successful fetch stored in CoverCache
When a thumbnail HTTP request returns a successful response with image data, the bytes SHALL be stored in `CoverCache` keyed by the item's `cover_url`, the in-flight marker SHALL be cleared, and the catalog view SHALL be re-rendered.

#### Scenario: HTTP 200 response stores cover
- **WHEN** a thumbnail fetch returns HTTP 200 with image bytes
- **THEN** the bytes are inserted into `CoverCache` and the catalog refreshes

#### Scenario: Failed fetch clears in-flight marker
- **WHEN** a thumbnail fetch fails (network error or non-200 response)
- **THEN** the in-flight marker is cleared and the queue continues to the next item

### Requirement: Activity panel tracking
While thumbnail fetches are waiting or running, the activity panel SHALL show a single aggregated item. Its label SHALL reflect the number of covers not yet finished, and its progress SHALL be the fraction of the batch that has finished. When the last fetch in the batch finishes, the activity item SHALL be marked complete.

#### Scenario: Activity item appears when the first fetch starts
- **WHEN** the first thumbnail fetch of a batch starts
- **THEN** a new activity entry is created with a label indicating covers are loading

#### Scenario: Activity label and progress update as fetches finish
- **WHEN** a thumbnail fetch finishes and others have not
- **THEN** the activity entry's label is updated to the number not yet finished, and its progress to the number finished out of those finished plus those not yet finished

#### Scenario: Activity item completes when the last fetch finishes
- **WHEN** the last unfinished fetch of the batch finishes, whether it was waiting or already running when the others finished
- **THEN** the activity entry is marked complete, and not before

### Requirement: Thumbnail fetches run without requiring an ambient async runtime
The thumbnail HTTP fetch SHALL execute in a way that does not depend on an active Tokio (or other) async reactor being present on the executing thread, since `dtrpg-ui` does not own or depend on one and gpui's executors are not Tokio-backed.

#### Scenario: Fetch succeeds without a Tokio runtime in scope
- **WHEN** a thumbnail fetch runs on a `gpui` background-executor thread with no Tokio runtime entered anywhere in the process
- **THEN** the fetch completes successfully (using a mechanism, such as `reqwest`'s blocking client, that manages its own runtime internally rather than requiring one)

### Requirement: Manual trigger via context menu
Every catalog entry in all three layouts (list, thumbs, grid) SHALL expose a "Load Thumbnail" context menu item that loads the entry's thumbnail ahead of background loading, fetching it from the network rather than the on-disk cache.

#### Scenario: Context menu loads the thumbnail
- **WHEN** the user right-clicks a catalog entry and selects "Load Thumbnail"
- **THEN** the entry's `cover_url` is scheduled ahead of any background thumbnail loading still waiting, and is fetched from the network even if a cached copy exists
