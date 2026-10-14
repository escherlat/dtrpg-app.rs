//! SDK operation boundary and its concrete implementations: a real
//! HTTP-backed gateway and a stub used when no credentials are available.

use dtrpg_sdk::{
    LibraryClient as SdkLibraryClient, PageParams, ProductListCollectionResponse, ProductListItem,
    ProductListItemsResponse,
};
use dtrpg_ui::services::{
    LoginTokens,
    collections::{CollectionsServiceError, CollectionsServiceErrorKind},
};
use tokio::runtime::Runtime;

use super::super::connection::{self, SdkConnection};
use super::errors::{map_client_error, map_connection_error};
use super::{extract_member_id, extract_product_list_item_id};

/// SDK operation boundary used by the Rust collections service adapter.
pub trait SdkCollectionsGateway: Send + Sync {
    /// Lists the user's DTRPG product lists, paginating through all pages.
    ///
    /// # Errors
    ///
    /// Returns [`CollectionsServiceError`] on network or session failures.
    fn list_product_lists(&self, page: u32)
                          -> Result<ProductListCollectionResponse, CollectionsServiceError>;

    /// Lists the items within a specific product list, paginating through all
    /// pages.
    ///
    /// # Errors
    ///
    /// Returns [`CollectionsServiceError`] on network or session failures.
    fn list_product_list_items(&self, product_list_id: u64, page: u32)
                               -> Result<ProductListItemsResponse, CollectionsServiceError>;

    /// Creates a new product list with the given name and returns it.
    ///
    /// # Errors
    ///
    /// Returns [`CollectionsServiceError`] on network or session failures.
    fn create_product_list(&self, name: &str) -> Result<ProductListItem, CollectionsServiceError>;

    /// Deletes the product list with the given id.
    ///
    /// # Errors
    ///
    /// Returns [`CollectionsServiceError`] on network or session failures.
    fn delete_product_list(&self, id: u64) -> Result<(), CollectionsServiceError>;

    /// Adds a product to a product list as a member.
    ///
    /// `product_id` must be the catalog `product_id`, not an
    /// `order_product_id` — the API rejects the latter with an
    /// invalid-product-id error.
    ///
    /// # Errors
    ///
    /// Returns [`CollectionsServiceError`] on network or session failures.
    fn add_product_list_item(&self, product_list_id: u64, product_id: u64)
                             -> Result<(), CollectionsServiceError>;

    /// Removes a product from a product list's membership.
    ///
    /// # Errors
    ///
    /// Returns [`CollectionsServiceError`] on network or session failures, or
    /// if no matching item can be found in the product list.
    fn remove_product_list_item(&self, product_list_id: u64, order_product_id: u64)
                                -> Result<(), CollectionsServiceError>;
}

pub(super) struct HttpSdkCollectionsGateway {
    client:  SdkLibraryClient,
    runtime: Runtime,
}

impl HttpSdkCollectionsGateway {
    pub(super) fn from_keyring_with_tokens(tokens: LoginTokens)
                                           -> Result<Self, CollectionsServiceError> {
        let SdkConnection { client, runtime } =
            connection::connect_from_keyring_with_tokens(tokens).map_err(map_connection_error)?;
        Ok(Self { client, runtime })
    }

    #[allow(dead_code)]
    pub(super) fn from_environment() -> Result<Self, CollectionsServiceError> {
        let SdkConnection { client, runtime } =
            connection::connect_from_environment().map_err(map_connection_error)?;
        Ok(Self { client, runtime })
    }

    /// Paginates a product list's items looking for one whose
    /// `order_product_id`/`product_id` matches `member_id`, returning that
    /// entry's own `productListItemId`.
    fn find_product_list_item_id(&self, product_list_id: u64, member_id: u64)
                                 -> Result<Option<u64>, CollectionsServiceError> {
        let mut page: u32 = 1;
        loop {
            let response =
                self.runtime
                    .block_on(self.client
                                  .list_product_list_items(product_list_id,
                                                           PageParams { page:      Some(page),
                                                                        page_size: None, }))
                    .map_err(map_client_error)?;

            for item in &response.data {
                if extract_member_id(item) == Some(member_id)
                   && let Some(item_id) = extract_product_list_item_id(item)
                {
                    return Ok(Some(item_id));
                }
            }

            if response.links.next.is_none() {
                return Ok(None);
            }
            page += 1;
        }
    }
}

