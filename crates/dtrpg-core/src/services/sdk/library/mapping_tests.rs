// Mapping behavior tests.

use dtrpg_sdk::{OrderProductOrder, OrderProductRelationships, RelationshipData, RelationshipRef};

use super::super::RustSdkLibraryService;
use super::mapping_test_support::*;
use super::*;
use crate::services::LibraryService;

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
    item.attributes.order = Some(OrderProductOrder { date_created:
                                                         Some("2026-01-03T09:00:00Z".to_string()),
                                                     order_id:     900, });

    let mapped = map_order_product(&item, &HashMap::new(), &HashMap::new(), 0);

    // "2026-01-03T09:00:00Z"
    assert_eq!(mapped.date_added, Some(1_767_430_800));
}

#[test]
fn map_order_product_prefers_date_purchased_over_order_date_created() {
    let mut item = order_product_item(515_276, "The Wellspring");
    item.attributes.order = Some(OrderProductOrder { date_created:
                                                         Some("2020-01-01T00:00:00Z".to_string()),
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
                                       thumbnail:     Some("4952/515276-thumb140.jpg".to_string()),
                                       thumbnail_100: Some("4952/515276-thumb100.jpg".to_string()),
                                       bundle_id:     0,
                                       date_created:  Some("2025-03-13T16:07:01-05:00".to_string()),
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
                          last:  Some("https://api.example.com/orders?pageSize=100".to_string()),
                          prev:  None,
                          next:  None, };
    assert_eq!(last_page_from_links(&links), None);
}
