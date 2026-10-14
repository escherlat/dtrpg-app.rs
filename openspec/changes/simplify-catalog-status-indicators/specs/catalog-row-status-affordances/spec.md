## ADDED Requirements

### Requirement: No dedicated status dot or glyph next to the publisher name

A catalog row (list, thumbs, or grid layout) SHALL NOT render a dedicated
dot or glyph next to the publisher name to indicate download status. The
row's hover tooltip SHALL state the item's download status instead.

#### Scenario: Downloaded row shows no publisher-line dot
- **WHEN** an item has `ItemStatus::Downloaded`
- **THEN** the row's publisher line shows no dot next to the publisher name
- **AND** hovering the row's tooltip states the item is downloaded

#### Scenario: Cloud row shows no publisher-line glyph
- **WHEN** an item has `ItemStatus::Cloud`
- **THEN** the row's publisher line shows no ☁ or other glyph next to the
  publisher name
- **AND** hovering the row's tooltip states the item is cloud-only

### Requirement: No permanent "Show in Finder" row; reveal stays in the context menu

A catalog row SHALL NOT render a permanent "Show in Finder" (or
platform-equivalent "Show in Explorer"/"Show in Files") text row beneath the
title or publisher line. The reveal-in-file-manager action remains available,
unchanged, from the row's existing `···` context menu for downloaded items.

#### Scenario: Downloaded row has no inline reveal text
- **WHEN** an item has `ItemStatus::Downloaded` and its row is rendered in
  list, thumbs, or grid layout
- **THEN** no "Show in Finder" (or platform-equivalent) text row appears
  beneath the title or publisher line

#### Scenario: Reveal-in-Finder remains in the context menu
- **WHEN** the user opens the `···` context menu on a row for an item with
  `ItemStatus::Downloaded`
- **THEN** the menu contains a "Reveal in Finder" (or platform-equivalent)
  action, identical to its current behavior
