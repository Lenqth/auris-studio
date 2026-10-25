//! Theme-aware window chrome shared by the project and its auxiliary windows.
//!
//! Only the dedicated, noninteractive regions are native drag targets. In GPUI 0.2 the
//! first matching window-control hitbox wins, so marking the whole bar as draggable would
//! turn a button inside it into a window drag on Windows.

use gpui::{
    App, Bounds, ClickEvent, CursorStyle, Decorations, Div, ElementId, IntoElement, PathBuilder,
    Pixels, ResizeEdge, SharedString, Stateful, TitlebarOptions, Window, WindowControlArea,
    WindowDecorations, WindowOptions, canvas, div, point, prelude::*, px,
};

use crate::theme::Theme;

/// Height of the application-drawn title bar, including its lower border.
pub const HEIGHT: Pixels = px(40.0);

const CONTROL_WIDTH: Pixels = px(46.0);
const TRAFFIC_LIGHT_INSET: Pixels = px(78.0);

/// Window defaults for application-drawn chrome on every desktop platform.
pub fn options(title: impl Into<SharedString>) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(12.0), px(13.0))),
        }),
        // `appears_transparent` only hides the native title bar on macOS and Windows.
        // Linux otherwise adds a second, server-drawn title bar above our own.
        window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
        ..Default::default()
    }
}

/// Resize targets for Linux windows whose frame is drawn by the application.
///
/// Append these last to a relatively positioned window root. Five-pixel edges and larger
/// corner targets leave the document's layout and window-space gesture coordinates unchanged.
/// The compositor can decline client decorations; in that case it still owns resizing.
pub fn resize_handles(window: &Window) -> Vec<Stateful<Div>> {
    if !cfg!(target_os = "linux") || window.is_fullscreen() || window.is_maximized() {
        return Vec::new();
    }
    let Decorations::Client { tiling } = window.window_decorations() else {
        return Vec::new();
    };
    let size = window.viewport_size();
    let edge = px(5.0);
    let corner = px(12.0);
    // Corners follow edges so their diagonal cursor and resize operation take precedence.
    [
        (
            ResizeEdge::Top,
            !tiling.top,
            px(0.0),
            px(0.0),
            size.width,
            edge,
        ),
        (
            ResizeEdge::Bottom,
            !tiling.bottom,
            px(0.0),
            size.height - edge,
            size.width,
            edge,
        ),
        (
            ResizeEdge::Left,
            !tiling.left,
            px(0.0),
            px(0.0),
            edge,
            size.height,
        ),
        (
            ResizeEdge::Right,
            !tiling.right,
            size.width - edge,
            px(0.0),
            edge,
            size.height,
        ),
        (
            ResizeEdge::TopLeft,
            !(tiling.top || tiling.left),
            px(0.0),
            px(0.0),
            corner,
            corner,
        ),
        (
            ResizeEdge::TopRight,
            !(tiling.top || tiling.right),
            size.width - corner,
            px(0.0),
            corner,
            corner,
        ),
        (
            ResizeEdge::BottomLeft,
            !(tiling.bottom || tiling.left),
            px(0.0),
            size.height - corner,
            corner,
            corner,
        ),
        (
            ResizeEdge::BottomRight,
            !(tiling.bottom || tiling.right),
            size.width - corner,
            size.height - corner,
            corner,
            corner,
        ),
    ]
    .into_iter()
    .enumerate()
    .filter(|(_, (_, enabled, ..))| *enabled)
    .map(|(index, (edge, _, x, y, width, height))| {
        let cursor = match edge {
            ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
            ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
            ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
            ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
        };
        div()
            .id(("window-resize", index))
            .absolute()
            .left(x)
            .top(y)
            .w(width)
            .h(height)
            .occlude()
            .cursor(cursor)
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                window.start_window_resize(edge);
            })
    })
    .collect()
}

