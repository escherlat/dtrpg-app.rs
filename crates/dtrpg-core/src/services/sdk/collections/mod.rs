//! Rust SDK-backed implementation of [`CollectionsService`].

mod errors;
mod gateway;

use std::sync::Arc;

use dtrpg_ui::{
    data::collection::CollectionEntry,
    services::{
        LoginTokens,
        collections::{CollectionsService, CollectionsServiceError, CollectionsServiceErrorKind},
    },
    util::datetime::parse_rfc3339_to_epoch,
};

pub use self::gateway::SdkCollectionsGateway;
use self::gateway::{HttpSdkCollectionsGateway, UnavailableCollectionsGateway};

/// Collections service adapter backed by the Rust SDK.
pub struct RustSdkCollectionsService {
    gateway: Box<dyn SdkCollectionsGateway>,
}

impl RustSdkCollectionsService {
    /// Creates a service from an SDK gateway implementation.
    pub fn new(gateway: Box<dyn SdkCollectionsGateway>) -> Self {
        Self { gateway }
    }

    /// Creates an unauthenticated service that returns a session error on all
    /// calls.
    pub fn unauthenticated() -> Self {
        Self::new(Box::new(UnavailableCollectionsGateway::new(
            CollectionsServiceError::new(
                CollectionsServiceErrorKind::Session,
                "Not signed in. Open Settings > Account to sign in.",
            ),
        )))
    }

    /// Creates the service using in-memory `tokens` obtained at startup or
    /// login.
    ///
    /// Reads only the API key from the platform keyring. Falls back to
    /// [`UnavailableCollectionsGateway`] when the API key cannot be found.
    pub fn from_keyring_with_tokens(tokens: LoginTokens) -> Self {
        match HttpSdkCollectionsGateway::from_keyring_with_tokens(tokens) {
            Ok(gateway) => Self::new(Box::new(gateway)),
            Err(error) => Self::new(Box::new(UnavailableCollectionsGateway::new(error))),
        }
    }
}

impl CollectionsService for RustSdkCollectionsService {
    fn list_collections(&self) -> Result<Vec<CollectionEntry>, CollectionsServiceError> {
        // Fetch all product list pages.
        let mut all_lists = Vec::new();
        let mut page: u32 = 1;
        loop {
            let response = self.gateway.list_product_lists(page)?;
            all_lists.extend(response.data);
            if response.links.next.is_none() {
                break;
            }
            page += 1;
        }

        // For each list, fetch all member IDs.
        let mut entries = Vec::with_capacity(all_lists.len());
        for list in all_lists {
            let id = list.attributes.product_list_id;
            let name: Arc<str> = list.attributes.name.as_str().into();
            let created_at = parse_rfc3339_to_epoch(&list.attributes.date_created).unwrap_or(0);

            let mut member_ids: Vec<u64> = Vec::new();
            let mut items_page: u32 = 1;
            loop {
                let items_resp = match self.gateway.list_product_list_items(id, items_page) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(
                            collection_id = id,
                            error = %e,
                            "failed to fetch collection items, skipping"
                        );
                        break;
                    }
                };

                for item in &items_resp.data {
                    if let Some(id_value) = extract_member_id(item) {
                        member_ids.push(id_value);
                    }
                    else {
                        tracing::warn!(
                            collection_id = id,
                            item = ?item,
                            "skipping list item: no orderProductId or productId found"
                        );
                    }
                }

                if items_resp.links.next.is_none() {
                    break;
                }
                items_page += 1;
            }

            entries.push(CollectionEntry { id,
                                           name,
                                           member_ids: Arc::from(member_ids.as_slice()),
                                           created_at });
        }

        Ok(entries)
    }

    fn create_collection(&self, name: &str) -> Result<CollectionEntry, CollectionsServiceError> {
        let item = self.gateway.create_product_list(name.trim())?;
        let created_at = parse_rfc3339_to_epoch(&item.attributes.date_created).unwrap_or(0);
        Ok(CollectionEntry { id: item.attributes.product_list_id,
                             name: Arc::from(item.attributes.name.as_str()),
                             member_ids: Arc::from(&[][..]),
                             created_at })
    }

    fn delete_collection(&self, id: u64) -> Result<(), CollectionsServiceError> {
        self.gateway.delete_product_list(id)
    }

    fn add_member(&self, collection_id: u64, item_id: u64) -> Result<(), CollectionsServiceError> {
        self.gateway.add_product_list_item(collection_id, item_id)
    }

    fn remove_member(&self, collection_id: u64, item_id: u64)
                     -> Result<(), CollectionsServiceError> {
        self.gateway
            .remove_product_list_item(collection_id, item_id)
    }
}

