// Tests for this module.

use std::sync::atomic::{AtomicU32, Ordering};

use dtrpg_sdk::{
    FileChecksum, OrderProductAttributes, OrderProductFile, OrderProductItem,
    OrderProductItemResponse, OrderProductListResponse, OrderProductRelationships, PaginationLinks,
    PaginationMeta, RelationshipData, RelationshipRef,
};

use super::*;

struct FakeSdkGateway {
    list_result:   Result<OrderProductListResponse, LibraryServiceError>,
    detail_result: Result<OrderProductItemResponse, LibraryServiceError>,
}

impl FakeSdkGateway {
    fn seeded() -> Self {
        let mut item = order_product_item(42, "A Better Dungeon");
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
        Self { list_result:   Ok(OrderProductListResponse { links:    pagination_links(None),
                                                            meta:
                                                                PaginationMeta { items_per_page:
                                                                                     100,
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
                                                            included: Some(vec![
            IncludedItem { id:            "/api/vBeta/publishers/7".to_string(),
                           resource_type: "Publisher".to_string(),
                           attributes:    serde_json::json!({
                               "name": "Lantern Press",
                               "publisherId": 7,
                               "slug": "lantern-press",
                           }), },
        ]), }), }
    }

    fn session_error() -> Self {
        let error = LibraryServiceError::new(LibraryServiceErrorKind::Session,
                                             "SDK session expired in test.");
        Self { list_result:   Err(error.clone()),
               detail_result: Err(error), }
    }
}

impl SdkLibraryGateway for FakeSdkGateway {
    fn list_order_products(&self, _params: LibraryItemsParams)
                           -> Result<OrderProductListResponse, LibraryServiceError> {
        self.list_result.clone()
    }

    fn get_order_product(&self, _id: u64) -> Result<OrderProductItemResponse, LibraryServiceError> {
        self.detail_result.clone()
    }

    fn prepare_download(&self, _order_product_id: u64, _index: u32)
                        -> Result<serde_json::Value, LibraryServiceError> {
        Err(LibraryServiceError::new(LibraryServiceErrorKind::NotFound, "not used"))
    }
}

/// Returns pages in order: first call gets page 1 with a `next` link,
/// second call gets page 2 with no `next` link.
struct TwoPageGateway {
    call_count: AtomicU32,
}

impl SdkLibraryGateway for TwoPageGateway {
    fn list_order_products(&self, _params: LibraryItemsParams)
                           -> Result<OrderProductListResponse, LibraryServiceError> {
        let call = self.call_count.fetch_add(1, Ordering::Relaxed);

        if call == 0 {
            Ok(OrderProductListResponse { links:    pagination_links(Some("page=2".to_string())),
                                          meta:     PaginationMeta { items_per_page: 1,
                                                                     current_page:   1, },
                                          data:     vec![order_product_item(42, "Item Page One")],
                                          included: None, })
        }
        else {
            Ok(OrderProductListResponse { links:    pagination_links(None),
                                          meta:     PaginationMeta { items_per_page: 1,
                                                                     current_page:   2, },
                                          data:     vec![order_product_item(99, "Item Page Two")],
                                          included: None, })
        }
    }

    fn get_order_product(&self, _id: u64) -> Result<OrderProductItemResponse, LibraryServiceError> {
        Err(LibraryServiceError::new(LibraryServiceErrorKind::NotFound, "not used"))
    }

    fn prepare_download(&self, _order_product_id: u64, _index: u32)
                        -> Result<serde_json::Value, LibraryServiceError> {
        Err(LibraryServiceError::new(LibraryServiceErrorKind::NotFound, "not used"))
    }
}

#[test]
fn sdk_service_maps_order_products_to_library_items() {
    let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::seeded()));

    let items = service.list_items().expect("list items");

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].numeric_id, 42);
    assert_eq!(items[0].title.as_ref(), "A Better Dungeon");
    assert_eq!(items[0].publisher.as_ref(), "Lantern Press");
    assert_eq!(items[0].kind.as_ref(), "Adventure");
}

#[test]
fn get_item_resolves_publisher_from_included() {
    let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::seeded()));

    let item = service.get_item(42).expect("get item");

    assert_eq!(item.publisher.as_ref(), "Lantern Press");
}

#[test]
fn count_items_uses_last_page_number_when_link_present() {
    let mut gateway = FakeSdkGateway::seeded();
    gateway.list_result = Ok(OrderProductListResponse {
        links: PaginationLinks {
            self_: "self".to_string(),
            first: None,
            last: Some("https://api.example.com/order_products?pageSize=1&page=137".to_string()),
            prev: None,
            next: None,
        },
        meta: PaginationMeta {
            items_per_page: 1,
            current_page: 1,
        },
        data: vec![order_product_item(1, "First Item")],
        included: None,
    });
    let service = RustSdkLibraryService::new(Box::new(gateway));

    let count = service.count_items().expect("count supported");

    assert_eq!(count.expect("count succeeds"), 137);
}

#[test]
fn count_items_falls_back_to_page_length_without_last_link() {
    let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::seeded()));

    let count = service.count_items().expect("count supported");

    assert_eq!(count.expect("count succeeds"), 1);
}

#[test]
fn count_items_propagates_gateway_error() {
    let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::session_error()));

    let count = service.count_items().expect("count supported");

    assert_eq!(count.expect_err("session error propagated").kind,
               LibraryServiceErrorKind::Session);
}

#[test]
fn sdk_service_preserves_session_error_classification() {
    let service = RustSdkLibraryService::new(Box::new(FakeSdkGateway::session_error()));

    let error = service.list_items().expect_err("session error");

    assert_eq!(error.kind, LibraryServiceErrorKind::Session);
}

#[test]
fn sdk_service_fetches_all_pages_via_pagination() {
    let gateway = TwoPageGateway { call_count: AtomicU32::new(0), };
    let service = RustSdkLibraryService::new(Box::new(gateway));

    let items = service.list_items().expect("list items");

    assert_eq!(items.len(), 2, "should have items from both pages");
    assert!(items.iter().any(|i| i.numeric_id == 42),
            "page 1 item should be present");
    assert!(items.iter().any(|i| i.numeric_id == 99),
            "page 2 item should be present");
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
