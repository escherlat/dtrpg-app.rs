//! MCP server binary that exposes tools to interact with the local library and
//! archive contents.
//!
//! The tools that read are always available. The ones that change the library
//! (`set_tags`, `set_tag_color`) refuse unless the server was started with
//! `--allow-write`, so that whoever configures an MCP client decides whether it
//! may edit the library.

use std::path::PathBuf;

use dtrpg_core::api::LibraryApi;
use dtrpg_core::data::paths::cache_dir;
use dtrpg_core::data::tag_color::TagColor;
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
    /// Whether the tools that change the library may do so.
    allow_write: bool,
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

impl LibriMcpServer {
    fn new(api: LibraryApi, allow_write: bool) -> Self {
        Self { api,
               allow_write,
               tool_router: Self::tool_router() }
    }

    /// The reply of a tool that changes the library when the server was not
    /// started with `--allow-write`.
    fn writes_disabled() -> String {
        error_json("changing the library is disabled; start libri-mcp with --allow-write to allow it")
    }
}

/// An error as the JSON a tool replies with, which is valid whatever the
/// message contains.
fn error_json(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

/// A tag as a tool reports it.
#[derive(Serialize)]
struct TagInfo {
    name:  String,
    /// The tag's color, or `null`.
    color: Option<String>,
    /// How many items have the tag. Absent for an item's own tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<usize>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct GetItemRequest {
    id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct SetTagsRequest {
    /// The item's id, as `list_items` gives it.
    id:   String,
    /// Every tag the item should have after this. Replaces the item's tags.
    tags: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
struct SetTagColorRequest {
    /// The tag's name, ignoring case.
    tag:   String,
    /// One of gray, green, purple, blue, yellow, red or orange, or `null` or
    /// "none" to clear the color.
    color: Option<String>,
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

    #[tool(description = "List every tag in the library, with its color and how many items have it")]
    async fn list_tags(&self) -> String {
        match self.api.index_tag_library() {
            Ok(library) => {
                let tags: Vec<TagInfo> =
                    library.entries()
                           .iter()
                           .map(|entry| TagInfo { name:  entry.name.to_string(),
                                                  color: entry.color.map(|c| c.to_string()),
                                                  count: Some(entry.count), })
                           .collect();
                serde_json::to_string(&tags).unwrap_or_default()
            }
            Err(err) => error_json(&err.to_string()),
        }
    }

    #[tool(description = "Get the tags of a library item by id")]
    async fn get_item_tags(&self, Parameters(GetItemRequest { id }): Parameters<GetItemRequest>)
                           -> String {
        match self.api.index_tag_library() {
            Ok(library) => {
                let tags: Vec<TagInfo> =
                    library.tags_of(&id)
                           .into_iter()
                           .map(|entry| TagInfo { name:  entry.name.to_string(),
                                                  color: entry.color.map(|c| c.to_string()),
                                                  count: None, })
                           .collect();
                serde_json::to_string(&tags).unwrap_or_default()
            }
            Err(err) => error_json(&err.to_string()),
        }
    }

    #[tool(description = "Replace the tags of a library item. Changes the library; refused unless the server allows writes. The new tags reach the item's files the next time Libri starts or the library is reloaded.")]
    async fn set_tags(&self, Parameters(SetTagsRequest { id, tags }): Parameters<SetTagsRequest>)
                      -> String {
        if !self.allow_write {
            return Self::writes_disabled();
        }
        match self.api.index_set_item_tags(&id, &tags) {
            Ok(()) => serde_json::json!({ "ok": true }).to_string(),
            Err(err) => error_json(&err.to_string()),
        }
    }

    #[tool(description = "Set the color of a tag, or clear it. Changes the library; refused unless the server allows writes.")]
    async fn set_tag_color(&self,
                           Parameters(SetTagColorRequest { tag, color }): Parameters<SetTagColorRequest>)
                           -> String {
        if !self.allow_write {
            return Self::writes_disabled();
        }
        let color = match color.as_deref().map(str::trim) {
            None | Some("") | Some("none") => None,
            Some(name) => match name.parse::<TagColor>() {
                Ok(color) => Some(color),
                Err(err) => return error_json(&err.to_string()),
            },
        };
        match self.api.index_set_tag_color(&tag, color) {
            Ok(true) => serde_json::json!({ "ok": true }).to_string(),
            Ok(false) => error_json(&format!("no item has the tag {tag:?}")),
            Err(err) => error_json(&err.to_string()),
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

    let allow_write = std::env::args().any(|arg| arg == "--allow-write");
    let service = LibriMcpServer::new(api, allow_write).serve(rmcp::transport::stdio())
                                                       .await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::PathBuf;

    use dtrpg_core::data::enums::ItemStatus;
    use dtrpg_core::data::index::LibraryIndex;
    use dtrpg_core::data::library::LibraryItem;

    use super::*;

    /// A server over a library index in a fresh temporary folder, holding items
    /// `a` and `b`, with the folder to remove afterwards.
    fn server(allow_write: bool, name: &str) -> (LibriMcpServer, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("libri_mcp_test_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let item = |id: &str| {
            LibraryItem::new(id,
                             "Title",
                             "Free League",
                             "",
                             "Core",
                             "PDF",
                             100,
                             10.0,
                             2024,
                             0,
                             ItemStatus::Cloud,
                             "#1C2A44",
                             "A game.",
                             None)
        };
        LibraryIndex::open_and_migrate(&dir).unwrap()
                                            .replace_all_items(&[item("a"), item("b")])
                                            .unwrap();
        let api = LibraryApi::new(Box::new(RustSdkLibraryService::unauthenticated()),
                                  Box::new(RustSdkCollectionsService::unauthenticated()),
                                  dir.clone());
        (LibriMcpServer::new(api, allow_write), dir)
    }

    fn json(reply: &str) -> serde_json::Value {
        serde_json::from_str(reply).unwrap()
    }

    fn set_tags(id: &str, tags: &[&str]) -> Parameters<SetTagsRequest> {
        Parameters(SetTagsRequest { id:   id.to_owned(),
                                    tags: tags.iter().map(|t| (*t).to_owned()).collect(), })
    }

    fn id(id: &str) -> Parameters<GetItemRequest> {
        Parameters(GetItemRequest { id: id.to_owned() })
    }

    fn color(tag: &str, color: Option<&str>) -> Parameters<SetTagColorRequest> {
        Parameters(SetTagColorRequest { tag:   tag.to_owned(),
                                        color: color.map(str::to_owned), })
    }

    #[tokio::test]
    async fn the_tools_that_read_list_tags_with_colors_and_counts() {
        let (server, dir) = server(true, "read");
        server.set_tags(set_tags("a", &["solo", "to run"])).await;
        server.set_tags(set_tags("b", &["solo"])).await;
        server.set_tag_color(color("solo", Some("red"))).await;

        let tags = json(&server.list_tags().await);
        assert_eq!(tags[0],
                   serde_json::json!({ "name": "solo", "color": "red", "count": 2 }));
        assert_eq!(tags[1],
                   serde_json::json!({ "name": "to run", "color": null, "count": 1 }));

        let of_a = json(&server.get_item_tags(id("a")).await);
        assert_eq!(of_a,
                   serde_json::json!([{ "name": "solo", "color": "red" },
                                      { "name": "to run", "color": null }]));
        assert_eq!(json(&server.get_item_tags(id("missing")).await),
                   serde_json::json!([]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_tools_that_change_the_library_refuse_unless_writes_are_allowed() {
        let (server, dir) = server(false, "refuse");

        let tagged = json(&server.set_tags(set_tags("a", &["solo"])).await);
        let colored = json(&server.set_tag_color(color("solo", Some("red"))).await);

        assert!(tagged["error"].as_str().unwrap().contains("--allow-write"));
        assert!(colored["error"].as_str().unwrap().contains("--allow-write"));
        assert_eq!(json(&server.list_tags().await), serde_json::json!([]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn setting_tags_replaces_the_items_tags_and_keeps_a_colors_that_stays() {
        let (server, dir) = server(true, "replace");
        server.set_tags(set_tags("a", &["solo", "old"])).await;
        server.set_tag_color(color("solo", Some("blue"))).await;

        let reply = json(&server.set_tags(set_tags("a", &["Solo", "new"])).await);

        assert_eq!(reply, serde_json::json!({ "ok": true }));
        let tags = json(&server.get_item_tags(id("a")).await);
        assert_eq!(tags,
                   serde_json::json!([{ "name": "new", "color": null },
                                      { "name": "solo", "color": "blue" }]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_tag_that_cannot_be_stored_or_an_unknown_item_is_an_error_and_changes_nothing() {
        let (server, dir) = server(true, "invalid");
        server.set_tags(set_tags("a", &["solo"])).await;

        let comma = json(&server.set_tags(set_tags("a", &["no, commas"])).await);
        let missing = json(&server.set_tags(set_tags("missing", &["solo"])).await);

        assert!(comma["error"].is_string());
        assert!(missing["error"].is_string());
        assert_eq!(json(&server.get_item_tags(id("a")).await),
                   serde_json::json!([{ "name": "solo", "color": null }]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_color_is_set_cleared_and_checked() {
        let (server, dir) = server(true, "colors");
        server.set_tags(set_tags("a", &["solo"])).await;

        assert_eq!(json(&server.set_tag_color(color("SOLO", Some("green"))).await),
                   serde_json::json!({ "ok": true }));
        assert_eq!(json(&server.list_tags().await)[0]["color"], "green");

        assert_eq!(json(&server.set_tag_color(color("solo", None)).await),
                   serde_json::json!({ "ok": true }));
        assert!(json(&server.list_tags().await)[0]["color"].is_null());

        let bad = json(&server.set_tag_color(color("solo", Some("teal"))).await);
        assert!(bad["error"].as_str().unwrap().contains("teal"));
        let unknown = json(&server.set_tag_color(color("nothing", Some("red"))).await);
        assert!(unknown["error"].is_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_error_is_valid_json_whatever_the_message_holds() {
        let reply = error_json("a \"quoted\" message\nwith a newline");

        assert_eq!(json(&reply)["error"],
                   "a \"quoted\" message\nwith a newline");
    }
}
