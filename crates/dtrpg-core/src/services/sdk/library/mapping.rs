//! Translates Rust SDK response shapes (`OrderProductItem` and its sideloaded
//! `included` resources) into the app's domain [`LibraryItem`] model.

use std::collections::{HashMap, HashSet};

use dtrpg_sdk::{
    IncludedItem, OrderProductAttributes, OrderProductFile, OrderProductInfo, OrderProductItem,
    PaginationLinks,
};

use crate::constants::{BYTES_PER_MB, DEFAULT_COLOR, DTRPG_IMAGES_BASE_URL};
use crate::data::{
    enums::ItemStatus,
    library::{LibraryItem, LibraryItemFile},
};
use crate::util::datetime::parse_rfc3339_to_epoch;

/// Extracts the last page number from a [`PaginationLinks`] `last` URL.
///
/// Parses the `page` query parameter from the URL using a simple string split.
/// Returns `None` if `links.last` is absent or contains no valid `page` value.
pub(super) fn last_page_from_links(links: &PaginationLinks) -> Option<u32> {
    let last_url = links.last.as_deref()?;
    // Find "page=" in the query string and parse the digits that follow.
    let page_part = last_url.split("page=").nth(1)?;
    let digits: String = page_part.chars()
                                  .take_while(|c| c.is_ascii_digit())
                                  .collect();
    digits.parse::<u32>().ok().filter(|&n| n > 0)
}

/// Builds a lookup from JSON:API resource id (e.g.
/// `"/api/vBeta/publishers/117"`) to publisher display name, from one page's
/// `included` array.
///
/// Keyed by resource id — not `royalty_publisher_id` — because the live API
/// has been observed to return an ordered product whose `royaltyPublisherId`
/// does not match the id referenced by its own `relationships.publisher`
/// (e.g. a reprint/white-label arrangement): matching on `royalty_publisher_id`
/// silently resolves to the wrong publisher's entry (or no entry at all) in
/// that case. Each ordered product's `relationships.publisher.data.id` is the
/// correct key into this map — see [`resolve_publisher`].
pub(super) fn publisher_lookup(included: &[IncludedItem]) -> HashMap<String, String> {
    included.iter()
            .filter_map(|entry| {
                entry.as_publisher()
                     .map(|publisher| (entry.id.clone(), publisher.name))
            })
            .collect()
}

/// Builds a lookup from JSON:API resource id (e.g.
/// `"/api/vBeta/products/187766"`) to `Product` resource attributes, from one
/// page's `included` array.
///
/// The live API sideloads `Product` (and `Publisher`/`Order`) resources in
/// `included` rather than embedding them on `OrderProductAttributes` directly;
/// each ordered product's `relationships.product.data.id` is the key into this
/// map.
pub(super) fn product_lookup(included: &[IncludedItem]) -> HashMap<String, OrderProductInfo> {
    included.iter()
            .filter_map(|entry| {
                entry.as_product()
                     .map(|product| (entry.id.clone(), product))
            })
            .collect()
}

/// Derives an uppercase format label (e.g. `"PDF"`, `"EPUB"`) from a file's
/// extension.
///
/// Returns `None` if `filename` has no extension.
fn file_extension_label(filename: &str) -> Option<String> {
    filename.rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_uppercase())
}

// Prefer the publisher name embedded directly on `attributes.publisher`
// (present on newer API responses); fall back to the sideloaded `included`
// publisher lookup resolved via `relationships.publisher.data.id` (see
// `publisher_lookup`'s doc comment for why this must be relationship-based
// rather than keyed by `royalty_publisher_id`); finally fall back to a
// placeholder if neither source resolves.
fn resolve_publisher(item: &OrderProductItem, publishers: &HashMap<String, String>) -> String {
    item.attributes
        .publisher
        .as_ref()
        .map(|p| p.name.clone())
        .or_else(|| {
            item.relationships
                .as_ref()
                .and_then(|r| r.publisher.as_ref())
                .and_then(|r| r.data.as_ref())
                .and_then(|d| publishers.get(&d.id).cloned())
        })
        .unwrap_or_else(|| format!("Publisher {}", item.attributes.royalty_publisher_id))
}

