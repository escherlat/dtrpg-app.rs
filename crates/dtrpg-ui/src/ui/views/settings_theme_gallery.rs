//! Theme gallery in the Settings > Appearance section (ADR-0005): a card
//! per known theme with a miniature preview of its palette, plus the theme
//! file actions — duplicate the active theme, import a theme file, reveal
//! the themes folder, and reload the catalog on demand.
//!
//! Card previews read the preview cache (`theme_loader::preview`), rebuilt
//! by every catalog refresh, so rendering never touches the filesystem (see
//! `AGENTS.md`'s "no I/O on the render path"). Action failures log warnings;
//! the settings window has no notification surface (same as the Storage
//! section's actions).

use gpui::prelude::FluentBuilder;
use gpui::{
    Entity, Hsla, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement,
    Styled, div, px, relative, rgb,
};
use gpui_kit::component::Icon;
use gpui_kit::component::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::data::theme::ColorTokens;
use crate::data::theme_catalog::{ThemeCatalog, ThemeId, theme_label};
use crate::data::theme_loader;
use crate::ui::widgets::small_caps_text;

// Card and preview dimensions from the design's `.theme-card`/`.theme-prev`
// styles, in pixels.
const CARD_WIDTH: f32 = 146.0;
const PREVIEW_HEIGHT: f32 = 76.0;
const TITLEBAR_HEIGHT: f32 = 15.0;
const SIDEBAR_WIDTH: f32 = 26.0;

// Traffic-light dots in the preview titlebar: decorative window chrome with
// fixed colors from the design, not theme data.
const DOT_RED: u32 = 0xEC_6A_5E;
const DOT_YELLOW: u32 = 0xF4_BF_4F;
const DOT_GREEN: u32 = 0x61_C5_54;

/// Renders the theme gallery: a wrapping grid of theme cards, then the
/// theme-file action buttons.
///
/// `colors` and `label_font_family` come from the active theme and style the
/// chrome around each card; a card's interior uses the preview palette of the
/// theme it depicts.
pub fn render_theme_gallery(entity: Entity<LibraryController>, colors: &ColorTokens,
                            theme_id: &ThemeId, label_font_family: &str)
                            -> impl IntoElement + 'static + use<> {
    let grid = div().flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .children(ThemeCatalog::all_ids().into_iter().map(|id| {
                                                                     theme_card(&id,
                                                                                id == *theme_id,
                                                                                entity.clone(),
                                                                                colors)
                                                                 }));

    div().flex()
         .flex_col()
         .gap(px(12.0))
         .child(heading(colors, label_font_family))
         .child(grid)
         .child(action_buttons(entity))
}

/// Section heading: the theme label in small caps, followed by a hairline
/// rule filling the remaining width.
fn heading(colors: &ColorTokens, label_font_family: &str) -> impl IntoElement {
    div().flex()
         .items_center()
         .gap(px(12.0))
         .child(div().text_sm()
                     .font_family(label_font_family.to_string())
                     .text_color(colors.text_primary)
                     .child(small_caps_text(t!("settings.appearance_theme_label"))))
         .child(div().flex_1().h(px(1.0)).bg(colors.border))
}

/// One theme card: the palette preview, a hairline, and the footer with the
/// theme's name, an origin tag for user themes, and a check mark on the
/// active theme. Clicking selects the theme.
fn theme_card(id: &ThemeId, active: bool, entity: Entity<LibraryController>,
              chrome: &ColorTokens)
              -> impl IntoElement + 'static + use<> {
    // Cache-backed; a miss would only be an id added without a catalog
    // refresh, and still resolves without I/O.
    let preview = theme_loader::preview(id);
    let label = theme_label(id);
    let is_builtin = ThemeCatalog::contains(id.as_str());
    let click_id = id.clone();

    div().id(format!("theme-card-{}", id))
         .w(px(CARD_WIDTH))
         .flex_none()
         .rounded(px(9.0))
         .overflow_hidden()
         .border_1()
         .border_color(if active { chrome.accent } else { chrome.border })
         .cursor_pointer()
         .on_click(move |_, _, cx| {
             let pick = click_id.clone();
             entity.update(cx, |ctrl, cx| ctrl.set_theme(pick, cx));
         })
         .child(preview_block(&preview))
         .child(div().h(px(1.0)).bg(chrome.border))
         .child(card_footer(&label, active, is_builtin, chrome))
}

