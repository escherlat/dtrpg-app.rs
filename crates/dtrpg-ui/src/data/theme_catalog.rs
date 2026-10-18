//! Theme identity and the catalog of available themes.
//!
//! [`ThemeId`] is a stable, lowercase string that names a theme (matching its
//! file id in the themes directory). It replaces the former closed `ThemeKey`
//! enum so a theme can be any file in the themes directory, not only a
//! compiled-in variant (ADR-0005). [`ThemeCatalog`] is the set of themes the
//! app knows about — today the built-ins, extended later by a themes
//! directory.

use rust_i18n::t;

/// Identifies a theme by its stable, lowercase id (e.g. `"parchment"`).
///
/// The id is independent of the display name so renaming a theme's label or
/// file never invalidates a persisted selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThemeId(String);

impl ThemeId {
    /// The stable identifier used for persistence and theme-file lookup.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ThemeId {
    fn default() -> Self {
        ThemeId::from("parchment")
    }
}

impl From<&str> for ThemeId {
    fn from(s: &str) -> Self {
        ThemeId(s.to_string())
    }
}

impl From<String> for ThemeId {
    fn from(s: String) -> Self {
        ThemeId(s)
    }
}

impl std::fmt::Display for ThemeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Built-in theme ids, in the display order offered by pickers.
const BUILTIN_IDS: [&str; 6] = ["parchment", "slate", "sage", "ink", "moss", "rosewood"];

/// The set of themes the app knows about.
///
/// Today this is the built-in themes shipped as embedded files; a themes
/// directory will extend it with user-authored themes (ADR-0005).
pub struct ThemeCatalog;

impl ThemeCatalog {
    /// All built-in theme ids, in display order.
    pub fn builtins() -> Vec<ThemeId> {
        BUILTIN_IDS.iter().map(|id| ThemeId::from(*id)).collect()
    }

    /// Whether `id` names a known theme.
    pub fn contains(id: &str) -> bool {
        BUILTIN_IDS.contains(&id)
    }

    /// Resolves a persisted id to a [`ThemeId`] if it names a known theme, or
    /// `None` for an unrecognized or removed theme (e.g. a prefs file written
    /// by a version with a different theme set).
    pub fn resolve(id: &str) -> Option<ThemeId> {
        Self::contains(id).then(|| ThemeId::from(id))
    }
}

/// Localized display label for a theme.
///
/// Built-in themes resolve through the i18n catalog; any other id (a user
/// theme, or an id no longer recognized) falls back to its raw id.
pub fn theme_label(id: &ThemeId) -> String {
    match id.as_str() {
        "parchment" => t!("theme.parchment").to_string(),
        "slate" => t!("theme.slate").to_string(),
        "sage" => t!("theme.sage").to_string(),
        "ink" => t!("theme.ink").to_string(),
        "moss" => t!("theme.moss").to_string(),
        "rosewood" => t!("theme.rosewood").to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ThemeCatalog, ThemeId};

    #[test]
    fn default_theme_is_parchment() {
        assert_eq!(ThemeId::default().as_str(), "parchment");
    }

    #[test]
    fn resolve_accepts_known_and_rejects_unknown_ids() {
        assert!(ThemeCatalog::resolve("ink").is_some());
        assert!(ThemeCatalog::resolve("nonexistent-theme").is_none());
    }

    #[test]
    fn builtins_covers_every_id_it_claims_to_contain() {
        let builtins = ThemeCatalog::builtins();
        assert_eq!(builtins.len(), 6);
        for id in &builtins {
            assert!(ThemeCatalog::contains(id.as_str()));
        }
    }
}
