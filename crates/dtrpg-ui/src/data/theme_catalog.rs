//! Theme identity and the catalog of available themes.
//!
//! [`ThemeId`] is a stable, lowercase string that names a theme (matching its
//! file id in the themes directory). It replaces the former closed `ThemeKey`
//! enum so a theme can be any file in the themes directory, not only a
//! compiled-in variant (ADR-0005).
//!
//! [`ThemeCatalog`] combines the built-in themes with user-authored themes
//! discovered from [`themes_dir`]. Discovery reads the directory once at
//! startup via [`ThemeCatalog::refresh`] and is cached in-process, so the
//! render-path helpers ([`ThemeCatalog::all_ids`], [`theme_label`]) never
//! touch the filesystem.

use std::path::Path;
use std::sync::RwLock;

use dtrpg_core::data::paths::themes_dir;
use rust_i18n::t;
use serde::Deserialize;

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

/// A user-authored theme discovered from the themes directory: its id (file
/// stem) and the display name declared in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserTheme {
    /// Theme id, matching the file stem.
    pub id: String,
    /// Display name from the file's `name` field.
    pub name: String,
}

/// Minimum a theme file must declare for discovery to accept it: a display
/// name. Palette validity is checked when the theme is loaded, not here.
#[derive(Debug, Deserialize)]
struct ThemeNameOnly {
    #[allow(dead_code)] // read for validation; the name surfaces via `UserTheme`.
    name: String,
}

/// Cached user themes, populated once at startup by [`ThemeCatalog::refresh`].
///
/// Held behind a lock rather than re-read per render so the render path stays
/// filesystem-free (see `AGENTS.md`'s "no I/O on the render path"). A future
/// hot-reload trigger re-runs [`ThemeCatalog::refresh`].
static USER_THEMES: RwLock<Vec<UserTheme>> = RwLock::new(Vec::new());

/// Returns a read guard over the cached user themes, tolerating a poisoned
/// lock (a panic while holding it only corrupts the last writer's view, never
/// the underlying list's structural validity).
fn read_users() -> std::sync::RwLockReadGuard<'static, Vec<UserTheme>> {
    USER_THEMES.read().unwrap_or_else(|e| e.into_inner())
}

/// The set of themes the app knows about: the built-in themes plus any
/// user-authored themes discovered in the themes directory (ADR-0005).
pub struct ThemeCatalog;

impl ThemeCatalog {
    /// Discovers user themes from the themes directory and refreshes the
    /// in-process cache. Creates the directory if missing so users have a
    /// place to drop theme files. Called at startup and by any future
    /// hot-reload trigger — not on the render path.
    pub fn refresh() {
        let dir = themes_dir();
        let _ = std::fs::create_dir_all(&dir);
        let users = discover(&dir);
        *USER_THEMES.write().unwrap_or_else(|e| e.into_inner()) = users;
    }

    /// Every known theme id — built-ins followed by user themes — in display
    /// order. Reads only the in-process cache (no I/O).
    pub fn all_ids() -> Vec<ThemeId> {
        let users = read_users();
        let mut ids: Vec<ThemeId> = BUILTIN_IDS.iter().map(|id| ThemeId::from(*id)).collect();
        ids.extend(users.iter().map(|u| ThemeId::from(u.id.as_str())));
        ids
    }

    /// All built-in theme ids, in display order.
    pub fn builtins() -> Vec<ThemeId> {
        BUILTIN_IDS.iter().map(|id| ThemeId::from(*id)).collect()
    }

    /// Whether `id` names a built-in theme.
    pub fn contains(id: &str) -> bool {
        BUILTIN_IDS.contains(&id)
    }

    /// Resolves a persisted id to a [`ThemeId`] if it names a known theme
    /// (built-in or discovered user theme), or `None` for an unrecognized or
    /// removed theme (e.g. a prefs file written by a version with a different
    /// theme set).
    pub fn resolve(id: &str) -> Option<ThemeId> {
        resolve_with(id, &read_users())
    }
}

/// [`ThemeCatalog::resolve`]'s logic against an explicit user-theme list, kept
/// separate so it is testable without mutating the process cache.
fn resolve_with(id: &str, users: &[UserTheme]) -> Option<ThemeId> {
    if BUILTIN_IDS.contains(&id) {
        return Some(ThemeId::from(id));
    }
    users.iter().any(|u| u.id == id).then(|| ThemeId::from(id))
}

