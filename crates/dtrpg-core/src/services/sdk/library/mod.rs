//! Rust SDK-backed implementation of [`LibraryService`].

mod download;
mod errors;
mod gateway;
mod mapping;

use dtrpg_sdk::{IncludedItem, LibraryItemsParams};
use dtrpg_ui::{
    data::library::LibraryItem,
    services::{LibraryService, LibraryServiceError, LibraryServiceErrorKind},
};

pub use self::gateway::SdkLibraryGateway;
use self::gateway::{HttpSdkLibraryGateway, UnavailableSdkGateway};
use self::mapping::{last_page_from_links, map_order_product, product_lookup, publisher_lookup};

/// Library service adapter backed by the Rust SDK.
pub struct RustSdkLibraryService {
    gateway: Box<dyn SdkLibraryGateway>,
}

impl RustSdkLibraryService {
    /// Creates a service from an SDK gateway implementation.
    pub fn new(gateway: Box<dyn SdkLibraryGateway>) -> Self {
        Self { gateway }
    }

    /// Creates the default service from environment-provided SDK configuration.
    ///
    /// Falls back to [`UnavailableSdkGateway`] when environment variables are
    /// absent.
    #[allow(dead_code)]
    pub fn from_environment() -> Self {
        match HttpSdkLibraryGateway::from_environment() {
            Ok(gateway) => Self::new(Box::new(gateway)),
            Err(error) => Self::new(Box::new(UnavailableSdkGateway::new(error))),
        }
    }

    /// Creates an unauthenticated service that returns a "not signed in" error
    /// on all calls.
    pub fn unauthenticated() -> Self {
        Self::new(Box::new(UnavailableSdkGateway::new(
            LibraryServiceError::new(
                LibraryServiceErrorKind::Session,
                "Not signed in. Open Settings > Account to sign in.",
            ),
        )))
    }

    /// Creates the service using in-memory `tokens` obtained at startup or
    /// login.
    ///
    /// Reads only the API key from the platform keyring; tokens are never
    /// persisted to the keychain. Falls back to [`UnavailableSdkGateway`]
    /// when the API key cannot be found.
    pub fn from_keyring_with_tokens(tokens: dtrpg_ui::services::LoginTokens) -> Self {
        match HttpSdkLibraryGateway::from_keyring_with_tokens(tokens) {
            Ok(gateway) => Self::new(Box::new(gateway)),
            Err(error) => Self::new(Box::new(UnavailableSdkGateway::new(error))),
        }
    }
}

impl LibraryService for RustSdkLibraryService {
    fn list_items(&self) -> Result<Vec<LibraryItem>, LibraryServiceError> {
        let mut all_items: Vec<LibraryItem> = Vec::new();
        self.list_items_paged(&mut |page_items| all_items.extend(page_items), None)?;
        Ok(all_items)
    }

    fn list_items_paged(&self, on_page: &mut dyn FnMut(Vec<LibraryItem>),
                        mut on_total: Option<&mut dyn FnMut(usize)>)
                        -> Result<(), LibraryServiceError> {
        let mut all_included: Vec<IncludedItem> = Vec::new();
        let mut page: u32 = 1;
        let mut global_index: u32 = 0;
        let mut total_reported = false;
        let page_size: u32 = 100;

        loop {
            let params = LibraryItemsParams { page:               Some(page),
                                              page_size:          Some(page_size),
                                              get_checksum:       Some(false),
                                              get_filters:        Some(true),
                                              library:            Some(true),
                                              archived:           Some(false),
                                              updated_date_after: None, };

            let response = self.gateway.list_order_products(params)?;

            // Derive estimated total from `links.last` on the first page
            // response.
            if !total_reported {
                if let (Some(cb), Some(last_page)) =
                    (on_total.as_deref_mut(), last_page_from_links(&response.links))
                {
                    let estimated = (last_page as usize).saturating_mul(page_size as usize);
                    cb(estimated);
                }
                total_reported = true;
            }

            // `Product` resources are only needed to resolve this page's items
            // — rebuilt fresh per page rather than accumulated,
            // since each ordered product's relationship points at
            // its own distinct `Product` resource.
            let products = response.included
                                   .as_deref()
                                   .map(product_lookup)
                                   .unwrap_or_default();

            if let Some(included) = &response.included {
                for entry in included {
                    if entry.resource_type == "Publisher"
                       && !all_included.iter().any(|p| p.id == entry.id)
                    {
                        all_included.push(entry.clone());
                    }
                }
            }

            let has_next = response.links.next.is_some();
            let publishers = publisher_lookup(&all_included);
            let page_items: Vec<LibraryItem> =
                response.data
                        .iter()
                        .enumerate()
                        .map(|(i, item)| {
                            map_order_product(item, &publishers, &products, global_index + i as u32)
                        })
                        .collect();

            global_index += page_items.len() as u32;
            on_page(page_items);

            if !has_next {
                break;
            }
            page += 1;
        }

        Ok(())
    }

