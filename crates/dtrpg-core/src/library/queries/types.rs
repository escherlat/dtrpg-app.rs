//! The data `Library`'s filtered/grouped query cache operates on: the
//! caller-supplied view state, and the cache itself.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::SystemTime;

use crate::data::enums::GroupBy;
use crate::data::library::{LibraryItem, SectionCounts};
use crate::util::filter::SidebarFilter;
use crate::util::publisher::{ItemGroup, PublisherEntry};
use crate::util::sort::{SortDirection, SortMethod};

/// The sidebar filter, search text, sort, and grouping the controller is
/// currently showing — everything [`super::super::Library::invalidate_cache`]
/// needs to recompute the filtered/grouped result set, independent of any
/// `gpui` type.
#[derive(Clone, Debug, Default)]
pub struct ViewState {
    pub filter:                       SidebarFilter,
    /// Numeric ids a `Collection` filter matches against. Empty for every
    /// other filter.
    pub collection_members:           HashSet<u64>,
    pub search_query:                 String,
    pub sort:                         SortMethod,
    pub sort_direction:               SortDirection,
    pub group_by:                     GroupBy,
    pub recently_updated_window_days: u32,
}

/// The cached result of the most recent
/// [`super::super::Library::invalidate_cache`] call: the filtered/sorted item
/// list, its grouped view, the sidebar facet counts, and the full-catalog
/// product-id set. Render paths read these fields through `Library`'s accessor
/// methods; nothing here is recomputed on a read.
#[derive(Default)]
pub(crate) struct QueryCache {
    pub(super) items:          Option<Arc<Vec<LibraryItem>>>,
    pub(super) grouped:        Option<Arc<Vec<ItemGroup>>>,
    pub(super) filter_count:   usize,
    pub(super) catalog_ids:    Arc<HashSet<u64>>,
    pub(super) section_counts: SectionCounts,
    pub(super) publishers:     Vec<PublisherEntry>,
}

/// Session-only per-item timestamps the index never persists, overlaid onto
/// an item right after it's read from the index.
#[derive(Default)]
pub(crate) struct EphemeralOverlay {
    pub(super) thumbnail_attempts:  HashMap<Arc<str>, SystemTime>,
    pub(super) availability_checks: HashMap<Arc<str>, SystemTime>,
}
