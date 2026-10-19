//! Active sidebar filter variants.

use std::collections::HashSet;
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
    /// Filter to the items that have this tag. `items` holds the ids of the
    /// items that had it when the filter was made, so matching an item needs
    /// no lookup; it is replaced when tags change.
    Tag {
        name:  Arc<str>,
        items: Arc<HashSet<Arc<str>>>,
    },
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
            // Tags are the same tag ignoring case, whatever items they were
            // last matched against.
            (Self::Tag { name: a, .. }, Self::Tag { name: b, .. }) => {
                a.to_lowercase() == b.to_lowercase()
            }
            _ => false,
        }
    }
}

impl Eq for SidebarFilter {}
