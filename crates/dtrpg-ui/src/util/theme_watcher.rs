//! Hot reload of the user themes directory (ADR-0005).
//!
//! Watches [`themes_dir`] for changes and, once the burst of events an
//! editor produces per save settles, re-runs [`ThemeCatalog::refresh`] and
//! re-installs the active theme so an edited theme file takes effect without
//! a restart. Started once from `init_globals`; runs for the life of the
//! process.

use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::App;
use notify::{EventKind, RecursiveMode, Watcher};

use crate::data::theme::{LibriTheme, install_theme};
use crate::data::theme_catalog::ThemeCatalog;

/// Settle window: editors typically produce several filesystem events per
/// save (truncate, write, rename); re-reading on the first one would race
/// the rest of the burst, so wait for it to end.
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(100);

/// Starts watching the themes directory for changes.
///
/// Non-fatal on failure (a read-only or missing directory just means no hot
/// reload for the session - the catalog was already populated at startup).
pub fn start_theme_watcher(cx: &App) {
    let (tx, rx) = unbounded();
    let mut watcher = match notify::recommended_watcher(event_handler(tx)) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("theme watcher unavailable: {e}");
            return;
        }
    };
    let dir = dtrpg_core::data::paths::themes_dir();
    if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
        tracing::warn!("cannot watch themes directory {}: {e}", dir.display());
        return;
    }
    cx.spawn(async move |cx| {
          // Held for the life of the task; dropping it stops the
          // watch, which is exactly what app shutdown wants.
          let _watcher = watcher;
          theme_reload_loop(rx, cx).await;
      })
      .detach();
}

/// Sends a signal for events that touch a theme TOML file.
fn event_handler(tx: UnboundedSender<()>) -> impl notify::EventHandler {
    move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event
           && event_touches_theme_file(&event)
        {
            let _ = tx.unbounded_send(());
        }
    }
}

/// Debounces the event burst, then re-reads the catalog and the active
/// theme on the main thread. Ends when the watcher (and with it the sender)
/// is gone.
async fn theme_reload_loop(mut rx: UnboundedReceiver<()>, cx: &mut gpui::AsyncApp) {
    while rx.next().await.is_some() {
        // Swallow the rest of this save's event burst before re-reading.
        cx.background_executor().timer(RELOAD_DEBOUNCE).await;
        while rx.try_recv().is_ok() {}
        cx.update(apply_theme_changes);
    }
}

/// Re-reads the theme catalog and re-installs the active theme.
///
/// The active id may name an edited file, a file that was just added (and is
/// now selectable elsewhere), or one that was removed - an id no longer in
/// the catalog falls back to the default theme for display without
/// persisting the fallback, matching startup behavior.
///
/// `pub` for the Settings gallery's Reload button, which runs the same
/// refresh on demand; `Context` derefs to `App`, so callers pass their
/// controller context directly.
pub fn apply_theme_changes(cx: &mut App) {
    ThemeCatalog::refresh();
    let current = cx.global::<LibriTheme>();
    let id = ThemeCatalog::resolve(current.id.as_str()).unwrap_or_default();
    let theme = LibriTheme::new(id, current.density, current.fonts.clone());
    install_theme(theme, cx);
    cx.refresh_windows();
}

/// Whether any path in the event is a theme TOML file - editor saves often
/// fire additional events for sidecar files that discovery ignores.
fn event_touches_theme_file(event: &notify::Event) -> bool {
    let is_toml = |p: &std::path::Path| p.extension().and_then(|e| e.to_str()) == Some("toml");
    // `Modify(Name(Both))` rename events carry both the old and new paths;
    // any matching path is enough to warrant a reload.
    !matches!(event.kind, EventKind::Access(_)) && event.paths.iter().any(|p| is_toml(p))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use notify::EventKind;

    use super::event_touches_theme_file;

    fn event(kind: EventKind, paths: &[&str]) -> notify::Event {
        paths.iter()
             .map(PathBuf::from)
             .fold(notify::Event::new(kind), |e, p| e.add_path(p))
    }

    #[test]
    fn toml_create_modify_and_remove_trigger_reload() {
        for kind in
            [EventKind::Create(notify::event::CreateKind::File),
             EventKind::Modify(notify::event::ModifyKind::Data(notify::event::DataChange::Any)),
             EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::Both)),
             EventKind::Remove(notify::event::RemoveKind::File)]
        {
            let e = event(kind, &["/themes/midnight.toml"]);
            assert!(event_touches_theme_file(&e), "{e:?} must trigger a reload");
        }
    }

    #[test]
    fn rename_carrying_the_new_toml_path_triggers_reload() {
        // Atomic saves rename a temp file onto the target; the toml path is
        // the second path of the pair.
        let e = event(
            EventKind::Modify(notify::event::ModifyKind::Name(
                notify::event::RenameMode::Both,
            )),
            &["/themes/.midnight.toml.swp", "/themes/midnight.toml"],
        );
        assert!(event_touches_theme_file(&e));
    }

    #[test]
    fn non_toml_and_access_events_are_ignored() {
        let e = event(EventKind::Create(notify::event::CreateKind::File),
                      &["/themes/notes.md"]);
        assert!(!event_touches_theme_file(&e));
        let e = event(EventKind::Access(notify::event::AccessKind::Any),
                      &["/themes/midnight.toml"]);
        assert!(!event_touches_theme_file(&e));
    }
}