/// Space occupied by the native macOS traffic lights.
pub fn traffic_light_inset(window: &Window) -> Pixels {
    if cfg!(target_os = "macos") && !window.is_fullscreen() {
        TRAFFIC_LIGHT_INSET
    } else {
        px(0.0)
    }
}

/// Space occupied by the application-drawn window controls.
pub fn controls_width(window: &Window) -> Pixels {
    if cfg!(target_os = "macos") || window.is_fullscreen() {
        px(0.0)
    } else {
        CONTROL_WIDTH * 3.0
    }
}

/// The top resize border, kept outside native drag and control hitboxes on Windows.
pub fn resize_inset(window: &Window) -> Pixels {
    if cfg!(target_os = "windows") && !window.is_maximized() && !window.is_fullscreen() {
        px(4.0)
    } else {
        px(0.0)
    }
}

/// A title-bar row, ready for the caller's document title, controls and drag regions.
///
/// Native traffic lights have their own reserved space. On other platforms the caller
/// appends [`controls`] after its content. The row itself does not capture window drags.
pub fn titlebar(window: &Window, theme: &Theme) -> Stateful<Div> {
    div()
        .id("window-titlebar")
        .debug_selector(|| "window-titlebar".into())
        // The resize strip must not let the root's automatic focus handler cancel its
        // native mouse-down action either. This does not consume event propagation.
        .occlude()
        .relative()
        .flex()
        .items_center()
        .h(HEIGHT)
        .w_full()
        .flex_shrink_0()
        .pl(traffic_light_inset(window))
        .pt(resize_inset(window))
        .border_b_1()
        .border_color(theme.border_subtle)
        .bg(theme.surface)
        .text_color(if window.is_window_active() {
            theme.text
        } else {
            theme.text_muted
        })
}

/// A native window-drag target containing only a label or empty space.
///
/// Keep interactive children alongside this region, rather than inside it. Windows
/// handles dragging, snapping and double-click maximization through the native hitbox;
/// macOS keeps its transparent native title bar and its double-click preference.
pub fn drag_region(id: impl Into<ElementId>) -> Stateful<Div> {
    let id = id.into();
    let selector = id.clone();
    div()
        .id(id)
        .debug_selector(move || selector.to_string())
        .flex()
        .items_center()
        .h_full()
        .min_w_0()
        // A focusable ancestor automatically prevents the mouse-down default while taking
        // focus. Windows then refuses the native caption drag. Exclude that ancestor's
        // hitbox without stopping event propagation to the operating system.
        .occlude()
        .window_control_area(WindowControlArea::Drag)
        .when(!cfg!(target_os = "windows"), |this| {
            this.on_mouse_down(gpui::MouseButton::Left, |event, window, _| {
                if window.is_fullscreen() {
                    return;
                }
                if event.click_count == 2 {
                    if cfg!(target_os = "macos") {
                        window.titlebar_double_click();
                    } else {
                        window.zoom_window();
                    }
                } else {
                    window.start_window_move();
                }
            })
        })
}

/// The minimize, maximize/restore and close controls for non-macOS windows.
///
/// Windows hitboxes preserve Snap Layouts and native maximize/restore behavior. Other
/// clicks invoke GPUI operations explicitly. `close` must supply the
/// same guarded close operation used by the window's `on_window_should_close` callback.
pub fn controls(
    window: &Window,
    theme: &Theme,
    close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let row = div().flex().h_full().flex_shrink_0();
    if controls_width(window) == px(0.0) {
        return row;
    }
    row.child(control(
        "window-minimize",
        WindowControlArea::Min,
        false,
        theme,
    ))
    .child(control(
        "window-maximize",
        WindowControlArea::Max,
        window.is_maximized(),
        theme,
    ))
    .child(
        control("window-close", WindowControlArea::Close, false, theme).on_click(
            move |event, window, cx| {
                cx.stop_propagation();
                close(event, window, cx);
            },
        ),
    )
}

