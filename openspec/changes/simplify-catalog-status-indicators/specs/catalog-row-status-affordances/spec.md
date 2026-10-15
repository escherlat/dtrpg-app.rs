## ADDED Requirements

### Requirement: Downloaded status has no dedicated dot or glyph; cloud status keeps an icon, left of the title

A catalog row (list, thumbs, or grid layout) SHALL NOT render a dedicated
dot next to the publisher name to indicate `ItemStatus::Downloaded`. The
row's hover tooltip SHALL state that the item is downloaded instead. A
`ItemStatus::Cloud` item SHALL still show a ☁ glyph, since it is an
intuitive, self-explanatory indicator unlike the removed dots, but
positioned immediately to the left of the row's title text rather than next
to the publisher name.

#### Scenario: Downloaded row shows no dot
- **WHEN** an item has `ItemStatus::Downloaded`
- **THEN** the row shows no dot next to the publisher name or anywhere else
  indicating download status
- **AND** hovering the row's tooltip states the item is downloaded

#### Scenario: Cloud row shows a ☁ glyph to the left of the title
- **WHEN** an item has `ItemStatus::Cloud`
- **THEN** a ☁ glyph appears immediately to the left of the row's title text
  in list, thumbs, and grid layouts
- **AND** the row's publisher line shows no glyph of its own
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
