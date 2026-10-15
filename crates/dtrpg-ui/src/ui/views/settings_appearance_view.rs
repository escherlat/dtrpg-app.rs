//! Appearance settings section: searchable font pickers for the four font
//! roles (body, value, label, monospace) covering every font installed on
//! the user's system, a shared UI text scale stepper, and the active color
//! theme picker.
//!
//! Font/theme changes apply immediately via `LibraryController`'s
//! `set_body_font`/`set_value_font`/`set_label_font`/`set_mono_font`/
//! `set_ui_text_size`/`set_theme`, which also persist the selection through
//! `crate::data::ui_preferences::UiPreferences` — see
//! `settings-appearance-fonts`.

use gpui::{
    Entity, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::select::Select;
use gpui_kit::component::tooltip::Tooltip;
use rust_i18n::t;

use crate::controllers::library::LibraryController;
use crate::controllers::settings::FontSelectState;
use crate::data::constants::{
    BODY_FONT_SIZE_PT, DEFAULT_UI_TEXT_SIZE, LABEL_MONO_FONT_SIZE_PT, MAX_UI_TEXT_SCALE,
    MIN_UI_TEXT_SCALE, UI_TEXT_SCALE_STEP, VALUE_FONT_SIZE_RATIO,
};
use crate::data::theme::{ColorTokens, LibriTheme, ThemeKey};
use crate::ui::widgets::small_caps_text;

/// Width of the label column shared by every row in this section, so the
/// controls on the right all start at the same horizontal position
/// regardless of how long each row's label text is.
const LABEL_COL_WIDTH: gpui::Pixels = px(130.0);

/// All theme keys offered by the picker, in display order.
const THEME_KEYS: [ThemeKey; 6] = [ThemeKey::Parchment,
                                   ThemeKey::Slate,
                                   ThemeKey::Sage,
                                   ThemeKey::Ink,
                                   ThemeKey::Moss,
                                   ThemeKey::Rosewood];

fn theme_label(key: ThemeKey) -> String {
    match key {
        ThemeKey::Parchment => t!("theme.parchment").to_string(),
        ThemeKey::Slate => t!("theme.slate").to_string(),
        ThemeKey::Sage => t!("theme.sage").to_string(),
        ThemeKey::Ink => t!("theme.ink").to_string(),
        ThemeKey::Moss => t!("theme.moss").to_string(),
        ThemeKey::Rosewood => t!("theme.rosewood").to_string(),
    }
}

/// Renders a row's label in the shared, fixed-width left column so every
/// row's control starts at the same horizontal position. Rendered in the
/// app's dedicated label font (not the body font every other row's own
/// content picker might be set to) and in small caps — see
/// [`small_caps_text`].
fn row_label(title: impl Into<gpui::SharedString>, colors: &ColorTokens, label_font_family: &str)
             -> impl IntoElement {
    div().w(LABEL_COL_WIDTH)
         .flex_none()
         .text_sm()
         .font_family(label_font_family.to_string())
         .text_color(colors.text_primary)
         .child(small_caps_text(title))
}

/// Renders one font-picker row: the role's label in the shared left column, a
/// searchable [`Select`] in the middle, and the font's rendered size in
/// points on the right — shown alongside the picker (not just settable
/// elsewhere) so a low-vision user picking a font can see, without hunting
/// for a separate control, exactly what size it will render at.
///
/// `select` is `None` for one render pass at most (the root view attaches it
/// immediately after construction, before this section can be reached), in
/// which case a disabled placeholder renders instead of a broken picker.
fn font_picker_row(title: impl Into<gpui::SharedString>, select: Option<&FontSelectState>,
                   size_pt: f32, colors: &ColorTokens, label_font_family: &str)
                   -> impl IntoElement + 'static {
    let row = div().flex()
                   .items_center()
                   .gap(px(12.0))
                   .child(row_label(title, colors, label_font_family));

    let Some(select) = select
    else {
        return row.child(div().text_sm().text_color(colors.text_tertiary).child("…"));
    };

    row.child(
        div().flex_1().child(
            Select::new(select)
                .menu_width(px(280.0))
                .placeholder(t!("settings.appearance_font_placeholder"))
                .search_placeholder(t!("settings.appearance_font_search_placeholder"))
                .w_full(),
        ),
    )
    .child(
        div()
            .w(px(48.0))
            .flex_none()
            .text_sm()
            .text_color(colors.text_tertiary)
            .text_align(gpui::TextAlign::Right)
            .child(format!("{size_pt:.0}pt")),
    )
}

