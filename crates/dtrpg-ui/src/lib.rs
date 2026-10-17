//! DriveThruRPG Rust desktop application entry point.
//!
//! This binary boots the GPUI desktop shell with SDK-backed library data.

// Embed all YAML locale files from `crates/dtrpg-ui/i18n/` at compile time.
// The `t!("module.key")` macro resolves to `crate::_rust_i18n_t(...)`, so this
// invocation must be at the crate root.
rust_i18n::i18n!("i18n", fallback = "en");

pub mod ui {
    pub mod actions;
    pub mod app;
    pub mod library;
    pub mod views;
    pub mod widgets;
}
pub mod build_info;
pub mod controllers;
pub mod data;
pub mod i18n;
pub mod util;
pub mod view_models;
