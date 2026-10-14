//! App-specific icon assets, embedded from the workspace-level `assets/icons`
//! directory (a much larger icon set than the ~99 icons `gpui-component`
//! bundles for its own `IconName` enum).
//!
//! `gpui::Application::with_assets` accepts only a single
//! [`gpui::AssetSource`], so [`Assets`] falls back to
//! [`gpui_component_assets::Assets`] for any path it doesn't embed itself,
//! keeping `IconName` lookups (used throughout the existing UI) working
//! unchanged.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// Embeds this app's custom icon set from the workspace `assets/icons`
/// directory, falling back to `gpui-component`'s bundled icons.
#[derive(rust_embed::RustEmbed)]
#[folder = "../../assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

impl Assets {
    /// Creates a new instance. `endpoint` exists only for source
    /// compatibility with [`gpui_component_assets::Assets::new`] and is
    /// ignored — assets are embedded into the binary at compile time.
    pub fn new(_endpoint: impl Into<SharedString>) -> Self {
        Self
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        if let Some(file) = Self::get(path) {
            return Ok(Some(file.data));
        }

        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut names: Vec<SharedString> = Self::iter()
            .filter(|p| p.starts_with(path))
            .map(Into::into)
            .collect();
        names.extend(gpui_component_assets::Assets.list(path)?);
        Ok(names)
    }
}
