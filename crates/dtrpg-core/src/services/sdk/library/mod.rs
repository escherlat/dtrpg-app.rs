//! Rust SDK-backed implementation of [`LibraryService`].

mod download;
mod errors;
mod gateway;
mod mapping;

use dtrpg_sdk::{IncludedItem, LibraryItemsParams};

pub use self::gateway::SdkLibraryGateway;
use self::gateway::{HttpSdkLibraryGateway, UnavailableSdkGateway};
use self::mapping::{last_page_from_links, map_order_product, product_lookup, publisher_lookup};
use crate::{
    data::{library::LibraryItem, token_status::TokenStatusHandle},
    services::{DownloadControl, LibraryService, LibraryServiceError, LibraryServiceErrorKind},
};

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
    pub fn from_keyring_with_tokens(tokens: crate::services::LoginTokens,
                                    token_status: TokenStatusHandle)
                                    -> Self {
        match HttpSdkLibraryGateway::from_keyring_with_tokens(tokens, token_status) {
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
                     control: DownloadControl<'_>)
                     -> Result<(), LibraryServiceError> {
        download::download_item(self.gateway.as_ref(),
                                order_product_id,
                                index,
                                dest,
                                control)
    }
}

#[cfg(test)]
// The existing gateway fixtures use expect for concise known-good assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "library_tests.rs"]
mod tests;
