//! Distance-based highlights on the same flattened centerline used by wire hit testing.

use super::EdgePath;

/// Short source-to-target sections with increasing brightness towards each highlight's head.
///
/// `phase` wraps at one. `spacing` and `length` are screen-space distances; applying viewport
/// scale to both keeps motion stable while panning and zooming. Degenerate or invalid routes
/// produce no highlights. Extremely long routes are limited to 128 highlights per paint.
pub fn flowing_edge_segments(
    path: &EdgePath,
    phase: f32,
    spacing: f32,
    length: f32,
) -> Vec<(EdgePath, f32)> {
    if !phase.is_finite()
        || !spacing.is_finite()
        || !length.is_finite()
        || spacing <= 0.0
        || length <= 0.0
    {
        return Vec::new();
    }
    let distances: Vec<_> = path
        .points
        .windows(2)
        .map(|pair| (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1))
        .collect();
    let total: f32 = distances.iter().sum();
    if !total.is_finite() || total <= f32::EPSILON {
        return Vec::new();
    }
    let spacing = spacing.max(total / 127.0);
    let length = length.min(spacing * 0.5);
    let offset = phase.rem_euclid(1.0) * spacing;
    let count = (((total + length - offset) / spacing).ceil().max(0.0) as usize).min(128);
    let mut segments = Vec::new();
    for index in 0..count {
        let head = offset + index as f32 * spacing;
        for (part, strength) in [0.2, 0.5, 0.9].into_iter().enumerate() {
            let start = (head - length + part as f32 * length / 3.0).max(0.0);
            let end = (head - length + (part + 1) as f32 * length / 3.0).min(total);
            if start >= end {
                continue;
            }
            let mut points = Vec::new();
            let mut at = 0.0;
            for (pair, &distance) in path.points.windows(2).zip(&distances) {
                let next = at + distance;
                if distance > f32::EPSILON && next > start && at < end {
                    let from = ((start - at) / distance).max(0.0);
                    let to = ((end - at) / distance).min(1.0);
                    let interpolate = |t: f32| {
                        (
                            pair[0].0 + (pair[1].0 - pair[0].0) * t,
                            pair[0].1 + (pair[1].1 - pair[0].1) * t,
                        )
                    };
                    if points.is_empty() {
                        points.push(interpolate(from));
                    }
                    points.push(interpolate(to));
                }
                at = next;
                if at >= end {
                    break;
                }
            }
            if points.len() >= 2 {
                segments.push((
                    EdgePath {
                        points,
                        label: (0.0, 0.0),
                    },
                    strength,
                ));
            }
        }
    }
    segments
}