/// The miniature app window: the theme's desktop background, a titlebar in
/// the surface color with three fixed-color dots, then a surface sidebar
/// beside accent/text rows at decreasing widths and opacities.
fn preview_block(p: &ColorTokens) -> impl IntoElement {
    let dot = |color: u32| {
        let hsla: Hsla = rgb(color).into();
        div().size(px(5.0)).rounded_full().bg(hsla)
    };
    let bar = |color: Hsla, width: f32, opacity: f32| {
        div().h(px(4.0))
             .rounded(px(2.0))
             .w(relative(width))
             .bg(Hsla { a: opacity,
                        ..color })
    };

    div().h(px(PREVIEW_HEIGHT))
         .flex()
         .flex_col()
         .bg(p.desktop_bg)
         .child(div().h(px(TITLEBAR_HEIGHT))
                     .flex()
                     .items_center()
                     .gap(px(3.0))
                     .px(px(5.0))
                     .bg(p.surface)
                     .child(dot(DOT_RED))
                     .child(dot(DOT_YELLOW))
                     .child(dot(DOT_GREEN)))
         .child(div().flex()
                     .flex_1()
                     .child(div().w(px(SIDEBAR_WIDTH)).bg(p.surface))
                     .child(div().flex_1()
                                 .flex()
                                 .flex_col()
                                 .gap(px(4.0))
                                 .p(px(6.0))
                                 .child(bar(p.accent, 0.58, 1.0))
                                 .child(bar(p.text_primary, 0.82, 0.72))
                                 .child(bar(p.text_primary, 0.68, 0.48))
                                 .child(bar(p.text_primary, 0.76, 0.32))))
}

/// Card footer in the active theme's chrome: the theme's name, a "user" tag
/// for non-built-in themes, and a check mark on the active card.
fn card_footer(label: &str, active: bool, is_builtin: bool, chrome: &ColorTokens)
               -> impl IntoElement {
    div().flex()
         .items_center()
         .gap(px(6.0))
         .px(px(9.0))
         .py(px(7.0))
         .bg(chrome.surface)
         .child(div().flex_1()
                     .text_sm()
                     .font_weight(gpui::FontWeight::SEMIBOLD)
                     .text_color(chrome.text_primary)
                     .truncate()
                     .child(label.to_string()))
         .when(!is_builtin, |footer| {
             footer.child(div().rounded(px(3.0))
                               .border_1()
                               .border_color(chrome.border)
                               .px(px(4.0))
                               .text_xs()
                               .text_color(chrome.text_tertiary)
                               .child(t!("settings.appearance_theme_origin_user").to_string()))
         })
         .when(active, |footer| {
             footer.child(Icon::new(IconName::CircleCheck).size(px(14.0))
                                                          .text_color(chrome.accent))
         })
}

/// The gallery's file actions: duplicate the active theme for editing,
/// import an external theme file, reveal the themes folder, and reload the
/// catalog without waiting for the file watcher.
fn action_buttons(entity: Entity<LibraryController>) -> impl IntoElement {
    let duplicate_entity = entity.clone();
    let import_entity = entity.clone();
    let reveal_entity = entity.clone();
    let reload_entity = entity;

    div()
        .flex()
        .gap(px(7.0))
        .child(
            Button::new("theme-duplicate")
                .outline()
                .icon(IconName::Copy)
                .child(t!("settings.appearance_theme_duplicate").to_string())
                .on_click(move |_, _, cx| {
                    let entity = duplicate_entity.clone();
                    entity.update(cx, |ctrl, cx| {
                        if let Err(e) = ctrl.duplicate_active_theme(cx) {
                            tracing::warn!("duplicate theme failed: {e}");
                        }
                    });
                }),
        )
        .child(
            Button::new("theme-import")
                .outline()
                .child(t!("settings.appearance_theme_import").to_string())
                .on_click(move |_event, _window, cx| {
                    let entity = import_entity.clone();
                    // Present the picker asynchronously rather than
                    // blocking here: a blocking `rfd::FileDialog` runs
                    // AppKit's modal loop while this handler still holds
                    // gpui's `App` borrow and a resuming task aborts in
                    // `AppCell::borrow_mut` (issue #257). See the Storage
                    // section's "Change…" button for the same pattern.
                    cx.spawn(async move |cx| {
                        let Some(handle) = rfd::AsyncFileDialog::new()
                            .add_filter(
                                t!("settings.appearance_theme_import_filter").to_string(),
                                &["toml"],
                            )
                            .pick_file()
                            .await
                        else {
                            return;
                        };
                        let path = handle.path().to_path_buf();
                        entity.update(cx, |ctrl, cx| {
                            if let Err(e) = ctrl.import_theme_file(path, cx) {
                                tracing::warn!("theme import rejected: {e}");
                            }
                        });
                    })
                    .detach();
                }),
        )
        .child(
            Button::new("theme-reveal-folder")
                .ghost()
                .icon(IconName::FolderOpen)
                .child(t!("settings.appearance_theme_reveal_folder").to_string())
                .on_click(move |_, _, cx| reveal_entity.read(cx).reveal_themes_folder()),
        )
        .child(
            Button::new("theme-reload")
                .ghost()
                .icon(IconName::RefreshCw)
                .child(t!("settings.appearance_theme_reload").to_string())
                .on_click(move |_, _, cx| {
                    reload_entity.update(cx, |ctrl, cx| ctrl.reload_themes(cx));
                }),
        )
}
