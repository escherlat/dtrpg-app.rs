//! Count-noun formatting with correct, localized singular/plural selection.
//!
//! All UI count strings MUST route through [`pluralize`] rather than formatting
//! literal English words at call sites, so noun forms come from the current
//! locale's translation catalog.

use rust_i18n::t;

/// Formats `count` with the correctly localized noun form.
///
/// `singular_key` and `plural_key` are `t!()` translation keys (e.g.
/// `"count.item"` / `"count.items"`), not literal words. Returns `"{count}
/// {noun}"` where `noun` is the localized value of `singular_key` when `count
/// == 1`, otherwise the localized value of `plural_key`.
///
/// # Examples
///
/// ```
/// use dtrpg_ui::util::pluralize::pluralize;
///
/// // en.yaml defines count.item: "item", count.items: "items"
/// assert_eq!(pluralize(1, "count.item", "count.items"), "1 item");
/// assert_eq!(pluralize(0, "count.item", "count.items"), "0 items");
/// assert_eq!(pluralize(42, "count.title", "count.titles"), "42 titles");
/// ```
#[must_use]
pub fn pluralize(count: usize, singular_key: &str, plural_key: &str) -> String {
    let key = if count == 1 { singular_key } else { plural_key };
    format!("{count} {}", t!(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_uses_plural() {
        assert_eq!(pluralize(0, "count.item", "count.items"), "0 items");
    }

    #[test]
    fn one_uses_singular() {
        assert_eq!(pluralize(1, "count.item", "count.items"), "1 item");
    }

    #[test]
    fn many_uses_plural() {
        assert_eq!(pluralize(42, "count.title", "count.titles"), "42 titles");
    }

    #[test]
    fn irregular_plural_key_pair() {
        assert_eq!(pluralize(1, "count.publisher_item", "count.publisher_items"),
                   "1 publisher item");
        assert_eq!(pluralize(3, "count.publisher_item", "count.publisher_items"),
                   "3 publisher items");
    }

    #[test]
    fn missing_key_falls_back_to_key_string() {
        // No locale entry exists for these keys; `t!()` falls back to returning
        // the key itself when no translation and no literal fallback text
        // apply.
        assert_eq!(pluralize(1, "count.nonexistent", "count.nonexistent_plural"),
                   "1 count.nonexistent");
    }
}
