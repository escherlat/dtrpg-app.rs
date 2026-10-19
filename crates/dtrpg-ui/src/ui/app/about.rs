//! About-window creation.

use super::*;

pub fn open_about_window(cx: &mut App) -> WindowHandle<Root> {
    let window_size = Size { width:  px(320.),
                             height: px(260.), };
    let (window, _view) = gpui_kit::open_window(
        WindowOptions {
            // Same chrome pattern as `open_settings_window`: hide the native
            // title text but keep the macOS traffic lights, and disable
            // minimize/resize since the content is fixed and doesn't scroll.
            titlebar: Some(TitlebarOptions {
                appears_transparent: true,
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(window_size, cx)),
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        },
        cx,
        move |window, cx| cx.new(|cx| AboutWindowView::new(window, cx)),
    )
    .expect("failed to open about window");
    // See `open_library_window`'s matching comment: `gpui_kit::open_window`
    // wraps the view in a `Root`, so the window's root entity type is always
    // `Root`.
    window.downcast::<Root>()
          .expect("open_window's root entity is always Root")
}
