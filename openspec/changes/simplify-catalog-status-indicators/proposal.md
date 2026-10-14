## Why

Catalog grid/thumbs/list rows currently show a downloaded item's status two
ways at once: a green dot `Badge` on the cover thumbnail's corner
(`item-download-badge`), and a second, differently-colored dot next to the
publisher name below it (`render_status` in `catalog_view.rs`). The second
dot is drawn with the active theme's accent color rather than a fixed color,
so under a dark theme like Ink it renders orange — a different color from
the hardcoded-green cover badge for the exact same status, with no
indication either dot is clickable or what distinguishes them, and no label
or tooltip of its own. Cloud-only items get a ☁ glyph in that same spot
instead of a dot — unlike the dots, that glyph is self-explanatory, but its
placement next to the publisher name rather than the title makes it easy to
miss. Rows also show a permanent "Show in Finder" text line below the title
for downloaded items, duplicating the "Reveal in Finder" action already
available from the row's own context menu (opened via its `···` trigger),
adding a line of clutter to every row for an action most users reach from
the menu.

## What Changes

- Remove the cover-thumbnail corner `Badge` dot (`item-download-badge`'s
  current green-dot requirement).
- Remove the accent-colored status dot shown for downloaded items next to
  the publisher name (`render_status` in `catalog_view.rs`).
- Keep the ☁ glyph for cloud-only items — it's intuitive, unlike the dots —
  but move it to sit immediately to the left of the row's title text instead
  of next to the publisher name.
- Remove the permanent "Show in Finder" text row beneath the title/publisher
  line in list, thumbs, and grid rows. Reveal-in-Finder remains available,
  unchanged, from the row's existing `···` context menu.
- Surface the Downloaded status as text in the row's existing hover tooltip
  (the Cloud glyph itself continues to convey Cloud status at a glance).

## Capabilities

### Modified Capabilities

- `item-download-badge`: replaces the corner-badge requirement with a
  tooltip-based status indicator; no dot or glyph is drawn on the card or
  row by default.

### New Capabilities

- `catalog-row-status-affordances`: a downloaded catalog row's status is
  conveyed through its tooltip rather than a dot; a cloud-only row keeps a
  ☁ glyph, repositioned to the left of the title; reveal-in-Finder is
  reached only through the row's context menu, not a permanent inline text
  row.

## Impact

- `crates/dtrpg-ui/src/ui/views/catalog_view.rs`: remove both
  `Badge::new().dot().color(gpui::green())` call sites (grid and thumbs
  cover cells); remove `render_status`'s dot rendering for
  `ItemStatus::Downloaded` next to the publisher name; move its ☁-glyph
  rendering for `ItemStatus::Cloud` from next to the publisher name to
  immediately left of the title text, in list, thumbs, and grid row layouts;
  remove the `reveal_row`/"Show in Finder" text element and its construction
  in each row-rendering function. Extend whatever tooltip the row already
  shows (the plain `.tooltip()` tracked by `catalog-title-tooltip`, or the
  `HoverCard` from the pending `use-hovercard-catalog-tooltips` change if
  that lands first) to state "Downloaded" for downloaded items.
- `openspec/changes/icloud-offloaded-file-awareness`: that pending,
  unimplemented change plans to add a third *visual badge variant*
  ("offloaded") to the exact corner badge and `render_status` dot this
  change removes, modifying the same `item-download-badge` capability. It
  needs to be revised to surface the offloaded state as additional tooltip
  text instead of a glyph variant once this change lands — it should not be
  implemented against the badge/dot this change deletes. Land this change
  first, or coordinate the two.
- `openspec/changes/use-hovercard-catalog-tooltips`: that pending,
  unimplemented change already plans to add "status" text to a `HoverCard`
  on catalog rows. This change's tooltip-based status requirement is
  satisfied by either the current plain tooltip or that `HoverCard`,
  whichever lands first; no conflict, but worth landing them together so
  "status in tooltip" only needs implementing once.
