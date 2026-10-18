use thiserror::Error;

/// Errors returned by container detection and archive operations.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ContainerError {
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("container I/O failed: {0}")]
    Io(String),
    #[error("unsupported or invalid container")]
    InvalidArchive,
    #[error("unsafe archive entry: {0}")]
    UnsafeEntry(String),
    #[error("archive entry not found: {0}")]
    EntryNotFound(String),
}
