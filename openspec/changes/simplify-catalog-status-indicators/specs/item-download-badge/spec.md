## MODIFIED Requirements

### Requirement: Thumbs and grid cards show download status badge

Each catalog item card in thumbs and grid views SHALL NOT display a `Badge`
or dot overlay indicating download status. The card's hover tooltip SHALL
state whether the item is downloaded to the device or cloud-only.

#### Scenario: Downloaded item shows filled badge
- **WHEN** an item has `ItemStatus::Downloaded`
- **THEN** its thumbs/grid card shows no dot, badge, or glyph overlay for
  download status
- **AND** hovering the card's tooltip states the item is downloaded

#### Scenario: Cloud item shows no badge or a muted indicator
- **WHEN** an item has `ItemStatus::Cloud`
- **THEN** its thumbs/grid card shows no dot, badge, or glyph overlay for
  download status
- **AND** hovering the card's tooltip states the item is cloud-only
