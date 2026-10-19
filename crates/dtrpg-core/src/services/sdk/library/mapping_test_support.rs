// Test fixtures shared by the mapping behavior tests.

use dtrpg_sdk::{
    FileChecksum, OrderProductAttributes, OrderProductFile, OrderProductItemResponse,
    OrderProductListResponse, PaginationLinks, PaginationMeta,
};

use super::super::gateway::SdkLibraryGateway;
use super::*;
use crate::services::LibraryServiceError;

pub(super) struct FakeSdkGateway {
    list_result:   Result<OrderProductListResponse, LibraryServiceError>,
    detail_result: Result<OrderProductItemResponse, LibraryServiceError>,
}

impl FakeSdkGateway {
    pub(super) fn seeded_with(item: OrderProductItem) -> Self {
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
                                                            included: None, }), }
    }
}

impl SdkLibraryGateway for FakeSdkGateway {
    fn list_order_products(&self, _params: dtrpg_sdk::LibraryItemsParams)
                           -> Result<OrderProductListResponse, LibraryServiceError> {
        self.list_result.clone()
    }

    fn get_order_product(&self, _id: u64) -> Result<OrderProductItemResponse, LibraryServiceError> {
        self.detail_result.clone()
    }

    fn prepare_download(&self, _order_product_id: u64, _index: u32)
                        -> Result<serde_json::Value, LibraryServiceError> {
        Err(LibraryServiceError::new(crate::services::LibraryServiceErrorKind::NotFound,
                                     "not used"))
    }
}

pub(super) fn order_product_item(id: u64, name: &str) -> OrderProductItem {
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

pub(super) fn pagination_links(next: Option<String>) -> PaginationLinks {
    PaginationLinks { self_: "self".to_string(),
                      first: None,
                      last: None,
                      prev: None,
                      next }
}