impl SdkCollectionsGateway for HttpSdkCollectionsGateway {
    fn list_product_lists(&self, page: u32)
                          -> Result<ProductListCollectionResponse, CollectionsServiceError> {
        self.runtime
            .block_on(self.client
                          .list_product_lists(PageParams { page:      Some(page),
                                                           page_size: None, }))
            .map_err(map_client_error)
    }

    fn list_product_list_items(&self, product_list_id: u64, page: u32)
                               -> Result<ProductListItemsResponse, CollectionsServiceError> {
        self.runtime
            .block_on(self.client.list_product_list_items(product_list_id,
                                                          PageParams { page:      Some(page),
                                                                       page_size: None, }))
            .map_err(map_client_error)
    }

    fn create_product_list(&self, name: &str) -> Result<ProductListItem, CollectionsServiceError> {
        self.runtime
            .block_on(self.client.create_product_list(name))
            .map_err(map_client_error)
    }

    fn delete_product_list(&self, id: u64) -> Result<(), CollectionsServiceError> {
        self.runtime
            .block_on(self.client.delete_product_list(id))
            .map_err(map_client_error)
    }

    fn add_product_list_item(&self, product_list_id: u64, product_id: u64)
                             -> Result<(), CollectionsServiceError> {
        self.runtime
            .block_on(self.client
                          .add_product_list_item(product_list_id, product_id))
            .map_err(map_client_error)
            .map(|_| ())
    }

    fn remove_product_list_item(&self, product_list_id: u64, order_product_id: u64)
                                -> Result<(), CollectionsServiceError> {
        // `DELETE /product_list_items/{id}` takes the item's own id, not the
        // product's id, so the matching list entry must be located first.
        let product_list_item_id = self.find_product_list_item_id(product_list_id,
                                                                  order_product_id)?
                                       .ok_or_else(|| {
                                           CollectionsServiceError::new(
                    CollectionsServiceErrorKind::Network,
                    format!(
                        "Item {order_product_id} was not found in collection {product_list_id}."
                    ),
                )
                                       })?;

        self.runtime
            .block_on(self.client.delete_product_list_item(product_list_item_id))
            .map_err(map_client_error)
    }
}

/// A gateway that returns a stored error on every call.
///
/// Used when SDK initialization fails (missing API key, invalid credentials)
/// so the app starts without crashing. The library window still opens; the
/// Collections section simply remains absent from the sidebar.
pub(super) struct UnavailableCollectionsGateway {
    error: CollectionsServiceError,
}

impl UnavailableCollectionsGateway {
    pub(super) fn new(error: CollectionsServiceError) -> Self {
        Self { error }
    }
}

impl SdkCollectionsGateway for UnavailableCollectionsGateway {
    fn list_product_lists(&self, _page: u32)
                          -> Result<ProductListCollectionResponse, CollectionsServiceError> {
        Err(self.error.clone())
    }

    fn list_product_list_items(&self, _product_list_id: u64, _page: u32)
                               -> Result<ProductListItemsResponse, CollectionsServiceError> {
        Err(self.error.clone())
    }

    fn create_product_list(&self, _name: &str) -> Result<ProductListItem, CollectionsServiceError> {
        Err(self.error.clone())
    }

    fn delete_product_list(&self, _id: u64) -> Result<(), CollectionsServiceError> {
        Err(self.error.clone())
    }

    fn add_product_list_item(&self, _product_list_id: u64, _product_id: u64)
                             -> Result<(), CollectionsServiceError> {
        Err(self.error.clone())
    }

    fn remove_product_list_item(&self, _product_list_id: u64, _order_product_id: u64)
                                -> Result<(), CollectionsServiceError> {
        Err(self.error.clone())
    }
}