// The live API resolves `Product` metadata (cover images) via
// `relationships.product` against the response's sideloaded `included` array
// rather than embedding it directly on `attributes` — fall back to the
// embedded field in case a future/legacy response shape does embed it inline.
fn resolve_product_info<'a>(item: &'a OrderProductItem,
                            products: &'a HashMap<String, OrderProductInfo>)
                            -> Option<&'a OrderProductInfo> {
    item.relationships
        .as_ref()
        .and_then(|r| r.product.as_ref())
        .and_then(|r| r.data.as_ref())
        .and_then(|d| products.get(&d.id))
        .or(item.attributes.product.as_ref())
}

// Small render contexts (grid card, thumb row) want the smallest
// correctly-sized source, falling back to progressively larger images only if
// no pre-generated thumbnail exists for this product.
fn resolve_cover_url(product_info: Option<&OrderProductInfo>) -> Option<String> {
    product_info.and_then(|p| {
                    p.thumbnail
                     .as_deref()
                     .or(p.thumbnail_100.as_deref())
                     .or(p.image.as_deref())
                     .or(p.web_image.as_deref())
                     .map(|path| format!("{DTRPG_IMAGES_BASE_URL}{path}"))
                })
}

// The detail panel (~480px wide) wants the largest available source so it
// doesn't upscale a small pre-generated thumbnail past its native resolution;
// pre-generated thumbnails are a last-resort fallback for catalog entries the
// API never populated a full-size or WebP image for.
fn resolve_detail_cover_url(product_info: Option<&OrderProductInfo>) -> Option<String> {
    product_info.and_then(|p| {
                    p.image
                     .as_deref()
                     .or(p.web_image.as_deref())
                     .or(p.thumbnail.as_deref())
                     .or(p.thumbnail_100.as_deref())
                     .map(|path| format!("{DTRPG_IMAGES_BASE_URL}{path}"))
                })
}

fn resolve_kind(attributes: &OrderProductAttributes) -> String {
    attributes.filters
              .as_ref()
              .and_then(|filters| filters.iter().find(|f| f.parent_filter_id == 0))
              .map(|f| {
                  if f.parent_name.is_empty() {
                      f.name.clone()
                  }
                  else {
                      f.parent_name.clone()
                  }
              })
              .unwrap_or_else(|| "Library item".to_string())
}

// File `title` is the document's display name (e.g. "Player's Handbook"), not
// a format type — derive the format from the file extension instead.
fn resolve_format(files: &[OrderProductFile]) -> String {
    let mut format_parts: Vec<String> = files.iter()
                                             .filter_map(|f| file_extension_label(&f.filename))
                                             .collect::<HashSet<_>>()
                                             .into_iter()
                                             .collect();
    format_parts.sort();
    if format_parts.is_empty() {
        "PDF".to_string()
    }
    else {
        format_parts.join(" + ")
    }
}

// Per-item file breakdown — more than one entry marks this a multi-item
// catalog entry (see the `catalog-entry-detail-view` capability).
//
// The API has been observed to repeat the same download record (identical
// `orderProductDownloadId`, identical `title`) verbatim across entries in
// `files` for what is genuinely a single file — deduplicate those exact
// repeats so a single-file product never renders its one item more than once
// in the detail tab's item list. `orderProductDownloadId` alone is NOT a
// reliable per-file key — the API has also been observed to reuse the same
// download id across genuinely distinct files within a bundle, so
// deduplicating on the id alone would incorrectly collapse a real multi-file
// entry down to one and hide its item-count badge. Keying on
// `(order_product_download_id, title)` catches true verbatim repeats while
// never merging two files with different titles.
fn map_files(files: &[OrderProductFile]) -> Vec<LibraryItemFile> {
    let mut seen_files: HashSet<(u64, &str)> = HashSet::new();
    files.iter()
         .filter(|f| seen_files.insert((f.order_product_download_id, f.title.as_str())))
         .map(|f| LibraryItemFile { id:         f.order_product_download_id.to_string().into(),
                                    index:      f.index,
                                    name:       f.title.as_str().into(),
                                    format:
                                        file_extension_label(&f.filename).unwrap_or_else(|| {
                                                                             "FILE".to_string()
                                                                         })
                                                                         .into(),
                                    size_mb:    f.size as f64 / BYTES_PER_MB,
                                    downloaded: false,
                                    offloaded:  false, })
         .collect()
}