    fn get_item(&self, id: u64) -> Result<LibraryItem, LibraryServiceError> {
        let response = self.gateway.get_order_product(id)?;
        // The detail endpoint sideloads the same Publisher/Product `included`
        // array as the list endpoint; resolve it the same way rather
        // than falling back to `attributes.product` embedding alone.
        let products = response.included
                               .as_deref()
                               .map(product_lookup)
                               .unwrap_or_default();
        let publishers = response.included
                                 .as_deref()
                                 .map(publisher_lookup)
                                 .unwrap_or_default();

        Ok(map_order_product(&response.data, &publishers, &products, 0))
    }

    fn count_items(&self) -> Option<Result<usize, LibraryServiceError>> {
        // Request a single item per page: with `pageSize=1`, the last page
        // number reported in `links.last` is numerically equal to the
        // total item count, so this is a cheap way to detect remote
        // changes without fetching all pages. Falls back to the single
        // returned page's length when there is no `last` link (i.e. the
        // whole library fits on one page of size 1: 0 or 1 items).
        let params = LibraryItemsParams { page:               Some(1),
                                          page_size:          Some(1),
                                          get_checksum:       Some(false),
                                          get_filters:        Some(false),
                                          library:            Some(true),
                                          archived:           Some(false),
                                          updated_date_after: None, };

        Some(self.gateway.list_order_products(params).map(|response| {
                                                         last_page_from_links(&response.links)
                .map(|last_page| last_page as usize)
                .unwrap_or(response.data.len())
                                                     }))
    }

    fn list_items_updated_since(&self, since_iso8601: &str,
                                on_page: &mut dyn FnMut(Vec<LibraryItem>))
                                -> Option<Result<(), LibraryServiceError>> {
        Some((|| {
                 let mut all_included: Vec<IncludedItem> = Vec::new();
                 let mut page: u32 = 1;
                 let mut global_index: u32 = 0;
                 let page_size: u32 = 100;

                 loop {
                     let params = LibraryItemsParams { page:               Some(page),
                                                       page_size:          Some(page_size),
                                                       get_checksum:       Some(false),
                                                       get_filters:        Some(true),
                                                       library:            Some(true),
                                                       archived:           Some(false),
                                                       updated_date_after:
                                                           Some(since_iso8601.to_string()), };

                     let response = self.gateway.list_order_products(params)?;

                     let products = response.included
                                            .as_deref()
                                            .map(product_lookup)
                                            .unwrap_or_default();

                     if let Some(included) = &response.included {
                         for entry in included {
                             if entry.resource_type == "Publisher"
                                && !all_included.iter().any(|p| p.id == entry.id)
                             {
                                 all_included.push(entry.clone());
                             }
                         }
                     }

                     let has_next = response.links.next.is_some();
                     let publishers = publisher_lookup(&all_included);
                     let page_items: Vec<LibraryItem> = response.data
                                                                .iter()
                                                                .enumerate()
                                                                .map(|(i, item)| {
                                                                    map_order_product(item,
                                                                                      &publishers,
                                                                                      &products,
                                                                                      global_index
                                                                                      + i as u32)
                                                                })
                                                                .collect();

                     global_index += page_items.len() as u32;
                     on_page(page_items);

                     if !has_next {
                         break;
                     }
                     page += 1;
                 }

                 Ok(())
             })())
    }

    fn download_item(&self, order_product_id: u64, index: u32, dest: &std::path::Path,
                     cancel: &std::sync::atomic::AtomicBool,
                     on_retry: Option<&mut dyn FnMut(u32, std::time::Duration)>)
                     -> Result<(), LibraryServiceError> {
        download::download_item(self.gateway.as_ref(),
                                order_product_id,
                                index,
                                dest,
                                cancel,
                                on_retry)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use dtrpg_sdk::{
        FileChecksum, OrderProductAttributes, OrderProductFile, OrderProductItem,
        OrderProductItemResponse, OrderProductListResponse, OrderProductRelationships,
        PaginationLinks, PaginationMeta, RelationshipData, RelationshipRef,
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

        fn get_order_product(&self, _id: u64)
                             -> Result<OrderProductItemResponse, LibraryServiceError> {
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
                Ok(OrderProductListResponse { links:
                                                  pagination_links(Some("page=2".to_string())),
                                              meta:     PaginationMeta { items_per_page: 1,
                                                                         current_page:   1, },
                                              data:     vec![order_product_item(42,
                                                                                "Item Page One")],
                                              included: None, })
            }
            else {
                Ok(OrderProductListResponse { links:    pagination_links(None),
                                              meta:     PaginationMeta { items_per_page: 1,
                                                                         current_page:   2, },
                                              data:     vec![order_product_item(99,
                                                                                "Item Page Two")],
                                              included: None, })
            }
        }

        fn get_order_product(&self, _id: u64)
                             -> Result<OrderProductItemResponse, LibraryServiceError> {
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
                last: Some(
                    "https://api.example.com/order_products?pageSize=1&page=137".to_string(),
                ),
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
}
