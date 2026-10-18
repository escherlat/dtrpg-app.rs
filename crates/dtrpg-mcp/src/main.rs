//! MCP server binary that exposes read-only tools to interact with the local
//! library and archive contents.

use std::path::PathBuf;

use dtrpg_core::api::LibraryApi;
use dtrpg_core::data::paths::cache_dir;
use dtrpg_core::services::sdk::{RustSdkCollectionsService, RustSdkLibraryService};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::Implementation;
use rmcp::model::InitializeResult;
use rmcp::model::ProtocolVersion;
use rmcp::model::ServerCapabilities;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
pub struct LibriMcpServer {
    api:         LibraryApi,
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

impl LibriMcpServer {
    fn new(api: LibraryApi) -> Self {
        Self { api,
               tool_router: Self::tool_router() }
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct GetItemRequest {
    id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct ListArchiveEntriesRequest {
    path: String,
}

#[tool_router]
impl LibriMcpServer {
    #[tool(description = "List items in the local library index")]
    async fn list_items(&self) -> String {
        match self.api.index_list_items() {
            Ok(items) => serde_json::to_string(&items).unwrap_or_default(),
            Err(err) => format!("{{\"error\":\"{}\"}}", err),
        }
    }

    #[tool(description = "Get a library item by id")]
    async fn get_item(&self, Parameters(GetItemRequest { id }): Parameters<GetItemRequest>)
                      -> String {
        match self.api.index_get_item(&id) {
            Ok(Some(item)) => serde_json::to_string(&item).unwrap_or_default(),
            Ok(None) => format!("{{\"error\":\"item {} not found\"}}", id),
            Err(err) => format!("{{\"error\":\"{}\"}}", err),
        }
    }

    #[tool(description = "List entries in a local archive file")]
    async fn list_archive_entries(&self,
                                  Parameters(ListArchiveEntriesRequest { path }): Parameters<ListArchiveEntriesRequest>)
                                  -> String {
        let p = PathBuf::from(&path);
        match self.api.list_archive_entries(&p) {
            Ok(entries) => serde_json::to_string(&entries).unwrap_or_default(),
            Err(err) => format!("{{\"error\":\"{}\"}}", err),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for LibriMcpServer {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::LATEST)
            .with_server_info(Implementation::new(
                "libri-mcp".to_string(),
                env!("CARGO_PKG_VERSION").to_string(),
            ))
    }
}

#[tokio::main]
#[allow(clippy::result_large_err)]
async fn main() -> Result<(), rmcp::RmcpError> {
    let api = LibraryApi::new(Box::new(RustSdkLibraryService::unauthenticated()),
                              Box::new(RustSdkCollectionsService::unauthenticated()),
                              cache_dir());

    let service = LibriMcpServer::new(api).serve(rmcp::transport::stdio())
                                          .await?;
    service.waiting().await?;
    Ok(())
}
