//! Active sidebar filter variants.

use std::sync::Arc;

// ── Filtering
// ─────────────────────────────────────────────────────────────────

/// Active filter applied in the sidebar.
#[derive(Debug, Clone, Default)]
pub enum SidebarFilter {
    #[default]
    AllTitles,
    RecentlyUpdated,
    OnDevice,
    InCloud,
    Publisher(Arc<str>),
    /// Filter to items belonging to the DTRPG product list with this id and
    /// display name.
    Collection(u64, Arc<str>),
}

impl PartialEq for SidebarFilter {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::AllTitles, Self::AllTitles)
            | (Self::RecentlyUpdated, Self::RecentlyUpdated)
            | (Self::OnDevice, Self::OnDevice)
            | (Self::InCloud, Self::InCloud) => true,
            (Self::Publisher(a), Self::Publisher(b)) => a == b,
            // Compare collections by id only — names are derived from the same id.
            (Self::Collection(a, _), Self::Collection(b, _)) => a == b,
            _ => false,
        }
    }
}

impl Eq for SidebarFilter {}
