//! GPUI-free core: domain models, persistence, service traits and their SDK
//! implementations, and the credential store. See
//! `docs/adr/0002-invert-crate-graph-and-add-library-facade.md`.
//!
//! No module here may depend on `gpui`. `dtrpg-ui` depends on this crate, not
//! the other way round; the desktop app (`dtrpg-desktop`) and the MCP server
//! (`dtrpg-mcp`) are both thin binaries on top of it.

pub mod api;
pub mod constants;
pub mod credentials;
pub mod data;
pub mod logging;
pub mod models;
pub mod platform;
pub mod runtime;
pub mod services;
pub mod util;
pub mod work;
