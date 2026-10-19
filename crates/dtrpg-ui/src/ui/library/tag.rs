//! Tag colors and the pieces shared by everywhere a tag is shown: the colored
//! dot, and the menu that sets a tag's color.

use dtrpg_core::data::tag_color::TagColor;
use gpui::prelude::*;
use gpui::{Div, Entity, Hsla, div, hsla, px};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use rust_i18n::t;

use crate::controllers::library::LibraryController;

/// The shade of a tag color. These are fixed rather than themed, like Finder's
/// own tag colors, and chosen to read on both light and dark surfaces.
#[must_use]
pub fn tag_hsla(color: TagColor) -> Hsla {
    let (hue, saturation, lightness) = match color {
        TagColor::Gray => (0.0, 0.0, 0.55),
        TagColor::Green => (145.0, 0.40, 0.42),
        TagColor::Purple => (285.0, 0.30, 0.52),
        TagColor::Blue => (215.0, 0.55, 0.50),
        TagColor::Yellow => (42.0, 0.70, 0.48),
        TagColor::Red => (5.0, 0.62, 0.52),
        TagColor::Orange => (25.0, 0.78, 0.52),
    };
    hsla(hue / 360.0, saturation, lightness, 1.0)
}

/// A small round dot in a tag's color, or in `no_color` if the tag has none.
pub fn tag_dot(color: Option<TagColor>, no_color: Hsla) -> Div {
    div().flex_none()
         .size(px(8.0))
         .rounded_full()
         .bg(color.map_or(no_color, tag_hsla))
}

/// The name of a tag color, in the user's language.
#[must_use]
pub fn tag_color_label(color: TagColor) -> String {
    match color {
        TagColor::Gray => t!("tag_color.gray"),
        TagColor::Green => t!("tag_color.green"),
        TagColor::Purple => t!("tag_color.purple"),
        TagColor::Blue => t!("tag_color.blue"),
        TagColor::Yellow => t!("tag_color.yellow"),
        TagColor::Red => t!("tag_color.red"),
        TagColor::Orange => t!("tag_color.orange"),
    }.to_string()
}

/// Adds to `menu` an item for each color, checking `current`, and one that
/// clears the color. Choosing one sets the color of the tag called `tag`.
pub fn color_menu(menu: PopupMenu, controller: &Entity<LibraryController>, tag: &str,
                  current: Option<TagColor>)
                  -> PopupMenu {
    let mut menu = menu;
    for color in TagColor::ALL {
        let controller = controller.clone();
        let tag = tag.to_owned();
        menu = menu.item(PopupMenuItem::new(tag_color_label(color)).checked(current == Some(color))
                                                                   .on_click(move |_, _, cx| {
                                                                       controller.update(cx, |ctrl, cx| {
                                                                           ctrl.set_tag_color(&tag, Some(color), cx);
                                                                       });
                                                                   }));
    }
    let controller = controller.clone();
    let tag = tag.to_owned();
    menu.separator()
        .item(PopupMenuItem::new(t!("tag_color.none")).checked(current.is_none())
                                                      .on_click(move |_, _, cx| {
                                                          controller.update(cx, |ctrl, cx| {
                                                                        ctrl.set_tag_color(&tag,
                                                                                           None,
                                                                                           cx);
                                                                    });
                                                      }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_color_has_its_own_shade_and_a_name() {
        let shades: Vec<_> = TagColor::ALL.iter()
                                          .map(|c| format!("{:?}", tag_hsla(*c)))
                                          .collect();
        let mut unique = shades.clone();
        unique.sort();
        unique.dedup();

        assert_eq!(unique.len(), TagColor::ALL.len());
        for color in TagColor::ALL {
            assert!(!tag_color_label(color).is_empty());
            assert_ne!(tag_color_label(color), format!("tag_color.{color}"));
        }
    }
}
