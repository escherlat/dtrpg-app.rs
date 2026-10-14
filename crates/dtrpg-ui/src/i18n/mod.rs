//! Locale initialization and the embedded string catalog.
//!
//! Call [`init`] once at startup, before any view renders. After that, use
//! `t!("module.key")` anywhere in the crate to get a translated string.
//!
//! # Supported locales
//! `en` (authoritative), `fr` (stub), `de` (stub). The system locale is
//! detected at startup via `sys-locale`; unrecognized locales fall back to
//! `en`. A persisted user selection (see [`crate::data::ui_preferences`])
//! takes precedence over both.

const SUPPORTED: [&str; 3] = ["en", "fr", "de"];

/// A supported UI locale, with its `rust_i18n` code and its endonym (the
/// language's name in itself, not translated into the active locale).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    Fr,
    De,
}

impl Locale {
    /// All supported locales, in the order they should appear in a picker.
    pub const ALL: [Locale; 3] = [Locale::En, Locale::Fr, Locale::De];

    /// The `rust_i18n` locale code (`"en"`, `"fr"`, `"de"`).
    pub fn code(&self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Fr => "fr",
            Locale::De => "de",
        }
    }

    /// The language's endonym (native name), e.g. "Français" for French.
    ///
    /// Deliberately not translated via `t!()` — a language's own name
    /// shouldn't change depending on whatever locale is currently active.
    pub fn label(&self) -> &'static str {
        match self {
            Locale::En => "English",
            Locale::Fr => "Français",
            Locale::De => "Deutsch",
        }
    }

    /// Maps an `rust_i18n` locale code back to a `Locale`, if supported.
    pub fn from_code(code: &str) -> Option<Locale> {
        Locale::ALL.into_iter().find(|l| l.code() == code)
    }
}

/// Detect the active locale and activate it.
///
/// Checks `UiPreferences::load().locale()` first; if present and still
/// supported, uses it. Otherwise falls back to system-locale detection.
///
/// Must be called before any `t!()` macro is evaluated.
pub fn init() {
    let persisted = crate::data::ui_preferences::UiPreferences::load().locale()
                                                                      .map(str::to_owned);

    let chosen = resolve_locale(persisted.as_deref(), &detect_os_locale());

    rust_i18n::set_locale(&chosen);
    tracing::debug!(locale = %chosen, "i18n locale initialized");
}

/// Picks the locale code to activate: a persisted override if present and
/// still supported, otherwise the OS-detected code.
fn resolve_locale(persisted: Option<&str>, os_detected: &str) -> String {
    persisted.filter(|code| SUPPORTED.contains(code))
             .unwrap_or(os_detected)
             .to_owned()
}

fn detect_os_locale() -> String {
    let raw = sys_locale::get_locale();

    let locale = raw.as_deref()
                    .and_then(|l| l.split(['-', '_']).next())
                    .unwrap_or("en")
                    .to_owned();

    if SUPPORTED.contains(&locale.as_str()) {
        locale
    }
    else {
        "en".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_code_and_label_round_trip() {
        for locale in Locale::ALL {
            assert_eq!(Locale::from_code(locale.code()), Some(locale));
            assert!(!locale.label().is_empty());
        }
    }

    #[test]
    fn from_code_rejects_unsupported() {
        assert_eq!(Locale::from_code("xx"), None);
    }

    #[test]
    fn resolve_locale_prefers_valid_persisted_override() {
        assert_eq!(resolve_locale(Some("fr"), "en"), "fr");
    }

    #[test]
    fn resolve_locale_falls_back_on_unsupported_persisted_code() {
        assert_eq!(resolve_locale(Some("xx"), "de"), "de");
    }

    #[test]
    fn resolve_locale_falls_back_when_nothing_persisted() {
        assert_eq!(resolve_locale(None, "de"), "de");
    }
}
