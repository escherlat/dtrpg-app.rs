//! Stub methods for generating test data. TODO: remove

use std::time::{SystemTime, UNIX_EPOCH};

use crate::data::enums::ItemStatus;
use crate::data::library::LibraryItem;

fn stub_date_added(added_order: u32) -> Option<i64> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)
                               .map(|d| d.as_secs() as i64)
                               .unwrap_or(0);
    Some(now - added_order as i64 * 43_200)
}

// ── Stub catalog
// ──────────────────────────────────────────────────────────────

#[path = "stubs_part1.rs"]
mod stubs_part1;
#[path = "stubs_part2.rs"]
mod stubs_part2;
#[path = "stubs_part3.rs"]
mod stubs_part3;

/// Returns the full stub catalog matching the design prototype data.
#[must_use]
pub fn stub_catalog() -> Vec<LibraryItem> {
    stubs_part1::stub_catalog_part1().into_iter()
                                     .chain(stubs_part2::stub_catalog_part2())
                                     .chain(stubs_part3::stub_catalog_part3())
                                     .collect()
}