/// Localized display label for a theme.
///
/// Built-in themes resolve through the i18n catalog; a user theme resolves to
/// its file's declared name; any id no longer known falls back to its raw id.
pub fn theme_label(id: &ThemeId) -> String {
    label_with(id, &read_users())
}

/// [`theme_label`]'s logic against an explicit user-theme list, kept separate
/// so it is testable without mutating the process cache.
fn label_with(id: &ThemeId, users: &[UserTheme]) -> String {
    match id.as_str() {
        "parchment" => t!("theme.parchment").to_string(),
        "slate" => t!("theme.slate").to_string(),
        "sage" => t!("theme.sage").to_string(),
        "ink" => t!("theme.ink").to_string(),
        "moss" => t!("theme.moss").to_string(),
        "rosewood" => t!("theme.rosewood").to_string(),
        other => users
            .iter()
            .find(|u| u.id == other)
            .map(|u| u.name.clone())
            .unwrap_or_else(|| other.to_string()),
    }
}

/// Reads the themes directory and returns one [`UserTheme`] per valid
/// `*.toml` file, sorted by id for a stable display order.
///
/// Skips files that aren't TOML, don't declare a parseable name, or reuse a
/// built-in id (built-ins take precedence over a same-named user file).
pub fn discover(dir: &Path) -> Vec<UserTheme> {
    let mut users = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return users;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if BUILTIN_IDS.contains(&id) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(parsed) = toml::from_str::<ThemeNameOnly>(&content) else {
            continue;
        };
        users.push(UserTheme {
            id: id.to_string(),
            name: parsed.name,
        });
    }
    users.sort_by(|a, b| a.id.cmp(&b.id));
    users
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures use unwrap/expect for concise assertions.

    use super::{ThemeCatalog, ThemeId, UserTheme, discover, label_with, resolve_with};

    #[test]
    fn default_theme_is_parchment() {
        assert_eq!(ThemeId::default().as_str(), "parchment");
    }

    #[test]
    fn resolve_accepts_known_builtin_and_rejects_unknown() {
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

    #[test]
    fn resolve_with_accepts_user_ids_and_rejects_unknown() {
        let users = vec![UserTheme {
            id: "midnight".into(),
            name: "Midnight".into(),
        }];
        assert!(resolve_with("midnight", &users).is_some());
        assert!(resolve_with("absent", &users).is_none());
        // Built-ins resolve regardless of the user list.
        assert!(resolve_with("sage", &[]).is_some());
    }

    #[test]
    fn label_with_prefers_user_name_then_raw_id_fallback() {
        let users = vec![UserTheme {
            id: "midnight".into(),
            name: "Midnight Rose".into(),
        }];
        assert_eq!(
            label_with(&ThemeId::from("midnight"), &users),
            "Midnight Rose"
        );
        // An id no longer known falls back to the raw id.
        assert_eq!(label_with(&ThemeId::from("gone"), &users), "gone");
        // A built-in resolves through i18n, not the user list.
        assert_eq!(label_with(&ThemeId::from("parchment"), &[]), "Parchment");
    }

    #[test]
    fn discover_reads_valid_themes_and_skips_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("midnight.toml"),
            "name = \"Midnight\"\nmode = \"dark\"\nsurface = \"#101010\"\n",
        )
        .unwrap();
        // A built-in id collision is skipped.
        std::fs::write(
            dir.path().join("parchment.toml"),
            "name = \"Shadow Parchment\"\n",
        )
        .unwrap();
        // No name declared -> skipped.
        std::fs::write(dir.path().join("nameless.toml"), "mode = \"light\"\n").unwrap();
        // Not a theme file -> skipped.
        std::fs::write(dir.path().join("notes.txt"), "ignore me").unwrap();

        let found = discover(dir.path());
        assert_eq!(
            found,
            vec![UserTheme {
                id: "midnight".into(),
                name: "Midnight".into(),
            }]
        );
    }

    #[test]
    fn discover_on_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        assert!(discover(&missing).is_empty());
    }
}