/// Extracts the id used for collection membership matching from a raw
/// `product_list_items` entry: `orderProductId` first (flat or JSON:API-style
/// `attributes`), falling back to `attributes.productId`.
fn extract_member_id(item: &serde_json::Value) -> Option<u64> {
    item.get("orderProductId")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            item.get("attributes")
                .and_then(|a| a.get("orderProductId"))
                .and_then(serde_json::Value::as_u64)
        })
        .or_else(|| {
            item.get("attributes")
                .and_then(|a| a.get("productId"))
                .and_then(serde_json::Value::as_u64)
        })
}

/// Extracts a product list item's own id (`productListItemId`, needed to
/// delete it via `DELETE /product_list_items/{id}`) from a raw
/// `product_list_items` entry.
///
/// Tries `productListItemId` (flat or nested in `attributes`, matching the
/// `POST /product_list_items` response shape) first, then falls back to the
/// JSON:API resource `id`, which the API may send as either a number or a
/// numeric string.
fn extract_product_list_item_id(item: &serde_json::Value) -> Option<u64> {
    item.get("productListItemId")
        .and_then(serde_json::Value::as_u64)
        .or_else(|| {
            item.get("attributes")
                .and_then(|a| a.get("productListItemId"))
                .and_then(serde_json::Value::as_u64)
        })
        .or_else(|| {
            item.get("id").and_then(|v| {
                              v.as_u64()
                               .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
                          })
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use dtrpg_sdk::{
        PaginationLinks, PaginationMeta, ProductListAttributes, ProductListCollectionResponse,
        ProductListItem, ProductListItemsResponse,
    };
    use serde_json::json;

    use super::*;

    fn pagination_links(next: Option<&str>) -> PaginationLinks {
        PaginationLinks { self_: "self".to_string(),
                          first: None,
                          last:  None,
                          prev:  None,
                          next:  next.map(String::from), }
    }

    fn product_list_item(id: u64, name: &str) -> ProductListItem {
        ProductListItem { id:            id.to_string(),
                          resource_type: "product_list".to_string(),
                          attributes:    ProductListAttributes { customer_id:     100,
                                                                 name:            name.to_string(),
                                                                 date_created:
                                                                     "2026-01-01".to_string(),
                                                                 product_list_id: id,
                                                                 slug:
                                                                     name.to_lowercase()
                                                                         .replace(' ', "-"),
                                                                 item_count:      2, }, }
    }

    struct FakeCollectionsGateway {
        lists:         Result<Vec<ProductListItem>, CollectionsServiceError>,
        items:         Result<Vec<serde_json::Value>, CollectionsServiceError>,
        create_result: Result<ProductListItem, CollectionsServiceError>,
        add_result:    Result<(), CollectionsServiceError>,
        remove_result: Result<(), CollectionsServiceError>,
    }

    impl FakeCollectionsGateway {
        fn seeded() -> Self {
            Self { lists:         Ok(vec![product_list_item(7, "Favorites")]),
                   items:         Ok(vec![json!({ "orderProductId": 42 }),
                                          json!({ "orderProductId": 99 }),]),
                   create_result: Ok(product_list_item(8, "New List")),
                   add_result:    Ok(()),
                   remove_result: Ok(()), }
        }

        fn session_error() -> Self {
            let err = CollectionsServiceError::new(CollectionsServiceErrorKind::Session,
                                                   "fake session error");
            Self { lists:         Err(err.clone()),
                   items:         Err(err.clone()),
                   create_result: Err(err.clone()),
                   add_result:    Err(err.clone()),
                   remove_result: Err(err), }
        }

        fn items_with_missing_field() -> Self {
            Self { lists:         Ok(vec![product_list_item(7, "Favorites")]),
                   items:         Ok(vec![json!({ "orderProductId": 42 }),
                                          json!({ "someOtherField": "abc" }),
                                          json!({ "orderProductId": 99 }),]),
                   create_result: Ok(product_list_item(8, "New List")),
                   add_result:    Ok(()),
                   remove_result: Ok(()), }
        }
    }

    impl SdkCollectionsGateway for FakeCollectionsGateway {
        fn list_product_lists(&self, _page: u32)
                              -> Result<ProductListCollectionResponse, CollectionsServiceError>
        {
            self.lists.clone().map(|data| {
                                  ProductListCollectionResponse {
                    links: pagination_links(None),
                    meta: PaginationMeta {
                        items_per_page: 100,
                        current_page: 1,
                    },
                    data,
                }
                              })
        }

        fn list_product_list_items(&self, _product_list_id: u64, _page: u32)
                                   -> Result<ProductListItemsResponse, CollectionsServiceError>
        {
            self.items.clone().map(|data| ProductListItemsResponse {
                links: pagination_links(None),
                meta: PaginationMeta {
                    items_per_page: 100,
                    current_page: 1,
                },
                data,
            })
        }

        fn create_product_list(&self, _name: &str)
                               -> Result<ProductListItem, CollectionsServiceError> {
            self.create_result.clone()
        }

        fn delete_product_list(&self, _id: u64) -> Result<(), CollectionsServiceError> {
            self.lists.as_ref().map(|_| ()).map_err(Clone::clone)
        }

        fn add_product_list_item(&self, _product_list_id: u64, _product_id: u64)
                                 -> Result<(), CollectionsServiceError> {
            self.add_result.clone()
        }

        fn remove_product_list_item(&self, _product_list_id: u64, _order_product_id: u64)
                                    -> Result<(), CollectionsServiceError> {
            self.remove_result.clone()
        }
    }

    #[test]
    fn seeded_data_produces_correct_collection_entries() {
        let service = RustSdkCollectionsService::new(Box::new(FakeCollectionsGateway::seeded()));
        let collections = service.list_collections().expect("list collections");

        assert_eq!(collections.len(), 1);
        let entry = &collections[0];
        assert_eq!(entry.id, 7);
        assert_eq!(entry.name.as_ref(), "Favorites");
        assert_eq!(entry.member_ids.as_ref(), &[42u64, 99u64]);
    }

    #[test]
    fn session_error_propagates() {
        let service =
            RustSdkCollectionsService::new(Box::new(FakeCollectionsGateway::session_error()));
        let err = service.list_collections().expect_err("session error");
        assert_eq!(err.kind, CollectionsServiceErrorKind::Session);
    }

    #[test]
    fn items_with_missing_order_product_id_are_skipped() {
        let service = RustSdkCollectionsService::new(Box::new(
            FakeCollectionsGateway::items_with_missing_field(),
        ));
        let collections = service.list_collections().expect("list collections");
        assert_eq!(collections.len(), 1);
        let member_ids = &collections[0].member_ids;
        assert_eq!(member_ids.as_ref(), &[42u64, 99u64]);
    }

    #[test]
    fn create_collection_returns_correct_entry() {
        let service = RustSdkCollectionsService::new(Box::new(FakeCollectionsGateway::seeded()));
        let entry = service.create_collection("New List")
                           .expect("create collection");

        assert_eq!(entry.id, 8);
        assert_eq!(entry.name.as_ref(), "New List");
        assert!(entry.member_ids.is_empty());
    }

    #[test]
    fn create_collection_propagates_session_error() {
        let service =
            RustSdkCollectionsService::new(Box::new(FakeCollectionsGateway::session_error()));
        let err = service.create_collection("Anything")
                         .expect_err("session error");
        assert_eq!(err.kind, CollectionsServiceErrorKind::Session);
    }

    #[test]
    fn extract_member_id_reads_flat_order_product_id() {
        let item = json!({ "orderProductId": 42 });
        assert_eq!(extract_member_id(&item), Some(42));
    }

    #[test]
    fn extract_member_id_reads_nested_order_product_id() {
        let item = json!({ "attributes": { "orderProductId": 42 } });
        assert_eq!(extract_member_id(&item), Some(42));
    }

    #[test]
    fn extract_member_id_falls_back_to_nested_product_id() {
        let item = json!({ "attributes": { "productId": 99 } });
        assert_eq!(extract_member_id(&item), Some(99));
    }

    #[test]
    fn extract_member_id_returns_none_when_no_field_matches() {
        let item = json!({ "someOtherField": "abc" });
        assert_eq!(extract_member_id(&item), None);
    }

    #[test]
    fn extract_product_list_item_id_reads_flat_field() {
        let item = json!({ "productListItemId": 2_629_321 });
        assert_eq!(extract_product_list_item_id(&item), Some(2_629_321));
    }

    #[test]
    fn extract_product_list_item_id_reads_nested_field() {
        let item = json!({ "attributes": { "productListItemId": 2_629_321 } });
        assert_eq!(extract_product_list_item_id(&item), Some(2_629_321));
    }

    #[test]
    fn extract_product_list_item_id_falls_back_to_numeric_resource_id() {
        let item = json!({ "id": 2_629_321 });
        assert_eq!(extract_product_list_item_id(&item), Some(2_629_321));
    }

    #[test]
    fn extract_product_list_item_id_falls_back_to_string_resource_id() {
        let item = json!({ "id": "2629321" });
        assert_eq!(extract_product_list_item_id(&item), Some(2_629_321));
    }

    #[test]
    fn extract_product_list_item_id_returns_none_when_no_field_matches() {
        let item = json!({ "someOtherField": "abc" });
        assert_eq!(extract_product_list_item_id(&item), None);
    }
}
