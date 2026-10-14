//! SDK operation boundary and its concrete implementations: a real
//! HTTP-backed gateway and a stub used when no credentials are available.

use dtrpg_sdk::{
    LibraryClient as SdkLibraryClient, LibraryItemsParams, OrderProductItemResponse,
    OrderProductListResponse,
};
use dtrpg_ui::services::LibraryServiceError;
use tokio::runtime::Runtime;

use super::super::connection::{self, SdkConnection};
use super::errors::{map_client_error, map_connection_error};

/// SDK operation boundary used by the Rust library service adapter.
pub trait SdkLibraryGateway: Send + Sync {
    /// Lists ordered products with optional pagination params.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] on network or session failures.
    fn list_order_products(&self, params: LibraryItemsParams)
                           -> Result<OrderProductListResponse, LibraryServiceError>;

    /// Loads an ordered product detail by its numeric id.
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] on network, session, or not-found
    /// failures.
    fn get_order_product(&self, id: u64) -> Result<OrderProductItemResponse, LibraryServiceError>;

    /// Prepares a download for the given ordered product's file, returning
    /// the raw API response (see [`SdkLibraryClient::prepare_download`]).
    ///
    /// # Errors
    ///
    /// Returns [`LibraryServiceError`] on network or session failures.
    fn prepare_download(&self, order_product_id: u64, index: u32)
                        -> Result<serde_json::Value, LibraryServiceError>;
}

pub(super) struct HttpSdkLibraryGateway {
    client:  SdkLibraryClient,
    runtime: Runtime,
}

impl HttpSdkLibraryGateway {
    /// Reads only the API key from the keyring and uses the provided in-memory
    /// tokens.
    pub(super) fn from_keyring_with_tokens(tokens: dtrpg_ui::services::LoginTokens)
                                           -> Result<Self, LibraryServiceError> {
        let SdkConnection { client, runtime } =
            connection::connect_from_keyring_with_tokens(tokens).map_err(map_connection_error)?;
        Ok(Self { client, runtime })
    }

    #[allow(dead_code)]
    pub(super) fn from_environment() -> Result<Self, LibraryServiceError> {
        let SdkConnection { client, runtime } =
            connection::connect_from_environment().map_err(map_connection_error)?;
        Ok(Self { client, runtime })
    }
}

impl SdkLibraryGateway for HttpSdkLibraryGateway {
    fn list_order_products(&self, params: LibraryItemsParams)
                           -> Result<OrderProductListResponse, LibraryServiceError> {
        self.runtime
            .block_on(self.client.list_order_products(params))
            .map_err(map_client_error)
    }

    fn get_order_product(&self, id: u64) -> Result<OrderProductItemResponse, LibraryServiceError> {
        self.runtime
            .block_on(self.client.get_order_product(id))
            .map_err(map_client_error)
    }

    fn prepare_download(&self, order_product_id: u64, index: u32)
                        -> Result<serde_json::Value, LibraryServiceError> {
        self.runtime
            .block_on(self.client.prepare_download(order_product_id, index))
            .map_err(map_client_error)
    }
}

pub(super) struct UnavailableSdkGateway {
    error: LibraryServiceError,
}

impl UnavailableSdkGateway {
    pub(super) fn new(error: LibraryServiceError) -> Self {
        Self { error }
    }
}

impl SdkLibraryGateway for UnavailableSdkGateway {
    fn list_order_products(&self, _params: LibraryItemsParams)
                           -> Result<OrderProductListResponse, LibraryServiceError> {
        Err(self.error.clone())
    }

    fn get_order_product(&self, _id: u64) -> Result<OrderProductItemResponse, LibraryServiceError> {
        Err(self.error.clone())
    }

    fn prepare_download(&self, _order_product_id: u64, _index: u32)
                        -> Result<serde_json::Value, LibraryServiceError> {
        Err(self.error.clone())
    }
}
