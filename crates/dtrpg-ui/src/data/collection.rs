//! Collection domain type.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// A user-created product list (DTRPG "collection").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionEntry {
    /// Numeric product list identifier.
    pub id:         u64,
    /// Display name of the collection.
    pub name:       Arc<str>,
    /// Product `order_product_id` values belonging to this collection.
    pub member_ids: Arc<[u64]>,
    /// Creation timestamp, Unix epoch seconds. Defaults to `0` when
    /// deserializing cache files written before this field existed — the
    /// cache is a startup-speed optimization, not a source of truth, and
    /// gets overwritten with real timestamps by the next `load_collections`
    /// fetch.
    #[serde(default)]
    pub created_at: i64,
}
