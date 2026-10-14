//! Cover rendering: real thumbnail (cache-first) with generative fallback.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui::{Hsla, Image, ImageFormat, IntoElement, ParentElement, Styled, div, px, rgb};

use crate::data::library::LibraryItem;
use crate::util::hash::fnv1a_32;
use crate::util::image_format::{ImageKind, sniff};

// ── CoverCache
// ────────────────────────────────────────────────────────────────

/// Derives the cache key used for an item's large-context (detail panel)
/// cover image, distinct from its plain item id (used for the small-context
/// grid/thumb cover) so both can be cached — in memory and on disk —
/// independently.
#[must_use]
pub fn detail_cache_key(item_id: &str) -> Arc<str> {
    Arc::from(format!("{item_id}::detail"))
}

/// Disk-backed in-memory cache of cover images keyed by item id.
///
/// Stored as a GPUI app-level global so all views share the same cache without
/// coordination overhead.
pub struct CoverCache {
    /// Decoded GPUI images keyed by item id.
    pub images:    HashMap<Arc<str>, Arc<Image>>,
    /// Tracks items whose download is currently in flight.
    pub in_flight: HashSet<Arc<str>>,
}

impl gpui::Global for CoverCache {}

impl CoverCache {
    /// Returns a new, empty cache.
    pub fn new() -> Self {
        Self { images:    HashMap::new(),
               in_flight: HashSet::new(), }
    }

    /// Returns the cached GPUI image for `id`, if present.
    pub fn get(&self, id: &str) -> Option<Arc<Image>> {
        self.images.get(id).cloned()
    }

    /// Stores encoded image bytes for `id`, auto-detecting the image format,
    /// and clears its in-flight marker.
    pub fn insert(&mut self, id: Arc<str>, bytes: Vec<u8>) {
        self.in_flight.remove(&id);
        let format = match sniff(&bytes) {
            ImageKind::Png => ImageFormat::Png,
            ImageKind::Jpeg => ImageFormat::Jpeg,
            ImageKind::Webp => ImageFormat::Webp,
            ImageKind::Gif => ImageFormat::Gif,
            ImageKind::Bmp => ImageFormat::Bmp,
        };
        let image = Image::from_bytes(format, bytes);
        self.images.insert(id, Arc::new(image));
    }

    /// Returns `true` if a download is already in flight for `id`.
    pub fn is_in_flight(&self, id: &str) -> bool {
        self.in_flight.contains(id)
    }

    /// Marks `id` as having an in-flight download.
    pub fn mark_in_flight(&mut self, id: Arc<str>) {
        self.in_flight.insert(id);
    }
}

impl Default for CoverCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── Generative cover style
// ────────────────────────────────────────────────────

/// Motif shape used on a generative cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motif {
    Circle,
    Diamond,
    DoubleRule,
    Triangle,
}

/// All style parameters needed to render a generative cover.
#[derive(Debug, Clone)]
pub struct CoverStyle {
    pub background:    Hsla,
    pub foreground:    Hsla,
    pub foreground_70: Hsla,
    pub foreground_45: Hsla,
    pub foreground_22: Hsla,
    pub foreground_12: Hsla,
    pub motif:         Motif,
}

/// ITU-R 601 luminance check: returns `true` when the hex color is perceptually
/// light.
fn is_light_hex(hex: &str) -> bool {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return true;
    }
    let Ok(n) = u32::from_str_radix(&h[..6], 16)
    else {
        return true;
    };
    let r = (n >> 16) & 0xFF;
    let g = (n >> 8) & 0xFF;
    let b = n & 0xFF;
    (r * 299 + g * 587 + b * 114) / 1000 > 150
}

fn hex_to_hsla(hex: &str) -> Hsla {
    let h = hex.trim_start_matches('#');
    let n = u32::from_str_radix(&h[..6.min(h.len())], 16).unwrap_or(0x1C_2A_44);
    rgb(n).into()
}

/// Derives a deterministic `CoverStyle` from `item` metadata.
///
/// Same input always produces the same motif and foreground, regardless of
/// theme or density.
#[must_use]
pub fn cover_style(item: &LibraryItem) -> CoverStyle {
    let light = is_light_hex(&item.color);

    let (fg_r, fg_g, fg_b) = if light {
        (0x1C_u8, 0x18_u8, 0x13_u8) // near-black on light backgrounds
    }
    else {
        (0xEF_u8, 0xE6_u8, 0xD2_u8) // cream on dark backgrounds
    };

    let fg_n = (u32::from(fg_r) << 16) | (u32::from(fg_g) << 8) | u32::from(fg_b);
    let fg_base: Hsla = rgb(fg_n).into();
    let fg = |a: f32| Hsla { a, ..fg_base };

    let background = hex_to_hsla(&item.color);

    // Hash item id + title for motif — same algorithm as the JS prototype.
    let hash_input = format!("{}{}", item.id, item.title);
    let h = fnv1a_32(&hash_input);
    let motif = match h % 4 {
        0 => Motif::Circle,
        1 => Motif::Diamond,
        2 => Motif::DoubleRule,
        _ => Motif::Triangle,
    };

    CoverStyle { background,
                 foreground: fg(1.0),
                 foreground_70: fg(0.70),
                 foreground_45: fg(0.45),
                 foreground_22: fg(0.22),
                 foreground_12: fg(0.12),
                 motif }
}

// ── Generative cover element
// ──────────────────────────────────────────────────

