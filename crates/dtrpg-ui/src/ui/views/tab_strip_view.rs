//! Tab strip view: segmented tab bar above the main content area.
//!
//! The catalog tab is always first and never closable. Expanded detail tabs
//! (opened by double-clicking a catalog item, per `main-window-tabs`) are
//! appended after it and are closable. The strip uses `gpui-kit`'s
//! `TabBar` overflow menu so tabs beyond the available width remain
//! reachable through a "more" menu.

use dtrpg_core::data::constants::DETAIL_TAB_TITLE_MAX_CHARS;
use gpui::prelude::*;
use gpui::{Entity, IntoElement, SharedString, div};
use gpui_kit::component::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tab::{Tab, TabBar};
use rust_i18n::t;

use crate::controllers::tabs::{TabTarget, TabsController};
use crate::util::text::truncate_with_ellipsis;

/// Renders the tab strip: the non-closable catalog tab followed by any open,
/// closable expanded detail tabs.
pub fn render_tab_strip(tabs: Entity<TabsController>, cx: &gpui::App)
                        -> impl IntoElement + 'static {
    let snap = tabs.read(cx).snapshot();
    let selected_index = snap.open_tabs
                             .iter()
                             .position(|t| *t == snap.active)
                             .unwrap_or(0);

    let mut bar = TabBar::new("main-tab-strip").segmented().menu(true);

    for target in &snap.open_tabs {
        let label: SharedString = match target {
            TabTarget::Catalog => t!("tabs.catalog_tab").to_string().into(),
            TabTarget::Detail(id) => {
                let title = snap.titles
                                .get(id)
                                .cloned()
                                .unwrap_or_else(|| t!("tabs.detail_tab_fallback").to_string());
                // The tab strip has no fixed per-tab width, so a long catalog
                // item title otherwise stretches its tab — and the whole
                // strip — rather than eliding.
                truncate_with_ellipsis(&title, DETAIL_TAB_TITLE_MAX_CHARS).into()
            }
        };

        let mut tab = Tab::new().label(label);
        if let TabTarget::Detail(id) = target {
            let id = id.clone();
            let tabs_for_close = tabs.clone();
            tab = tab.suffix(
                Button::new(SharedString::from(format!("tab-close-{id}")))
                    .ghost()
                    .compact()
                    .icon(IconName::Close)
                    .on_click(move |_, _, cx| {
                        tabs_for_close.update(cx, |ctrl, cx| ctrl.close_detail_tab(&id, cx));
                    }),
            );
        }
        bar = bar.child(tab);
    }

    let tabs_for_click = tabs.clone();
    let open_tabs = snap.open_tabs.clone();
    bar = bar.selected_index(selected_index)
             .on_click(move |ix, _, cx| {
                 if let Some(target) = open_tabs.get(*ix) {
                     let target = target.clone();
                     tabs_for_click.update(cx, |ctrl, cx| ctrl.activate(target, cx));
                 }
             });

    div().flex_none().child(bar)
}
