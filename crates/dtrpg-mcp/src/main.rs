//! Stub MCP server binary.
//!
//! Proves `dtrpg-core` links and runs with no GPUI dependency -- see
//! `docs/adr/0002-invert-crate-graph-and-add-library-facade.md`. The actual
//! MCP protocol surface (tools, scopes, audit logging) is scoped to the
//! `mcp-server` child change and is not implemented here.

use dtrpg_core::api::LibraryApi;
use dtrpg_core::data::paths::cache_dir;
use dtrpg_core::services::sdk::{RustSdkCollectionsService, RustSdkLibraryService};

fn main() {
    let api = LibraryApi::new(Box::new(RustSdkLibraryService::unauthenticated()),
                              Box::new(RustSdkCollectionsService::unauthenticated()),
                              cache_dir());

    match api.list_items() {
        Ok(items) => println!("dtrpg-mcp stub: linked dtrpg-core with no GPUI dependency, \
                               {} item(s) visible unauthenticated",
                              items.len()),
        Err(e) => println!("dtrpg-mcp stub: linked dtrpg-core with no GPUI dependency, \
                            list_items returned an expected unauthenticated error: {e}"),
    }
}
