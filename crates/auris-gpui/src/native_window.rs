//! Native window creation with the application's backdrop and theme lifecycle.

use gpui::{App, Entity, Render, Window, WindowHandle, WindowOptions};

use crate::theme::Theme;

/// Opens a window with the native material behind its translucent surfaces.
pub(crate) fn open<V: 'static + Render>(
    cx: &mut App,
    mut options: WindowOptions,
    build_root_view: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> gpui::Result<WindowHandle<V>> {
    options.window_background = cx.global::<Theme>().atelier.window_background();
    cx.open_window(options, |window, cx| {
        let view = build_root_view(window, cx);
        apply_theme(window, cx);
        window.observe_global::<Theme>(cx, apply_theme).detach();
        view
    })
}

fn apply_theme(window: &mut Window, cx: &mut App) {
    let theme = &cx.global::<Theme>().atelier;
    if cfg!(test) {
        // The headless platform has no native handle for Atelier's OS backdrop.
        window.set_background_appearance(theme.window_background());
    } else {
        atelier::set_window_background(window, theme);
    }
}
