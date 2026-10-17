use std::sync::Arc;

// ── Selection
// ─────────────────────────────────────────────────────────────────

/// What is currently selected in the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Selection {
    #[default]
    None,
    Item(Arc<str>),
}
