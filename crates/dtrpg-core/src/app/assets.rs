//! App-specific icon assets, embedded from the workspace-level `assets/icons`
//! directory (a much larger icon set than gpui-kit bundles for its own
//! `IconName` enum).
//!
//! `gpui::Application::with_assets` accepts only a single
//! [`gpui::AssetSource`], so [`Assets`] falls back to [`gpui_kit::assets::
//! Assets`] for any path it doesn't embed itself, keeping `IconName` lookups
//! (used throughout the existing UI) working unchanged.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// Embeds this app's custom icon set from the workspace `assets/icons`
/// directory, falling back to gpui-kit's bundled icons.
#[derive(rust_embed::RustEmbed)]
#[folder = "../../assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

impl Assets {
    /// Creates a new instance. `endpoint` exists only for source
    /// compatibility with [`gpui_kit::assets::Assets::new`] and is
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

        // Unlike the old `gpui_component_assets::Assets::load`, gpui-kit's
        // falls-back fails with `Err` rather than `Ok(None)` for an unknown
        // path — downgrade that to `Ok(None)` to preserve the old
        // "unrecognized path is just absent, not an error" contract.
        Ok(gpui_kit::assets::Assets.load(path).unwrap_or(None))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut names: Vec<SharedString> = Self::iter().filter(|p| p.starts_with(path))
                                                       .map(Into::into)
                                                       .collect();
        names.extend(gpui_kit::assets::Assets.list(path)?);
        Ok(names)
    }
}
