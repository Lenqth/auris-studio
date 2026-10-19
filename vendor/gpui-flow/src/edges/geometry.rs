//! One screen-space route for painting, labels and pointer hit testing.

use super::{bezier::get_bezier_path, find_handle_position, smooth_step::get_smooth_step_path};
use crate::{FlowEdge, FlowState, HandleType};

/// A route in window coordinates, with curves flattened to a quarter-pixel tolerance.
pub struct EdgePath {
    /// Consecutive points on the visible centerline, including both handle centers.
    pub points: Vec<(f32, f32)>,
    /// Label anchor on the same route.
    pub label: (f32, f32),
}

impl EdgePath {
    /// Distance to the actual painted segments, including points between curve samples.
    pub fn distance(&self, point: (f32, f32)) -> f32 {
        self.points
            .windows(2)
            .map(|pair| segment_distance(point, pair[0], pair[1]))
            .fold(f32::INFINITY, f32::min)
    }
}

/// Resolve an edge once, applying its lane offset in flow units before flattening.
pub fn compute_edge_path(state: &FlowState, edge: &FlowEdge) -> Option<EdgePath> {
    let source_node = state.get_node(&edge.source)?;
    let target_node = state.get_node(&edge.target)?;
    let source_position =
        find_handle_position(source_node, &edge.source_handle, HandleType::Source);
    let target_position =
        find_handle_position(target_node, &edge.target_handle, HandleType::Target);
    let source = state.find_handle_center(&edge.source, &edge.source_handle, source_position)?;
    let target = state.find_handle_center(&edge.target, &edge.target_handle, target_position)?;
    match edge.edge_type {
        crate::EdgeType::Bezier { curvature } => {
            let curve = get_bezier_path(
                source.0,
                source.1,
                source_position,
                target.0,
                target.1,
                target_position,
                curvature,
            )
            .with_lane_offset(edge.lane_offset * state.viewport.zoom * state.edge_lane_scale);
            let mut points = vec![curve.source];
            let mut pending = vec![(
                [
                    curve.source,
                    curve.source_control,
                    curve.target_control,
                    curve.target,
                ],
                0,
            )];
            while let Some((p, depth)) = pending.pop() {
                if depth >= 12
                    || (segment_distance(p[1], p[0], p[3]) <= 0.25
                        && segment_distance(p[2], p[0], p[3]) <= 0.25)
                {
                    points.push(p[3]);
                } else {
                    let a = midpoint(p[0], p[1]);
                    let b = midpoint(p[1], p[2]);
                    let c = midpoint(p[2], p[3]);
                    let d = midpoint(a, b);
                    let e = midpoint(b, c);
                    let f = midpoint(d, e);
                    pending.push(([f, e, c, p[3]], depth + 1));
                    pending.push(([p[0], a, d, f], depth + 1));
                }
            }
            Some(EdgePath {
                points,
                label: curve.point(0.5),
            })
        }
        crate::EdgeType::Straight => Some(EdgePath {
            points: vec![source, target],
            label: midpoint(source, target),
        }),
        crate::EdgeType::SmoothStep { offset, .. } => {
            let step = get_smooth_step_path(
                source.0,
                source.1,
                source_position,
                target.0,
                target.1,
                target_position,
                offset * state.viewport.zoom,
            );
            let label = polyline_midpoint(&step.points);
            Some(EdgePath {
                points: step.points,
                label,
            })
        }
    }
}

fn midpoint(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

fn polyline_midpoint(points: &[(f32, f32)]) -> (f32, f32) {
    let mut remaining = points
        .windows(2)
        .map(|pair| (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1))
        .sum::<f32>()
        / 2.0;
    for pair in points.windows(2) {
        let length = (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
        if length > f32::EPSILON && remaining <= length {
            let t = remaining / length;
            return (
                pair[0].0 + t * (pair[1].0 - pair[0].0),
                pair[0].1 + t * (pair[1].1 - pair[0].1),
            );
        }
        remaining -= length;
    }
    points.first().copied().unwrap_or_default()
}

fn segment_distance(point: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let square = dx * dx + dy * dy;
    let t = if square > f32::EPSILON {
        (((point.0 - a.0) * dx + (point.1 - a.1) * dy) / square).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (point.0 - a.0 - t * dx).hypot(point.1 - a.1 - t * dy)
}