/// Renders the shared "Text Scale" stepper row, matching the concurrency
/// stepper style already used in `settings_storage_view`.
///
/// The value is a decimal multiplier, shown and adjusted in tenths, where
/// `1.0` is the app's normal text scale (body fonts render at
/// [`BODY_FONT_SIZE_PT`], value fonts at `BODY_FONT_SIZE_PT *`
/// [`VALUE_FONT_SIZE_RATIO`], label/mono fonts at [`LABEL_MONO_FONT_SIZE_PT`]).
fn text_scale_row(scale: f32, entity: Entity<LibraryController>, colors: &ColorTokens,
                  label_font_family: &str)
                  -> impl IntoElement + 'static {
    let entity_dec = entity.clone();
    let entity_inc = entity;

    div().flex()
         .items_center()
         .gap(px(12.0))
         .child(row_label(t!("settings.appearance_text_size_label"),
                          colors,
                          label_font_family))
         .child(div().flex()
                     .items_center()
                     .gap(px(8.0))
                     .child(div().id("appearance-text-size-decrement")
                                 .flex_none()
                                 .size(px(28.0))
                                 .rounded(px(6.0))
                                 .border_1()
                                 .border_color(colors.border)
                                 .flex()
                                 .items_center()
                                 .justify_center()
                                 .cursor_pointer()
                                 .tooltip(|window, cx| {
                                     Tooltip::new(
                                t!("settings.appearance_text_size_decrement_tooltip").to_string(),
                            )
                            .build(window, cx)
                                 })
                                 .on_click(move |_, _, cx| {
                                     let next = (scale - UI_TEXT_SCALE_STEP).max(MIN_UI_TEXT_SCALE);
                                     entity_dec.update(cx, |ctrl, cx| {
                                         ctrl.set_ui_text_size(px(next * DEFAULT_UI_TEXT_SIZE), cx);
                                     });
                                 })
                                 .child(div().text_sm()
                                             .text_color(colors.text_primary)
                                             .child("\u{2212}")))
                     .child(div().w(px(48.0))
                                 .text_sm()
                                 .text_color(colors.text_primary)
                                 .text_align(gpui::TextAlign::Center)
                                 .child(format!("{scale:.1}")))
                     .child(div().id("appearance-text-size-increment")
                                 .flex_none()
                                 .size(px(28.0))
                                 .rounded(px(6.0))
                                 .border_1()
                                 .border_color(colors.border)
                                 .flex()
                                 .items_center()
                                 .justify_center()
                                 .cursor_pointer()
                                 .tooltip(|window, cx| {
                                     Tooltip::new(
                                t!("settings.appearance_text_size_increment_tooltip").to_string(),
                            )
                            .build(window, cx)
                                 })
                                 .on_click(move |_, _, cx| {
                                     let next = (scale + UI_TEXT_SCALE_STEP).min(MAX_UI_TEXT_SCALE);
                                     entity_inc.update(cx, |ctrl, cx| {
                                         ctrl.set_ui_text_size(px(next * DEFAULT_UI_TEXT_SIZE), cx);
                                     });
                                 })
                                 .child(div().text_sm()
                                             .text_color(colors.text_primary)
                                             .child("+"))))
}

/// The four Appearance-page font picker states, bundled into one struct
/// rather than threaded as separate function arguments (see `docs/rust.md`'s
/// guidance against many-argument functions).
pub struct AppearanceFontSelects {
    pub body:  Option<FontSelectState>,
    pub value: Option<FontSelectState>,
    pub label: Option<FontSelectState>,
    pub mono:  Option<FontSelectState>,
}

/// Renders the Appearance settings section: body/value/label/monospace font
/// pickers (searchable, listing every font installed on the user's system),
/// a shared text-scale stepper, and the active theme picker.
pub fn render_appearance_section(entity: Entity<LibraryController>, colors: &ColorTokens,
                                 theme: &LibriTheme, font_selects: AppearanceFontSelects)
                                 -> impl IntoElement + 'static + use<> {
    let AppearanceFontSelects { body: body_font_select,
                                value: value_font_select,
                                label: label_font_select,
                                mono: mono_font_select, } = font_selects;
    let text_primary = colors.text_primary;
    let border = colors.border;
    let theme_key = theme.key;
    let scale = theme.fonts.ui_text_size.as_f32() / DEFAULT_UI_TEXT_SIZE;
    let body_pt = BODY_FONT_SIZE_PT * scale;
    let value_pt = body_pt * VALUE_FONT_SIZE_RATIO;
    let label_mono_pt = LABEL_MONO_FONT_SIZE_PT * scale;
    let label_font_family = theme.fonts.label_font.to_string();

    let entity_theme = entity.clone();

    let theme_button = Button::new("appearance-theme").outline()
                                                      .label(theme_label(theme_key))
                                                      .dropdown_menu(move |menu, _, _| {
                                                          let mut m = menu;
                                                          for key in THEME_KEYS {
                                                              let e = entity_theme.clone();
                                                              m = m.item(
                    PopupMenuItem::new(theme_label(key))
                        .checked(key == theme_key)
                        .on_click(move |_, _, cx| {
                            e.update(cx, |ctrl, cx| ctrl.set_theme(key, cx));
                        }),
                );
                                                          }
                                                          m
                                                      });
    let theme_row = div().flex()
                         .items_center()
                         .gap(px(12.0))
                         .child(row_label(t!("settings.appearance_theme_label"),
                                          colors,
                                          &label_font_family))
                         .child(theme_button);

    div().flex()
         .flex_col()
         .gap(px(24.0))
         .p(px(24.0))
         .child(div().text_sm()
                     .font_weight(gpui::FontWeight::SEMIBOLD)
                     .text_color(text_primary)
                     .child(t!("settings.appearance_title")))
         .child(div().h(px(1.0)).bg(border))
         .child(font_picker_row(t!("settings.appearance_body_font_label"),
                                body_font_select.as_ref(),
                                body_pt,
                                colors,
                                &label_font_family))
         .child(font_picker_row(t!("settings.appearance_value_font_label"),
                                value_font_select.as_ref(),
                                value_pt,
                                colors,
                                &label_font_family))
         .child(font_picker_row(t!("settings.appearance_label_font_label"),
                                label_font_select.as_ref(),
                                label_mono_pt,
                                colors,
                                &label_font_family))
         .child(font_picker_row(t!("settings.appearance_mono_font_label"),
                                mono_font_select.as_ref(),
                                label_mono_pt,
                                colors,
                                &label_font_family))
         .child(text_scale_row(scale, entity, colors, &label_font_family))
         .child(div().h(px(1.0)).bg(border))
         .child(theme_row)
}
