use crate::types::HandlePosition;

/// Compute the control point offset based on distance and curvature.
fn calculate_control_offset(distance: f32, curvature: f32) -> f32 {
    if distance >= 0.0 {
        0.5 * distance
    } else {
        curvature * 25.0 * (-distance).sqrt()
    }
}

/// Get the control point for one side of the bezier curve.
fn get_control_with_curvature(
    pos: HandlePosition,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    c: f32,
) -> (f32, f32) {
    match pos {
        HandlePosition::Left => (x1 - calculate_control_offset(x1 - x2, c), y1),
        HandlePosition::Right => (x1 + calculate_control_offset(x2 - x1, c), y1),
        HandlePosition::Top => (x1, y1 - calculate_control_offset(y1 - y2, c)),
        HandlePosition::Bottom => (x1, y1 + calculate_control_offset(y2 - y1, c)),
    }
}

/// Result of bezier path computation.
pub struct BezierPath {
    pub source: (f32, f32),
    pub source_control: (f32, f32),
    pub target_control: (f32, f32),
    pub target: (f32, f32),
    pub label_x: f32,
    pub label_y: f32,
}

impl BezierPath {
    /// Separate a parallel lane while keeping its endpoints attached to the handles.
    pub fn with_lane_offset(mut self, offset: f32) -> Self {
        let dx = self.target.0 - self.source.0;
        let dy = self.target.1 - self.source.1;
        let length = dx.hypot(dy);
        let normal = if length > f32::EPSILON {
            (-dy / length, dx / length)
        } else {
            (0.0, 1.0)
        };
        for control in [&mut self.source_control, &mut self.target_control] {
            control.0 += normal.0 * offset;
            control.1 += normal.1 * offset;
        }
        let label = self.point(0.5);
        self.label_x = label.0;
        self.label_y = label.1;
        self
    }

    /// An exact point on this cubic, with `t` clamped to the interval 0..1.
    pub fn point(&self, t: f32) -> (f32, f32) {
        let t = t.clamp(0.0, 1.0);
        let u = 1.0 - t;
        let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        let points = [
            self.source,
            self.source_control,
            self.target_control,
            self.target,
        ];
        points
            .into_iter()
            .zip(weights)
            .fold((0.0, 0.0), |sum, (p, w)| (sum.0 + p.0 * w, sum.1 + p.1 * w))
    }
}

/// Compute a bezier edge path between two handle positions.
///
/// Ported from xyflow's `getBezierPath`.
pub fn get_bezier_path(
    source_x: f32,
    source_y: f32,
    source_position: HandlePosition,
    target_x: f32,
    target_y: f32,
    target_position: HandlePosition,
    curvature: f32,
) -> BezierPath {
    let (source_ctrl_x, source_ctrl_y) = get_control_with_curvature(
        source_position,
        source_x,
        source_y,
        target_x,
        target_y,
        curvature,
    );

    let (target_ctrl_x, target_ctrl_y) = get_control_with_curvature(
        target_position,
        target_x,
        target_y,
        source_x,
        source_y,
        curvature,
    );

    // Cubic bezier t=0.5 approximate center
    let label_x =
        source_x * 0.125 + source_ctrl_x * 0.375 + target_ctrl_x * 0.375 + target_x * 0.125;
    let label_y =
        source_y * 0.125 + source_ctrl_y * 0.375 + target_ctrl_y * 0.375 + target_y * 0.125;

    BezierPath {
        source: (source_x, source_y),
        source_control: (source_ctrl_x, source_ctrl_y),
        target_control: (target_ctrl_x, target_ctrl_y),
        target: (target_x, target_y),
        label_x,
        label_y,
    }
}