fn control(
    id: &'static str,
    area: WindowControlArea,
    maximized: bool,
    theme: &Theme,
) -> Stateful<Div> {
    let is_close = area == WindowControlArea::Close;
    // GPUI 0.2's Windows `zoom_window` only maximizes. The native control handler is
    // what also restores, so leave its events unhandled and exclude ancestor focus.
    let native_maximize = cfg!(target_os = "windows") && area == WindowControlArea::Max;
    let hover_background = if is_close {
        theme.danger
    } else {
        theme.surface_hover
    };
    let hover_foreground = if is_close {
        theme.text_on(theme.danger)
    } else {
        theme.text
    };
    div()
        .id(id)
        .debug_selector(move || id.into())
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .w(CONTROL_WIDTH)
        .h_full()
        .flex_shrink_0()
        .text_color(theme.text_muted)
        .hover(move |this| this.bg(hover_background).text_color(hover_foreground))
        .active(move |this| this.bg(hover_background).text_color(hover_foreground))
        .when(cfg!(target_os = "windows"), |this| {
            this.window_control_area(area)
        })
        .when(!native_maximize, |this| {
            this.on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                // Let the click callback own this operation without native double activation.
                window.prevent_default();
                cx.stop_propagation();
            })
            .when(!is_close, |this| {
                this.on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if area == WindowControlArea::Min {
                        window.minimize_window();
                    } else {
                        window.zoom_window();
                    }
                })
            })
        })
        .child(control_icon(area, maximized))
}

fn control_icon(area: WindowControlArea, maximized: bool) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| paint_control_icon(bounds, area, maximized, window),
    )
    .size(px(12.0))
}

fn paint_control_icon(
    bounds: Bounds<Pixels>,
    area: WindowControlArea,
    maximized: bool,
    window: &mut Window,
) {
    let at = |x: f32, y: f32| bounds.origin + point(px(x), px(y));
    let mut path = PathBuilder::stroke(px(1.0));
    match area {
        WindowControlArea::Min => {
            path.move_to(at(1.0, 6.0));
            path.line_to(at(11.0, 6.0));
        }
        WindowControlArea::Max if maximized => {
            path.move_to(at(4.0, 1.0));
            path.line_to(at(11.0, 1.0));
            path.line_to(at(11.0, 8.0));
            path.move_to(at(1.0, 4.0));
            path.line_to(at(8.0, 4.0));
            path.line_to(at(8.0, 11.0));
            path.line_to(at(1.0, 11.0));
            path.close();
        }
        WindowControlArea::Max => {
            path.move_to(at(1.0, 1.0));
            path.line_to(at(11.0, 1.0));
            path.line_to(at(11.0, 11.0));
            path.line_to(at(1.0, 11.0));
            path.close();
        }
        WindowControlArea::Close => {
            path.move_to(at(1.0, 1.0));
            path.line_to(at(11.0, 11.0));
            path.move_to(at(11.0, 1.0));
            path.line_to(at(1.0, 11.0));
        }
        WindowControlArea::Drag => return,
    }
    if let Ok(path) = path.build() {
        let color = window.text_style().color;
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use crate::harness::{open, paint, press, release};

    #[gpui::test]
    fn a_caption_press_does_not_let_the_focusable_root_cancel_native_dragging(
        cx: &mut gpui::TestAppContext,
    ) {
        let (app, cx) = open(cx);
        if !cfg!(target_os = "windows") {
            // The headless platform cannot perform a native window move. Fullscreen keeps
            // the same caption occlusion while avoiding that unsupported OS operation.
            // Windows uses its native hitbox instead, so retain the normal-window case there.
            cx.update(|window, _| window.toggle_fullscreen());
            paint(&app, cx);
        }
        let caption = cx.debug_bounds("project-title").expect("caption is drawn");
        cx.update(|window, cx| {
            assert!(!app.read(cx).focus.is_focused(window));
        });
        press(cx, caption.center());
        cx.update(|window, cx| {
            assert!(
                !app.read(cx).focus.is_focused(window),
                "the root must not take focus and prevent the native mouse-down default"
            );
        });
        release(cx, caption.center());
    }
}
