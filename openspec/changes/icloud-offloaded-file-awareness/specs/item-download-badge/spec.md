## MODIFIED Requirements

### Requirement: Thumbs and grid cards show download status badge

Each catalog item card in thumbs and grid views SHALL display a `Badge`
overlay that visually indicates whether the item is downloaded to the
device, downloaded but offloaded (iCloud evicted the local data), or
cloud-only.

#### Scenario: Downloaded item shows filled badge
- **WHEN** an item has `ItemStatus::Downloaded` and none of its downloaded
  files are offloaded
- **THEN** its thumbs/grid card shows a green dot `Badge` (or equivalent
  accent indicator) in the corner

#### Scenario: Downloaded but offloaded item shows a distinct badge variant
- **WHEN** an item has `ItemStatus::Downloaded` and at least one of its
  downloaded files is offloaded
- **THEN** its thumbs/grid card shows a distinct badge variant (visually
  different from both the fully-downloaded and cloud-only badges) with a
  tooltip indicating the file has been offloaded to iCloud to save disk
  space

#### Scenario: Cloud item shows no badge or a muted indicator
- **WHEN** an item has `ItemStatus::Cloud`
- **THEN** its thumbs/grid card either shows no badge or a muted dot so
  downloaded items are visually distinct

## ADDED Requirements

### Requirement: List view, item popover, and detail panel show the offloaded state

Wherever the catalog list view's status glyph, the item popover's download
button, or the detail panel's download button/status glyph currently
indicate `ItemStatus::Downloaded`, they SHALL show a distinct visual variant
and tooltip when the item's downloaded files are offloaded, matching the
thumbs/grid badge's offloaded variant.

#### Scenario: List row status glyph shows offloaded variant
- **WHEN** an item has `ItemStatus::Downloaded` and at least one downloaded
  file is offloaded
- **THEN** the list view's status glyph for that row shows the offloaded
  variant with a tooltip indicating the file has been offloaded to iCloud

#### Scenario: Item popover and detail panel show offloaded variant
- **WHEN** the user opens the item popover or detail panel for an item with
  `ItemStatus::Downloaded` and at least one downloaded file offloaded
- **THEN** the download button/status glyph shows the offloaded variant with
  a tooltip indicating the file has been offloaded to iCloud
