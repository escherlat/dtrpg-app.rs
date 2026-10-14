## Why

Catalog grid/thumbs/list rows currently show a downloaded item's status two
ways at once: a green dot `Badge` on the cover thumbnail's corner
(`item-download-badge`), and a second, differently-colored dot (or a ☁
glyph for cloud-only items) next to the publisher name below it
(`render_status` in `catalog_view.rs`). The second dot is drawn with the
active theme's accent color rather than a fixed color, so under a dark theme
like Ink it renders orange — a different color from the hardcoded-green
cover badge for the exact same status, with no indication either dot is
clickable or what distinguishes them. Neither carries a label or tooltip of
its own. Rows also show a permanent "Show in Finder" text line below the
title for downloaded items, duplicating the "Reveal in Finder" action
already available from the row's own context menu (opened via its `···`
trigger), adding a line of clutter to every row for an action most users
reach from the menu.

## What Changes

- Remove the cover-thumbnail corner `Badge` dot (`item-download-badge`'s
  current green-dot requirement).
- Remove the second status dot / ☁ glyph rendered next to the publisher
  name (`render_status` in `catalog_view.rs`).
- Remove the permanent "Show in Finder" text row beneath the title/publisher
  line in list, thumbs, and grid rows. Reveal-in-Finder remains available,
  unchanged, from the row's existing `···` context menu.
- Surface the item's download status (Downloaded / Cloud) as text in the
  row's existing hover tooltip instead, so the information survives but
  requires a hover rather than two always-visible, uncolor-coded dots and a
  permanent text row.

## Capabilities

### Modified Capabilities

- `item-download-badge`: replaces the corner-badge requirement with a
  tooltip-based status indicator; no dot or glyph is drawn on the card or
  row by default.

### New Capabilities

- `catalog-row-status-affordances`: a catalog row's download status is
  conveyed through its tooltip rather than a dedicated badge/dot/glyph, and
  reveal-in-Finder is reached only through the row's context menu, not a
  permanent inline text row.

## Impact

- `crates/dtrpg-ui/src/ui/views/catalog_view.rs`: remove both
  `Badge::new().dot().color(gpui::green())` call sites (grid and thumbs
  cover cells), remove `render_status`'s call sites next to the publisher
  name (list, thumbs, grid), remove the `reveal_row`/"Show in Finder" text
  element and its construction in each row-rendering function. Extend
  whatever tooltip the row already shows (the plain `.tooltip()` tracked by
  `catalog-title-tooltip`, or the `HoverCard` from the pending
  `use-hovercard-catalog-tooltips` change if that lands first) to include
  the item's download status as a text line.
- `openspec/changes/icloud-offloaded-file-awareness`: that pending,
  unimplemented change plans to add a third *visual badge variant*
  ("offloaded") to the exact corner badge and `render_status` glyph this
  change removes, modifying the same `item-download-badge` capability. It
  needs to be revised to surface the offloaded state as additional tooltip
  text instead of a glyph variant once this change lands — it should not be
  implemented against the badge/glyph this change deletes. Land this change
  first, or coordinate the two.
- `openspec/changes/use-hovercard-catalog-tooltips`: that pending,
  unimplemented change already plans to add "status" text to a `HoverCard`
  on catalog rows. This change's tooltip-based status requirement is
  satisfied by either the current plain tooltip or that `HoverCard`,
  whichever lands first; no conflict, but worth landing them together so
  "status in tooltip" only needs implementing once.
