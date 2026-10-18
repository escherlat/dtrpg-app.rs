## REMOVED Requirements

### Requirement: Sequential single-item processing
**Reason**: Contradicted `thumbnail-queue-concurrency`, which has allowed up to
`max_concurrent_downloads` thumbnail fetches at once since `download-queue-concurrency-control`.
Thumbnails have not loaded one at a time since.
**Migration**: No change in behavior. `thumbnail-queue-concurrency` is the requirement in force.

## MODIFIED Requirements

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

### Requirement: Manual trigger via context menu
Every catalog entry in all three layouts (list, thumbs, grid) SHALL expose a "Load Thumbnail" context menu item that loads the entry's thumbnail ahead of background loading, fetching it from the network rather than the on-disk cache.

#### Scenario: Context menu loads the thumbnail
- **WHEN** the user right-clicks a catalog entry and selects "Load Thumbnail"
- **THEN** the entry's `cover_url` is scheduled ahead of any background thumbnail loading still waiting, and is fetched from the network even if a cached copy exists
