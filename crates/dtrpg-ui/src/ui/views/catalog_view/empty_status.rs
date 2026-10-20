use super::*;

pub(super) fn render_no_matches_state(search_query: &str, text_color: Hsla)
                                      -> impl IntoElement + 'static {
    let hint = if search_query.is_empty() {
        t!("catalog.try_different_section")
    }
    else {
        t!("catalog.try_clear_search")
    };

    div().flex()
         .flex_col()
         .items_center()
         .justify_center()
         .h_full()
         .gap(px(12.0))
         .child(div().text_2xl().text_color(text_color).child("⊘"))
         .child(div().text_sm()
                     .text_color(text_color)
                     .child(t!("catalog.no_titles_match")))
         .child(div().text_xs().text_color(text_color).child(hint))
}

pub(super) fn render_library_empty_state(text_color: Hsla) -> impl IntoElement + 'static {
    div().flex()
         .flex_col()
         .items_center()
         .justify_center()
         .h_full()
         .gap(px(12.0))
         .child(div().text_2xl().text_color(text_color).child("⊘"))
         .child(div().text_sm()
                     .text_color(text_color)
                     .child(t!("catalog.library_empty")))
}

// ── Group header
// ──────────────────────────────────────────────────────────────

pub(super) fn render_group_header(title: &str, kind: GroupBy, count: usize,
                                  colors: &ColorTokens, entity: Entity<LibraryController>)
                                  -> AnyElement {
    let text_primary = colors.text_primary;
    let text_tertiary = colors.text_tertiary;
    let title = title.to_string();
    let publisher = title.clone();
    let header = div().id(SharedString::from(format!("group-header-{title}")))
                      .flex()
                      .items_center()
                      .gap(px(8.0))
                      .py(px(10.0))
                      .child(div().text_sm()
                                  .font_weight(FontWeight::SEMIBOLD)
                                  .text_color(text_primary)
                                  .child(title))
                      .child(div().text_xs()
                                  .text_color(text_tertiary)
                                  .child(count.to_string()));
    // "Download all" is for a publisher. A tag has nothing to offer here yet,
    // so its header has no menu rather than an empty one.
    if kind != GroupBy::Publisher {
        return header.into_any_element();
    }
    header.context_menu(move |menu, _, _| {
              let entity = entity.clone();
              let publisher = publisher.clone();
              menu.item(PopupMenuItem::new(t!("catalog.publisher_download_all")).on_click(
                  move |_, _, cx| {
                      entity.update(cx, |ctrl, cx| {
                                ctrl.request_download_all_for_publisher(&publisher, cx)
                            });
                  },
              ))
          })
          .into_any_element()
}

// ── Status glyph
// ──────────────────────────────────────────────────────────────

/// Renders the download-status glyph for `id`.
///
/// While `activity` is not idle the glyph is replaced by a spinner with its
/// own tooltip rather than showing a downloaded/cloud state that is about to
/// change or may be stale: [`ItemActivity::Downloading`] when one of the
/// entry's files is queued or downloading, [`ItemActivity::Verifying`] while
/// a file-presence check (`verify-downloaded-status-against-disk`) is in
/// flight for it.
pub(super) fn render_status(id: &Arc<str>, status: ItemStatus, offloaded: bool,
                            activity: ItemActivity, colors: &ColorTokens)
                            -> AnyElement {
    match activity {
        ItemActivity::Downloading => {
            return render_downloading_indicator(id, colors).into_any_element();
        }
        ItemActivity::Verifying => {
            return render_verifying_indicator(id, colors).into_any_element();
        }
        ItemActivity::Idle => {}
    }
    let accent = colors.accent;
    let text_tertiary = colors.text_tertiary;
    match status {
        ItemStatus::Downloaded if offloaded => div().text_xs()
                                                    .text_color(colors.warning_text)
                                                    .flex_none()
                                                    .child("☁")
                                                    .into_any_element(),
        ItemStatus::Downloaded => div().size(px(7.0))
                                       .rounded_full()
                                       .bg(accent)
                                       .flex_none()
                                       .into_any_element(),
        ItemStatus::Cloud => div().text_xs()
                                  .text_color(text_tertiary)
                                  .flex_none()
                                  .child("☁")
                                  .into_any_element(),
    }
}

/// Renders an accent-colored spinner with a "Downloading…" tooltip for a
/// catalog entry with a file queued or downloading (see
/// `LibraryController::is_downloading`). Accent-colored, unlike the muted
/// verifying spinner, so a download in progress stands out in the catalog.
pub(super) fn render_downloading_indicator(id: &Arc<str>, colors: &ColorTokens)
                                           -> impl IntoElement + 'static + use<> {
    let elem_id: Arc<str> = Arc::from(format!("downloading-{id}"));
    div().id(elem_id)
         .flex_none()
         .child(Spinner::new().with_size(Size::XSmall).color(colors.accent))
         .tooltip(|window, cx| {
             Tooltip::new(t!("detail.item_status_downloading").to_string()).build(window, cx)
         })
}

/// Renders a small tinted spinner with a "checking download status" tooltip
/// for a catalog entry whose file-presence verification is currently in
/// flight (`LibraryController::is_verifying_downloads`, see
/// `verify-downloaded-status-against-disk`). Distinct from
/// [`render_checking_indicator`]'s untinted spinner (a network-bound
/// availability check) so the two kinds of in-flight check are never
/// visually conflated.
pub(super) fn render_verifying_indicator(id: &Arc<str>, colors: &ColorTokens)
                                         -> impl IntoElement + 'static + use<> {
    let elem_id: Arc<str> = Arc::from(format!("verifying-{id}"));
    div().id(elem_id)
         .flex_none()
         .child(Spinner::new().with_size(Size::XSmall)
                              .color(colors.text_tertiary))
         .tooltip(|window, cx| {
             Tooltip::new(t!("detail.tooltip_verifying_download").to_string()).build(window, cx)
         })
}
