//! Translates Rust SDK response shapes (`OrderProductItem` and its sideloaded
//! `included` resources) into the app's domain [`LibraryItem`] model.

use std::collections::{HashMap, HashSet};

use dtrpg_sdk::{
    IncludedItem, OrderProductAttributes, OrderProductFile, OrderProductInfo, OrderProductItem,
    PaginationLinks,
};
use dtrpg_ui::data::{
    enums::ItemStatus,
    library::{LibraryItem, LibraryItemFile},
};
use dtrpg_ui::util::datetime::parse_rfc3339_to_epoch;

use crate::constants::{BYTES_PER_MB, DEFAULT_COLOR, DTRPG_IMAGES_BASE_URL};

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
                                    downloaded: false, })
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use dtrpg_sdk::{
        FileChecksum, OrderProductAttributes, OrderProductFile, OrderProductItemResponse,
        OrderProductListResponse, OrderProductOrder, OrderProductRelationships, PaginationLinks,
        PaginationMeta, RelationshipData, RelationshipRef,
    };
    use dtrpg_ui::services::{LibraryService, LibraryServiceError};

    use super::super::RustSdkLibraryService;
    use super::super::gateway::SdkLibraryGateway;
    use super::*;

    struct FakeSdkGateway {
        list_result:   Result<OrderProductListResponse, LibraryServiceError>,
        detail_result: Result<OrderProductItemResponse, LibraryServiceError>,
    }

    impl FakeSdkGateway {
        fn seeded_with(item: OrderProductItem) -> Self {
            Self { list_result:
                       Ok(OrderProductListResponse { links:    pagination_links(None),
                                                     meta:     PaginationMeta { items_per_page: 100,
                                                                                current_page:   1, },
                                                     data:     vec![item.clone()],
                                                     included: Some(vec![
                    IncludedItem { id:            "/api/vBeta/publishers/7".to_string(),
                                   resource_type: "Publisher".to_string(),
                                   attributes:    serde_json::json!({
                                       "name": "Lantern Press",
                                       "publisherId": 7,
                                       "slug": "lantern-press",
                                   }), },
                ]), }),
                   detail_result: Ok(OrderProductItemResponse { data:     item,
                                                                included: None, }), }
        }
    }

    impl SdkLibraryGateway for FakeSdkGateway {
        fn list_order_products(&self, _params: dtrpg_sdk::LibraryItemsParams)
                               -> Result<OrderProductListResponse, LibraryServiceError> {
            self.list_result.clone()
        }

        fn get_order_product(&self, _id: u64)
                             -> Result<OrderProductItemResponse, LibraryServiceError> {
            self.detail_result.clone()
        }

        fn prepare_download(&self, _order_product_id: u64, _index: u32)
                            -> Result<serde_json::Value, LibraryServiceError> {
            Err(LibraryServiceError::new(dtrpg_ui::services::LibraryServiceErrorKind::NotFound,
                                         "not used"))
        }
    }

    fn order_product_item(id: u64, name: &str) -> OrderProductItem {
        OrderProductItem {
            id: id.to_string(),
            resource_type: "order_product".to_string(),
            attributes: OrderProductAttributes {
                order_id: 900,
                product_id: id,
                royalty_publisher_id: 7,
                isbn: None,
                name: name.to_string(),
                date_purchased: Some("2026-01-01T10:45:52-05:00".to_string()),
                filesize: Some(1024),
                final_price: 12.5,
                quantity: 1,
                bundle_id: 0,
                archived: 0,
                add_on_info: None,
                order_product_id: id,
                customer_id: 123,
                file_last_modified: Some("2026-01-02T08:30:00Z".to_string()),
                file_last_downloaded: None,
                files: vec![OrderProductFile {
                    index: 0,
                    order_product_download_id: 1234,
                    title: "PDF".to_string(),
                    filename: "better-dungeon.pdf".to_string(),
                    size: 1_048_576,
                    size_mb: "1.0".to_string(),
                    checksums: vec![FileChecksum {
                        checksum: "abc123".to_string(),
                        checksum_date: "2026-01-02".to_string(),
                    }],
                }],
                filters: Some(vec![dtrpg_sdk::OrderProductFilter {
                    filter_id: 1,
                    parent_filter_id: 0,
                    name: "Dungeon".to_string(),
                    parent_name: "Adventure".to_string(),
                }]),
                history: None,
                attributes: None,
                publisher: None,
                product: None,
                order: None,
            },
            relationships: None,
        }
    }

    fn pagination_links(next: Option<String>) -> PaginationLinks {
        PaginationLinks { self_: "self".to_string(),
                          first: None,
                          last: None,
                          prev: None,
                          next }
    }

    #[test]
    fn map_order_product_parses_date_purchased_into_date_added() {
        let item = order_product_item(515_276, "The Wellspring");

        let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

        // "2026-01-01T10:45:52-05:00" == 2026-01-01T15:45:52Z
        assert_eq!(mapped.date_added, Some(1_767_282_352));
    }

    #[test]
    fn map_order_product_falls_back_to_order_date_created_when_date_purchased_absent() {
        // `datePurchased` is documented as present "if available" — observed
        // absent for bundle members. `attributes.order.dateCreated` is
        // embedded on every standard order_products response and is a
        // reliable fallback so the item isn't left with no `date_added` at
        // all.
        let mut item = order_product_item(515_276, "The Wellspring");
        item.attributes.date_purchased = None;
        item.attributes.order =
            Some(OrderProductOrder { date_created: Some("2026-01-03T09:00:00Z".to_string()),
                                     order_id:     900, });

        let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

        // "2026-01-03T09:00:00Z"
        assert_eq!(mapped.date_added, Some(1_767_430_800));
    }

    #[test]
    fn map_order_product_prefers_date_purchased_over_order_date_created() {
        let mut item = order_product_item(515_276, "The Wellspring");
        item.attributes.order =
            Some(OrderProductOrder { date_created: Some("2020-01-01T00:00:00Z".to_string()),
                                     order_id:     900, });

        let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

        // "2026-01-01T10:45:52-05:00" == 2026-01-01T15:45:52Z, unchanged from
        // `date_purchased` despite an older `order.date_created` also being
        // set.
        assert_eq!(mapped.date_added, Some(1_767_282_352));
    }

    #[test]
    fn map_order_product_date_added_is_none_when_both_date_purchased_and_order_absent() {
        let mut item = order_product_item(515_276, "The Wellspring");
        item.attributes.date_purchased = None;
        item.attributes.order = None;

        let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

        assert_eq!(mapped.date_added, None);
    }

    #[test]
    fn map_order_product_parses_file_last_modified_into_date_updated() {
        let item = order_product_item(515_276, "The Wellspring");

        let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

        // "2026-01-02T08:30:00Z"
        assert_eq!(mapped.date_updated, Some(1_767_342_600));
    }

    #[test]
    fn map_order_product_desc_never_leaks_raw_rfc3339_timestamps() {
        let item = order_product_item(515_276, "The Wellspring");

        let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

        // Dates are surfaced as structured `date_added`/`date_updated` fields
        // and rendered by the detail panel as relative labels with an
        // absolute-date tooltip — never as raw text in `desc`.
        assert!(!mapped.desc.contains('T'),
                "desc leaked a raw RFC3339 timestamp: {}",
                mapped.desc);
        assert!(!mapped.desc.contains("2026-01"), "desc: {}", mapped.desc);
    }

    #[test]
    fn map_order_product_derives_format_from_file_extension_not_title() {
        let mut item = order_product_item(515_276, "The Wellspring");
        // File `title` is the document's display name, distinct from its
        // extension — the mapped format must come from the extension,
        // not this field.
        item.attributes.files =
            vec![OrderProductFile { index:                     0,
                                    order_product_download_id: 1234,
                                    title:                     "The Wellspring".to_string(),
                                    filename:                  "the-wellspring.epub".to_string(),
                                    size:                      1_048_576,
                                    size_mb:                   "1.0".to_string(),
                                    checksums:                 vec![], }];

        let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::seeded_with(item)));
        let items = service.list_items().expect("list items");

        assert_eq!(items[0].format.as_ref(), "EPUB");
    }

    #[test]
    fn map_order_product_joins_multiple_distinct_extensions() {
        let mut item = order_product_item(515_276, "The Wellspring");
        item.attributes.files =
            vec![OrderProductFile { index:                     0,
                                    order_product_download_id: 1234,
                                    title:                     "The Wellspring".to_string(),
                                    filename:                  "the-wellspring.pdf".to_string(),
                                    size:                      1_048_576,
                                    size_mb:                   "1.0".to_string(),
                                    checksums:                 vec![], },
                 OrderProductFile { index:                     1,
                                    order_product_download_id: 1235,
                                    title:                     "The Wellspring".to_string(),
                                    filename:                  "the-wellspring.epub".to_string(),
                                    size:                      1_048_576,
                                    size_mb:                   "1.0".to_string(),
                                    checksums:                 vec![], },];

        let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::seeded_with(item)));
        let items = service.list_items().expect("list items");

        assert_eq!(items[0].format.as_ref(), "EPUB + PDF");
    }

    #[test]
    fn map_order_product_populates_per_item_files_for_multi_item_entries() {
        let mut item = order_product_item(515_276, "Moria");
        item.attributes.files =
            vec![OrderProductFile { index:                     0,
                                    order_product_download_id: 1234,
                                    title:                     "Moria Rulebook".to_string(),
                                    filename:                  "moria-rulebook.pdf".to_string(),
                                    size:                      1_048_576,
                                    size_mb:                   "1.0".to_string(),
                                    checksums:                 vec![], },
                 OrderProductFile { index:                     1,
                                    order_product_download_id: 1235,
                                    title:                     "Moria Map Sheet".to_string(),
                                    filename:                  "moria-map.pdf".to_string(),
                                    size:                      524_288,
                                    size_mb:                   "0.5".to_string(),
                                    checksums:                 vec![], },];

        let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::seeded_with(item)));
        let items = service.list_items().expect("list items");

        assert!(items[0].is_multi_item());
        assert_eq!(items[0].files.len(), 2);
        assert_eq!(items[0].files[0].name.as_ref(), "Moria Rulebook");
        assert_eq!(items[0].files[0].id.as_ref(), "1234");
        assert_eq!(items[0].files[1].name.as_ref(), "Moria Map Sheet");
    }

    #[test]
    fn map_order_product_dedupes_repeated_download_ids() {
        let mut item = order_product_item(515_276, "The Wellspring");
        item.attributes.files =
            vec![OrderProductFile { index:                     0,
                                    order_product_download_id: 1234,
                                    title:                     "The Wellspring".to_string(),
                                    filename:                  "the-wellspring.pdf".to_string(),
                                    size:                      1_048_576,
                                    size_mb:                   "1.0".to_string(),
                                    checksums:                 vec![], },
                 // Same `orderProductDownloadId` as above — the API has been
                 // observed to repeat a download record; the mapper must not
                 // surface it as a second, distinct item.
                 OrderProductFile { index:                     1,
                                    order_product_download_id: 1234,
                                    title:                     "The Wellspring".to_string(),
                                    filename:                  "the-wellspring.pdf".to_string(),
                                    size:                      1_048_576,
                                    size_mb:                   "1.0".to_string(),
                                    checksums:                 vec![], },];

        let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::seeded_with(item)));
        let items = service.list_items().expect("list items");

        assert_eq!(items[0].files.len(), 1);
        assert!(!items[0].is_multi_item());
    }

    #[test]
    fn map_order_product_keeps_distinct_files_that_share_a_download_id() {
        // Regression: `orderProductDownloadId` alone is not a reliable
        // per-file key — the API has been observed to reuse it across
        // genuinely distinct files within a bundle. Deduping on the id alone
        // would collapse this real 2-file bundle down to 1 and hide its
        // item-count badge in the catalog grid/list views.
        let mut item = order_product_item(515_276, "Moria");
        item.attributes.files =
            vec![OrderProductFile { index:                     0,
                                    order_product_download_id: 1234,
                                    title:                     "Moria Rulebook".to_string(),
                                    filename:                  "moria-rulebook.pdf".to_string(),
                                    size:                      1_048_576,
                                    size_mb:                   "1.0".to_string(),
                                    checksums:                 vec![], },
                 OrderProductFile { index:                     1,
                                    order_product_download_id: 1234,
                                    title:                     "Moria Map Sheet".to_string(),
                                    filename:                  "moria-map.pdf".to_string(),
                                    size:                      524_288,
                                    size_mb:                   "0.5".to_string(),
                                    checksums:                 vec![], },];

        let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::seeded_with(item)));
        let items = service.list_items().expect("list items");

        assert_eq!(items[0].files.len(), 2);
        assert!(items[0].is_multi_item());
    }

    #[test]
    fn map_order_product_single_file_is_not_multi_item() {
        let mut item = order_product_item(515_276, "The Wellspring");
        item.attributes.files =
            vec![OrderProductFile { index:                     0,
                                    order_product_download_id: 1234,
                                    title:                     "The Wellspring".to_string(),
                                    filename:                  "the-wellspring.pdf".to_string(),
                                    size:                      1_048_576,
                                    size_mb:                   "1.0".to_string(),
                                    checksums:                 vec![], }];

        let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::seeded_with(item)));
        let items = service.list_items().expect("list items");

        assert!(!items[0].is_multi_item());
        assert_eq!(items[0].files.len(), 1);
    }

    #[test]
    fn map_order_product_builds_cover_url_from_sideloaded_product_relationship() {
        // Matches the live API's actual shape: `product` metadata is *not*
        // embedded on `attributes` — it's referenced via
        // `relationships.product.data.id` and resolved against the
        // response's `included` array.
        let mut item = order_product_item(515_276, "The Wellspring");
        item.attributes.royalty_publisher_id = 4952;
        item.relationships = Some(OrderProductRelationships {
            publisher: Some(RelationshipRef {
                data: Some(RelationshipData {
                    resource_type: "Publisher".to_string(),
                    id: "/api/vBeta/publishers/4952".to_string(),
                }),
            }),
            product: Some(RelationshipRef {
                data: Some(RelationshipData {
                    resource_type: "Product".to_string(),
                    id: "/api/vBeta/products/515276".to_string(),
                }),
            }),
            order: None,
        });

        let mut products = HashMap::new();
        products.insert("/api/vBeta/products/515276".to_string(),
                        OrderProductInfo { image:         Some("4952/515276.jpg".to_string()),
                                           web_image:     Some("4952/515276.webp".to_string()),
                                           thumbnail:
                                               Some("4952/515276-thumb140.jpg".to_string()),
                                           thumbnail_100:
                                               Some("4952/515276-thumb100.jpg".to_string()),
                                           bundle_id:     0,
                                           date_created:
                                               Some("2025-03-13T16:07:01-05:00".to_string()),
                                           product_id:    515_276,
                                           description:   None,
                                           filesize:      Some(24.13), });

        let mut publishers = HashMap::new();
        publishers.insert("/api/vBeta/publishers/4952".to_string(),
                          "Monte Cook Games".to_string());

        let mapped = map_order_product(&item, &publishers, &products, 0);

        assert_eq!(mapped.publisher.as_ref(), "Monte Cook Games");
        assert_eq!(mapped.cover_url.as_deref(),
                   Some("https://api.drivethrurpg.com/images/4952/515276-thumb140.jpg"));
        assert_eq!(mapped.detail_cover_url.as_deref(),
                   Some("https://api.drivethrurpg.com/images/4952/515276.jpg"));
    }

    #[test]
    fn map_order_product_builds_cover_url_from_embedded_thumbnail_fallback() {
        // Defensive fallback path: if `product` were ever embedded directly on
        // `attributes` (e.g. a future/legacy response shape), it should still
        // resolve without a `relationships`/`included` sideload.
        let mut item = order_product_item(515_276, "The Wellspring");
        item.attributes.royalty_publisher_id = 4952;
        item.attributes.publisher =
            Some(dtrpg_sdk::OrderProductPublisher { name:         "Monte Cook Games".to_string(),
                                                    publisher_id: 4952,
                                                    slug:         "monte-cook-games".to_string(), });
        item.attributes.product =
            Some(OrderProductInfo { image:         Some("4952/515276.jpg".to_string()),
                                    web_image:     Some("4952/515276.webp".to_string()),
                                    thumbnail:     Some("4952/515276-thumb140.jpg".to_string()),
                                    thumbnail_100: Some("4952/515276-thumb100.jpg".to_string()),
                                    bundle_id:     0,
                                    date_created:  Some("2025-03-13T16:07:01-05:00".to_string()),
                                    product_id:    515_276,
                                    description:   None,
                                    filesize:      Some(24.13), });

        let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

        assert_eq!(mapped.publisher.as_ref(), "Monte Cook Games");
        assert_eq!(mapped.cover_url.as_deref(),
                   Some("https://api.drivethrurpg.com/images/4952/515276-thumb140.jpg"));
        assert_eq!(mapped.detail_cover_url.as_deref(),
                   Some("https://api.drivethrurpg.com/images/4952/515276.jpg"));
    }

    #[test]
    fn map_order_product_detail_cover_url_prefers_web_image_over_thumbnail() {
        let mut item = order_product_item(515_276, "The Wellspring");
        item.attributes.product =
            Some(OrderProductInfo { image:         None,
                                    web_image:     Some("4952/515276.webp".to_string()),
                                    thumbnail:     Some("4952/515276-thumb140.jpg".to_string()),
                                    thumbnail_100: Some("4952/515276-thumb100.jpg".to_string()),
                                    bundle_id:     0,
                                    date_created:  Some("2025-03-13T16:07:01-05:00".to_string()),
                                    product_id:    515_276,
                                    description:   None,
                                    filesize:      Some(24.13), });

        let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

        assert_eq!(mapped.detail_cover_url.as_deref(),
                   Some("https://api.drivethrurpg.com/images/4952/515276.webp"));
        assert_eq!(mapped.cover_url.as_deref(),
                   Some("https://api.drivethrurpg.com/images/4952/515276-thumb140.jpg"));
    }

    #[test]
    fn map_order_product_detail_cover_url_falls_back_to_thumbnail_only() {
        let mut item = order_product_item(515_276, "The Wellspring");
        item.attributes.product =
            Some(OrderProductInfo { image:         None,
                                    web_image:     None,
                                    thumbnail:     None,
                                    thumbnail_100: Some("4952/515276-thumb100.jpg".to_string()),
                                    bundle_id:     0,
                                    date_created:  Some("2025-03-13T16:07:01-05:00".to_string()),
                                    product_id:    515_276,
                                    description:   None,
                                    filesize:      Some(24.13), });

        let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

        assert_eq!(mapped.detail_cover_url.as_deref(),
                   Some("https://api.drivethrurpg.com/images/4952/515276-thumb100.jpg"));
        assert_eq!(mapped.cover_url.as_deref(),
                   Some("https://api.drivethrurpg.com/images/4952/515276-thumb100.jpg"));
    }

    #[test]
    fn map_order_product_falls_back_to_publisher_lookup_when_not_embedded() {
        let mut item = order_product_item(7, "No Embedded Publisher");
        item.relationships = Some(OrderProductRelationships {
            publisher: Some(RelationshipRef {
                data: Some(RelationshipData {
                    resource_type: "Publisher".to_string(),
                    id: "/api/vBeta/publishers/7".to_string(),
                }),
            }),
            product: None,
            order: None,
        });
        let mut publishers = HashMap::new();
        publishers.insert("/api/vBeta/publishers/7".to_string(),
                          "Lantern Press".to_string());

        let mapped = map_order_product(&item, &publishers, &HashMap::new(), 0);

        assert_eq!(mapped.publisher.as_ref(), "Lantern Press");
        assert!(mapped.cover_url.is_none());
    }

    #[test]
    fn map_order_product_resolves_publisher_by_relationship_id_not_royalty_publisher_id() {
        // Confirmed live against a real account: an ordered product's
        // `royaltyPublisherId` does not always match the id referenced by its
        // own `relationships.publisher` (e.g. a reprint/white-label deal).
        // Matching on `royalty_publisher_id` alone silently resolves to the
        // wrong publisher entry (or none at all) in that case.
        let mut item = order_product_item(9_988_031, "Some Reprinted Title");
        item.attributes.royalty_publisher_id = 526;
        item.relationships = Some(OrderProductRelationships {
            publisher: Some(RelationshipRef {
                data: Some(RelationshipData {
                    resource_type: "Publisher".to_string(),
                    id: "/api/vBeta/publishers/342".to_string(),
                }),
            }),
            product: None,
            order: None,
        });

        let mut publishers = HashMap::new();
        publishers.insert("/api/vBeta/publishers/342".to_string(),
                          "RPGnet".to_string());

        let mapped = map_order_product(&item, &publishers, &HashMap::new(), 0);

        assert_eq!(mapped.publisher.as_ref(), "RPGnet");
    }

    #[test]
    fn map_order_product_falls_back_to_placeholder_when_publisher_unresolvable() {
        let item = order_product_item(7, "No Publisher Anywhere");

        let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

        assert_eq!(mapped.publisher.as_ref(), "Publisher 7");
    }

    #[test]
    fn last_page_from_links_parses_page_number() {
        let links = PaginationLinks {
            self_: "self".to_string(),
            first: None,
            last: Some("https://api.example.com/orders?page=42&pageSize=100".to_string()),
            prev: None,
            next: None,
        };
        assert_eq!(last_page_from_links(&links), Some(42));
    }

    #[test]
    fn last_page_from_links_returns_none_when_absent() {
        let links = pagination_links(None);
        assert_eq!(last_page_from_links(&links), None);
    }

    #[test]
    fn last_page_from_links_returns_none_when_no_page_param() {
        let links =
            PaginationLinks { self_: "self".to_string(),
                              first: None,
                              last:
                                  Some("https://api.example.com/orders?pageSize=100".to_string()),
                              prev:  None,
                              next:  None, };
        assert_eq!(last_page_from_links(&links), None);
    }
}
