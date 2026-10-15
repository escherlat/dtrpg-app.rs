## 1. Remove the cover-corner badge

- [ ] 1.1 Remove both `Badge::new().dot().color(gpui::green())` call sites
      in `catalog_view.rs` (grid cell, thumbs cell) wrapping the cover
      element.

## 2. Remove the publisher-line dot; move the cloud glyph to the title

- [ ] 2.1 Remove `render_status`'s dot rendering for `ItemStatus::Downloaded`
      next to the publisher name in the list, thumbs, and grid
      row-rendering functions.
- [ ] 2.2 Move the ☁ glyph rendered for `ItemStatus::Cloud` from next to the
      publisher name to immediately left of the title text, in each of
      list, thumbs, and grid row layouts.
- [ ] 2.3 Update or remove `render_verifying_indicator` as needed to keep
      the in-flight presence-check spinner working at the glyph's new
      position, if it anchors off `render_status`'s old placement.

## 3. Remove the inline "Show in Finder" row

- [ ] 3.1 Remove the `reveal_row` element and its construction in each
      row-rendering function (list, thumbs, grid) for `ItemStatus::Downloaded`
      items.
- [ ] 3.2 Confirm the `···` context menu's "Reveal in Finder" action is
      unaffected and still present for downloaded items.

## 4. Move the downloaded-status text into the row's tooltip

- [ ] 4.1 Extend the row/card's existing tooltip (plain `.tooltip()`, or the
      `HoverCard` from `use-hovercard-catalog-tooltips` if that change has
      landed) to state "Downloaded" for `ItemStatus::Downloaded` items. No
      tooltip text is needed for `ItemStatus::Cloud` beyond what the ☁ glyph
      already conveys, though adding "Cloud" is harmless if the tooltip
      already shows other item metadata.

## 5. Coordinate with in-flight changes

- [ ] 5.1 If `icloud-offloaded-file-awareness` has not yet been implemented,
      update its proposal to surface the offloaded state as tooltip text
      alongside the Downloaded/Cloud line added in 4.1, rather than as a
      badge/glyph variant on indicators this change removes.

## 6. Build and quality

- [ ] 6.1 `cargo check --workspace --all-targets`
- [ ] 6.2 `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] 6.3 `cargo +nightly fmt --check` (invoke the real nightly rustfmt
      directly — see the gpui-kit migration branch's papercut notes on
      `rustup run nightly cargo fmt` silently resolving the wrong toolchain
      in this repo)
- [ ] 6.4 `cargo test --workspace`

## 7. Manual verification

- [ ] 7.1 Confirm no dot or badge remains on catalog cards or rows, in any
      of list, thumbs, and grid presentations, for downloaded items.
- [ ] 7.2 Confirm the ☁ glyph appears immediately left of the title text
      (not next to the publisher name) for cloud-only items, in all three
      presentations.
- [ ] 7.3 Confirm no "Show in Finder" text row remains beneath any row's
      title/publisher line.
- [ ] 7.4 Confirm hovering a downloaded row/card's tooltip states it is
      downloaded.
- [ ] 7.5 Confirm "Reveal in Finder" still works from the `···` context menu
      for a downloaded item.
