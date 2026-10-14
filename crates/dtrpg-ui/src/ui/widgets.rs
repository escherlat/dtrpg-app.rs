//! Small reusable rendering helpers shared across views.

use gpui::{ElementId, SharedString};
use gpui_kit::component::text::TextView;

use crate::util::text::escape_markdown;

/// Renders `text` as selectable, copyable plain text via `gpui-kit`'s
/// `TextView`.
///
/// The text is Markdown-escaped first — `TextView` parses its input as
/// Markdown, and raw catalog/account data can contain characters (`*`, `_`,
/// `#`, ...) that would otherwise be misread as formatting. `id` must be
/// unique among sibling elements, same as any other `gpui` element id.
pub fn selectable_text(id: impl Into<ElementId>, text: impl Into<SharedString>) -> TextView {
    let escaped = escape_markdown(text.into().as_ref());
    TextView::markdown(id, escaped).selectable(true)
}

/// Uppercases a field/section label.
///
/// `gpui` has no `font-variant: small-caps`, and synthesizing it by rendering
/// two run sizes in one row put mismatched-height text on the same line
/// (baselines didn't line up). Plain uppercase gets the "this is a label, not
/// a value" visual distinction without that defect.
pub fn small_caps_text(text: impl Into<SharedString>) -> String {
    text.into().to_uppercase()
}
