//! Storage root configuration, placement rules, and capacity helpers.

mod config;
mod paths;

#[cfg(test)]
mod tests;

pub mod placement;
pub mod roots;

pub use config::{DEFAULT_MAX_CONCURRENT_DOWNLOADS, DEFAULT_ROOT_ID, StorageConfig, StorageError};
pub use paths::{available_space_mb, collection_dir, publisher_dir, validate_writable};
