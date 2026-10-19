use crate::PixelsExt;
pub mod bezier;
mod flow;
mod geometry;
pub mod smooth_step;
pub mod straight;

use crate::{EdgeId, EdgeStroke, FlowEdge, FlowNode, FlowState, HandlePosition, HandleType};
pub use flow::flowing_edge_segments;
pub use geometry::{EdgePath, compute_edge_path};
use gpui::{Background, PathBuilder, Point, SharedString, Window, px};

const ARROW_SIZE: f32 = 6.0;

/// Paint edges using the same flattened routes as labels and hit testing.
pub fn paint_edges(state: &FlowState, window: &mut Window) {
    let size = window.viewport_size();
    let margin = 100.0;
    for edge in &state.edges {
        if edge.hidden {
            continue;
        }
        let Some(path) = compute_edge_path(state, edge) else {
            continue;
        };
        if path.points.iter().all(|p| p.0 < -margin)
            || path
                .points
                .iter()
                .all(|p| p.0 > size.width.as_f32() + margin)
            || path.points.iter().all(|p| p.1 < -margin)
            || path
                .points
                .iter()
                .all(|p| p.1 > size.height.as_f32() + margin)
        {
            continue;
        }
        let color: Background = gpui::rgb(edge.color.unwrap_or(0xb1b1b7)).into();
        let width = edge.stroke_width.unwrap_or(2.0);
        let activity = if edge.activity.is_finite() {
            edge.activity.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if activity > 0.0 {
            // The existing role hue and dash pattern carry through the glow. Selection is
            // painted afterwards so a live connection still has an unambiguous focus outline.
            for (spread, opacity) in [(10.0, 0.06), (6.0, 0.12), (3.0, 0.22)] {
                let mut glow: gpui::Hsla = gpui::rgb(edge.color.unwrap_or(0xb1b1b7)).into();
                glow.a = activity * opacity;
                paint_edge_path(
                    &path,
                    edge.stroke,
                    width + spread * activity,
                    glow.into(),
                    window,
                );
            }
        }
        if edge.selected {
            paint_edge_path(
                &path,
                edge.stroke,
                width + 3.0,
                gpui::rgb(state.edge_selection_color.unwrap_or(0x555555)).into(),
                window,
            );
        }
        paint_edge_path(&path, edge.stroke, width + activity, color.clone(), window);
        if edge.animated
            && activity > 0.0
            && let Some(flow) = state.edge_flow
        {
            let scale = state.viewport.zoom * state.edge_lane_scale;
            let role = gpui::rgb(edge.color.unwrap_or(0xb1b1b7));
            let foreground = gpui::rgb(flow.color);
            for (segment, strength) in
                flowing_edge_segments(&path, flow.phase, flow.spacing * scale, flow.length * scale)
            {
                let highlight = gpui::Rgba {
                    r: role.r * 0.35 + foreground.r * 0.65,
                    g: role.g * 0.35 + foreground.g * 0.65,
                    b: role.b * 0.35 + foreground.b * 0.65,
                    a: strength * activity,
                };
                paint_edge_path(
                    &segment,
                    EdgeStroke::Solid,
                    width + activity,
                    highlight.into(),
                    window,
                );
            }
        }
        if let Some(pair) = path.points.get(path.points.len().saturating_sub(2)..) {
            if pair.len() == 2 {
                paint_arrowhead(window, pair[1].0, pair[1].1, pair[0].0, pair[0].1, color);
            }
        }
    }
}

/// Paint a solid, dashed or dotted route; patterns continue across segment boundaries.
pub fn paint_edge_path(
    path: &EdgePath,
    stroke: EdgeStroke,
    width: f32,
    color: Background,
    window: &mut Window,
) {
    let Some(first) = path.points.first() else {
        return;
    };
    let mut builder = PathBuilder::stroke(px(width));
    builder.move_to(Point::new(px(first.0), px(first.1)));
    let pattern: Option<(f32, f32)> = match stroke {
        EdgeStroke::Solid => None,
        EdgeStroke::Dashed => Some((8.0, 5.0)),
        EdgeStroke::Dotted => Some((2.0, 5.0)),
    };
    if let Some((dash, gap)) = pattern {
        let mut remaining = dash;
        let mut drawing = true;
        for pair in path.points.windows(2) {
            let dx = pair[1].0 - pair[0].0;
            let dy = pair[1].1 - pair[0].1;
            let length = dx.hypot(dy);
            let mut at = 0.0;
            while at < length {
                let step = remaining.min(length - at);
                let next = at + step;
                if next <= at {
                    break;
                }
                at = next;
                remaining -= step;
                let point = Point::new(
                    px(pair[0].0 + dx * at / length),
                    px(pair[0].1 + dy * at / length),
                );
                if drawing {
                    builder.line_to(point);
                } else {
                    builder.move_to(point);
                }
                if remaining <= f32::EPSILON {
                    drawing = !drawing;
                    remaining = if drawing { dash } else { gap };
                }
            }
        }
    } else {
        for point in path.points.iter().skip(1) {
            builder.line_to(Point::new(px(point.0), px(point.1)));
        }
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// Resolve the label anchor from the visible route.
pub fn compute_edge_label_position(state: &FlowState, edge: &FlowEdge) -> Option<(f32, f32)> {
    Some(compute_edge_path(state, edge)?.label)
}

/// Choose the nearest selectable route within the pointer tolerance, with painted order breaking ties.
pub fn hit_test_edges(state: &FlowState, mx: f32, my: f32, threshold: f32) -> Option<EdgeId> {
    state
        .edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| !edge.hidden && edge.selectable)
        .filter_map(|(index, edge)| {
            let distance = compute_edge_path(state, edge)?.distance((mx, my));
            (distance <= threshold).then_some((distance, index, edge.id.clone()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.1.cmp(&a.1)))
        .map(|(_, _, id)| id)
}
/// Paint a filled triangle arrowhead pointing from (from_x, from_y) toward (tip_x, tip_y).
fn paint_arrowhead(
    window: &mut Window,
    tip_x: f32,
    tip_y: f32,
    from_x: f32,
    from_y: f32,
    color: Background,
) {
    let dx = tip_x - from_x;
    let dy = tip_y - from_y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return;
    }

    // Unit vector along the arrow direction
    let ux = dx / len;
    let uy = dy / len;

    // Perpendicular
    let px = -uy;
    let py = ux;

    let half_width = ARROW_SIZE * 0.5;

    // Base points of the triangle
    let base_x = tip_x - ux * ARROW_SIZE;
    let base_y = tip_y - uy * ARROW_SIZE;

    let p1_x = base_x + px * half_width;
    let p1_y = base_y + py * half_width;
    let p2_x = base_x - px * half_width;
    let p2_y = base_y - py * half_width;

    let mut builder = PathBuilder::fill();
    builder.move_to(Point::new(gpui::px(tip_x), gpui::px(tip_y)));
    builder.line_to(Point::new(gpui::px(p1_x), gpui::px(p1_y)));
    builder.line_to(Point::new(gpui::px(p2_x), gpui::px(p2_y)));
    builder.line_to(Point::new(gpui::px(tip_x), gpui::px(tip_y)));

    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// Find the handle position for a given node and handle type.
fn find_handle_position(
    node: &FlowNode,
    handle_id: &Option<SharedString>,
    handle_type: HandleType,
) -> HandlePosition {
    // Try exact match first
    if let Some(id) = handle_id {
        if let Some(def) = node.handles.iter().find(|h| h.id.as_ref() == Some(id)) {
            return def.position;
        }
    }

    // Fall back to first handle of the right type
    node.handles
        .iter()
        .find(|h| h.handle_type == handle_type && h.id.is_none())
        .map(|h| h.position)
        .unwrap_or(match handle_type {
            HandleType::Source => HandlePosition::Right,
            HandleType::Target => HandlePosition::Left,
        })
}