fn render_motif(motif: Motif, fg: Hsla) -> impl IntoElement {
    let size = px(24.0);
    match motif {
        // ○ circle outline
        Motif::Circle => div().text_xl().text_color(fg).child("○").into_any_element(),
        // ◇ diamond outline (Unicode avoids CSS rotate)
        Motif::Diamond => div().text_xl().text_color(fg).child("◇").into_any_element(),
        // two horizontal rules
        Motif::DoubleRule => div().flex()
                                  .flex_col()
                                  .gap_1()
                                  .child(div().w(size).h(px(1.0)).bg(fg))
                                  .child(div().w(size).h(px(1.0)).bg(fg))
                                  .into_any_element(),
        // △ triangle outline (Unicode avoids CSS rotate)
        Motif::Triangle => div().text_xl().text_color(fg).child("△").into_any_element(),
    }
}

/// Renders a generative cover tile at the given pixel dimensions.
pub fn render_generative_cover(item: &LibraryItem, width: f32, height: f32, render_text: bool)
                               -> impl IntoElement + 'static + use<> {
    let style = cover_style(item);
    let bg = style.background;
    let fg = style.foreground;
    let fg70 = style.foreground_70;
    let fg45 = style.foreground_45;
    let motif = style.motif;

    let mut cover = div().w(px(width))
                         .h(px(height))
                         .bg(bg)
                         .flex()
                         .flex_col()
                         .overflow_hidden();

    if render_text {
        let publisher = item.publisher.to_string();
        let title = item.title.to_string();
        let line = item.line.to_string();

        cover = cover.justify_between()
                     .child(div().px_2()
                                 .pt_2()
                                 .text_color(fg70)
                                 .text_xs()
                                 .truncate()
                                 .child(publisher))
                     .child(div().flex()
                                 .flex_col()
                                 .items_center()
                                 .gap_1()
                                 .px_1()
                                 .child(render_motif(motif, fg45))
                                 .child(div().text_color(fg)
                                             .text_xs()
                                             .font_weight(gpui::FontWeight::SEMIBOLD)
                                             .child(title)))
                     .child(div().px_2()
                                 .pb_2()
                                 .text_color(fg45)
                                 .text_xs()
                                 .truncate()
                                 .child(line));
    }
    else {
        cover = cover.items_center()
                     .justify_center()
                     .child(render_motif(motif, fg45));
    }

    cover
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::enums::ItemStatus;
    use crate::util::stubs::*;

    fn make_item(id: &str, title: &str, color: &str) -> LibraryItem {
        LibraryItem { id:                        id.into(),
                      numeric_id:                0,
                      order_product_id:          0,
                      product_id:                0,
                      title:                     title.into(),
                      publisher:                 "Test".into(),
                      line:                      "Test Line".into(),
                      kind:                      "Core".into(),
                      format:                    "PDF".into(),
                      pages:                     200,
                      size_mb:                   50.0,
                      year:                      2020,
                      added_order:               100,
                      status:                    ItemStatus::Downloaded,
                      color:                     color.into(),
                      desc:                      "A test item.".into(),
                      cover_url:                 None,
                      detail_cover_url:          None,
                      date_added:                None,
                      date_updated:              None,
                      thumbnail_last_attempted:  None,
                      is_available:              true,
                      availability_last_checked: None,
                      files:                     Vec::new(), }
    }

    #[test]
    fn cover_style_deterministic() {
        let item = make_item("b1", "Player's Handbook", "#1C2A44");
        let s1 = cover_style(&item);
        let s2 = cover_style(&item);
        assert_eq!(s1.motif, s2.motif, "motif must be deterministic");
    }

    #[test]
    fn dark_background_gets_cream_foreground() {
        let item = make_item("b1", "Dark Book", "#1C2A44");
        let style = cover_style(&item);
        // Cream has high lightness
        assert!(style.foreground.l > 0.8,
                "expected cream foreground on dark bg");
    }

    #[test]
    fn light_background_gets_dark_foreground() {
        // #C9A02C is a lighter warm-gold: r=201,g=160,b=44 →
        // (201*299+160*587+44*114)/1000 ≈ 160 > 150
        let item = make_item("b_light", "Light Book", "#C9A02C");
        let style = cover_style(&item);
        assert!(style.foreground.l < 0.2,
                "expected dark foreground on light bg");
    }

    #[test]
    fn detail_cache_key_is_distinct_from_item_id_and_deterministic() {
        let key_a = detail_cache_key("b1");
        let key_b = detail_cache_key("b1");
        let key_other = detail_cache_key("b2");

        assert_ne!(key_a.as_ref(),
                   "b1",
                   "must not collide with the small-context cache key");
        assert_eq!(key_a, key_b,
                   "same item id must derive the same detail cache key");
        assert_ne!(key_a, key_other,
                   "different item ids must derive different detail cache keys");
    }

    #[test]
    fn all_stub_items_produce_valid_cover_style() {
        for item in stub_catalog() {
            let style = cover_style(&item);
            // Just ensure no panic and motif is one of the 4 variants.
            let _ = style.motif;
        }
    }

    #[test]
    fn malformed_color_falls_back_without_panicking() {
        // Other-details swatch rendering (`render_other_details` in
        // `detail_panel_view.rs`) reuses `cover_style(item).background`, so
        // this fallback path must never panic on a malformed or empty
        // `item.color`.
        for color in ["", "not-a-color", "#zzzzzz"] {
            let item = make_item("b_malformed", "Malformed Color Book", color);
            let style = cover_style(&item);
            let default_background: Hsla = rgb(0x1C_2A_44).into();
            assert_eq!(style.background, default_background,
                       "expected fallback background for malformed color {color:?}");
        }
    }
}
