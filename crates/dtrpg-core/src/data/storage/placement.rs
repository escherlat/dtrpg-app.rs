//! Placement rules: an ordered list of (match condition -> target root),
//! evaluated top to bottom, falling back to the configured default root
//! when nothing matches.

use serde::{Deserialize, Serialize};

/// A single placement condition. The first rule whose `condition` matches
/// an item wins; rules are evaluated in list order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlacementCondition {
    /// Matches items from the given publisher (exact, case-sensitive match
    /// against `LibraryItem::publisher`).
    Publisher(String),
    /// Matches items that are a member of the given collection id.
    Collection(u64),
    /// Matches a file whose size is at least `mb` megabytes.
    SizeAtLeastMb(f64),
}

impl PlacementCondition {
    /// Returns `true` if this condition matches an item with the given
    /// `publisher`, membership in `collection_ids`, and a file of
    /// `size_mb`.
    #[must_use]
    pub fn matches(&self, publisher: &str, collection_ids: &[u64], size_mb: f64) -> bool {
        match self {
            Self::Publisher(name) => name == publisher,
            Self::Collection(id) => collection_ids.contains(id),
            Self::SizeAtLeastMb(threshold) => size_mb >= *threshold,
        }
    }
}

/// One ordered placement rule: route matching items to `target_root_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacementRule {
    pub condition: PlacementCondition,
    pub target_root_id: u32,
}

/// Returns the id of the first rule in `rules` (evaluated in order) whose
/// condition matches, or `None` if no rule matches -- callers fall back to
/// the configured default root in that case.
#[must_use]
pub fn resolve_root_id(
    rules: &[PlacementRule], publisher: &str, collection_ids: &[u64], size_mb: f64,
) -> Option<u32> {
    rules
        .iter()
        .find(|rule| rule.condition.matches(publisher, collection_ids, size_mb))
        .map(|rule| rule.target_root_id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn publisher_condition_matches_exact_name() {
        let cond = PlacementCondition::Publisher("Free League".to_string());
        assert!(cond.matches("Free League", &[], 0.0));
        assert!(!cond.matches("Chaosium", &[], 0.0));
    }

    #[test]
    fn collection_condition_matches_membership() {
        let cond = PlacementCondition::Collection(42);
        assert!(cond.matches("anyone", &[1, 42, 7], 0.0));
        assert!(!cond.matches("anyone", &[1, 7], 0.0));
    }

    #[test]
    fn size_condition_matches_at_or_above_threshold() {
        let cond = PlacementCondition::SizeAtLeastMb(500.0);
        assert!(cond.matches("anyone", &[], 500.0));
        assert!(cond.matches("anyone", &[], 600.0));
        assert!(!cond.matches("anyone", &[], 499.9));
    }

    #[test]
    fn resolve_root_id_returns_none_when_nothing_matches() {
        let rules = vec![PlacementRule {
            condition: PlacementCondition::Publisher("Chaosium".to_string()),
            target_root_id: 2,
        }];
        assert_eq!(resolve_root_id(&rules, "Free League", &[], 0.0), None);
    }

    #[test]
    fn resolve_root_id_returns_the_first_matching_rule() {
        let rules = vec![
            PlacementRule {
                condition: PlacementCondition::Publisher("Free League".to_string()),
                target_root_id: 2,
            },
            PlacementRule {
                condition: PlacementCondition::SizeAtLeastMb(100.0),
                target_root_id: 3,
            },
        ];
        // Matches both rules; the first in list order wins.
        assert_eq!(resolve_root_id(&rules, "Free League", &[], 200.0), Some(2));
    }

    #[test]
    fn resolve_root_id_falls_through_to_a_later_matching_rule() {
        let rules = vec![
            PlacementRule {
                condition: PlacementCondition::Publisher("Chaosium".to_string()),
                target_root_id: 2,
            },
            PlacementRule {
                condition: PlacementCondition::SizeAtLeastMb(100.0),
                target_root_id: 3,
            },
        ];
        assert_eq!(resolve_root_id(&rules, "Free League", &[], 200.0), Some(3));
    }
}