fn resolve_year(attributes: &OrderProductAttributes) -> u32 {
    attributes.file_last_modified
              .as_deref()
              .or(attributes.date_purchased.as_deref())
              .and_then(|date| date.get(..4))
              .and_then(|y| y.parse::<u32>().ok())
              .unwrap_or(0)
}

pub(super) fn map_order_product(item: &OrderProductItem, publishers: &HashMap<String, String>,
                                products: &HashMap<String, OrderProductInfo>, order: u32)
                                -> LibraryItem {
    let attributes = &item.attributes;

    let numeric_id = attributes.order_product_id
                               .max(item.id.parse::<u64>().unwrap_or_default())
                               .max(attributes.product_id);

    let publisher = resolve_publisher(item, publishers);
    let product_info = resolve_product_info(item, products);
    let cover_url = resolve_cover_url(product_info);
    let detail_cover_url = resolve_detail_cover_url(product_info);
    let kind = resolve_kind(attributes);
    let format = resolve_format(&attributes.files);
    let size_mb = attributes.files.iter().map(|f| f.size as f64).sum::<f64>() / BYTES_PER_MB;
    let files = map_files(&attributes.files);
    let year = resolve_year(attributes);

    // The API reports `datePurchased`/`fileLastModified` as raw RFC 3339
    // strings (e.g. "2024-07-16T10:45:52-05:00"). Parse them into
    // `date_added` / `date_updated` rather than embedding the machine
    // format as text — the detail panel renders these as relative labels
    // with a human-readable absolute-date tooltip (see
    // `render_relative_date_value`).
    //
    // `datePurchased` is documented as present "if available" — it's absent
    // for some ordered products (observed for bundle members), which would
    // otherwise leave `date_added` unset and make an item invisible to the
    // "Recently Updated" filter regardless of how recently it actually
    // appeared in the user's library. `attributes.order.dateCreated` (the
    // embedded order's creation date, always present alongside the standard
    // order_products response — no extra request needed) is a reliable
    // fallback: every ordered product belongs to an order, and the order's
    // creation date is at least as good a "when this entered the library"
    // signal as the purchase date.
    let date_purchased_epoch = attributes.date_purchased
                                         .as_deref()
                                         .or_else(|| {
                                             attributes.order
                                                       .as_ref()
                                                       .and_then(|o| o.date_created.as_deref())
                                         })
                                         .and_then(parse_rfc3339_to_epoch);
    let file_last_modified_epoch = attributes.file_last_modified
                                             .as_deref()
                                             .and_then(parse_rfc3339_to_epoch);

    LibraryItem { id: item.id.as_str().into(),
                  numeric_id,
                  order_product_id: attributes.order_product_id,
                  product_id: attributes.product_id,
                  title: attributes.name.as_str().into(),
                  publisher: publisher.as_str().into(),
                  line: "".into(),
                  kind: kind.as_str().into(),
                  format: format.as_str().into(),
                  pages: 0,
                  size_mb,
                  year,
                  added_order: order,
                  status: ItemStatus::Cloud,
                  color: DEFAULT_COLOR.into(),
                  desc: "".into(),
                  cover_url: cover_url.map(Into::into),
                  detail_cover_url: detail_cover_url.map(Into::into),
                  date_added: date_purchased_epoch,
                  date_updated: file_last_modified_epoch,
                  thumbnail_last_attempted: None,
                  is_available: true,
                  availability_last_checked: None,
                  files }
}

#[cfg(test)]
// Fixtures intentionally use concise unwraps for known-good test data.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "mapping_test_support.rs"]
mod mapping_test_support;

#[cfg(test)]
// Mapping tests keep the original fixture assertions and setup style.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "mapping_tests.rs"]
mod mapping_tests;
