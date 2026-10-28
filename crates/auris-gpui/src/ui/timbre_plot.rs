//! Interaction state for Atelier's orthographic timbre scatter plot.

use gpui::{Bounds, Pixels, Point, point, px};
use std::{cell::Cell, rc::Rc};

pub(super) struct TimbreCamera {
    pub azimuth: f32,
    pub elevation: f32,
    pub preset: Option<usize>,
    pub bounds: Rc<Cell<Bounds<Pixels>>>,
    drag: Option<(Point<Pixels>, Point<Pixels>)>,
    moved: bool,
}

impl Default for TimbreCamera {
    fn default() -> Self {
        Self {
            azimuth: -37.5,
            elevation: 30.,
            preset: Some(0),
            bounds: Rc::default(),
            drag: None,
            moved: false,
        }
    }
}

impl TimbreCamera {
    pub fn set_view(&mut self, preset: usize) {
        (self.azimuth, self.elevation) = match preset {
            1 => (0., 90.),
            2 => (0., 0.),
            3 => (90., 0.),
            _ => (-37.5, 30.),
        };
        self.preset = Some(preset);
        self.drag = None;
    }

    pub fn turn(&mut self, azimuth: f32, elevation: f32) {
        self.azimuth = (self.azimuth + azimuth + 180.).rem_euclid(360.) - 180.;
        self.elevation = (self.elevation + elevation).clamp(-90., 90.);
        self.preset = None;
    }

    pub fn press(&mut self, position: Point<Pixels>) {
        self.drag = Some((position, position));
        self.moved = false;
    }

    pub fn drag(&mut self, position: Point<Pixels>, dragging: bool) -> bool {
        let Some((start, last)) = self.drag.filter(|_| dragging) else {
            self.drag = None;
            return false;
        };
        // Physical pointer tolerance separates picking a bead from rotating the plot.
        self.moved |= (position - start).magnitude() > 4.;
        if self.moved {
            let delta = position - last;
            self.turn(-f32::from(delta.x) * 0.4, f32::from(delta.y) * 0.4);
        }
        self.drag = Some((start, position));
        self.moved
    }

    pub fn release(&mut self) -> bool {
        self.drag.take().is_some() && !self.moved
    }

    /// Screen locations and depths matching the pinned Atelier Plot3d camera.
    /// Only picking is computed here; Atelier owns all plot drawing and axis layout.
    pub fn projected(&self, positions: &[[f32; 3]], extent: f32) -> Vec<(Point<Pixels>, f32)> {
        let bounds = self.bounds.get();
        let (sa, ca) = self.azimuth.to_radians().sin_cos();
        let (se, ce) = self.elevation.to_radians().sin_cos();
        let view = |[x, y, z]: [f32; 3]| {
            let away = -x * sa + y * ca;
            (x * ca + y * sa, z * ce + away * se, away * ce - z * se)
        };
        // Plot3d fits a box whose vertical aspect is 0.75, with a 44 px axis margin.
        let mut half = [0f32; 2];
        for x in [-1., 1.] {
            for y in [-1., 1.] {
                for z in [-0.75, 0.75] {
                    let (u, v, _) = view([x, y, z]);
                    half[0] = half[0].max(u.abs());
                    half[1] = half[1].max(v.abs());
                }
            }
        }
        let scale = ((f32::from(bounds.size.width) - 88.).max(1.) / (2. * half[0]))
            .min((f32::from(bounds.size.height) - 88.).max(1.) / (2. * half[1]));
        let center = bounds.center();
        positions
            .iter()
            .map(|p| {
                let (u, v, depth) = view([p[0] / extent, p[1] / extent, p[2] / extent * 0.75]);
                (
                    point(center.x + px(u * scale), center.y - px(v * scale)),
                    depth,
                )
            })
            .collect()
    }

    pub fn pick(&self, positions: &[[f32; 3]], extent: f32, at: Point<Pixels>) -> Option<usize> {
        self.projected(positions, extent)
            .iter()
            .enumerate()
            .filter(|(_, (screen, _))| (*screen - at).magnitude() <= 9.)
            .min_by(|(ia, (a, da)), (ib, (b, db))| {
                // Coincident points choose the front bead; the list reaches hidden sources.
                (*a - at)
                    .magnitude()
                    .partial_cmp(&(*b - at).magnitude())
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(da.total_cmp(db))
                    .then(ia.cmp(ib))
            })
            .map(|(i, _)| i)
    }
}

pub(super) fn plot_data(positions: &[[f64; 3]]) -> (Vec<[f32; 3]>, f32) {
    let points: Vec<_> = positions.iter().map(|p| p.map(|v| v as f32)).collect();
    let extent = points.iter().flatten().fold(1f32, |a, b| a.max(b.abs())) * 1.1;
    (points, extent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    #[test]
    fn picking_uses_depth_and_tracks_rotation_and_resize() {
        let mut camera = TimbreCamera::default();
        for dimensions in [size(px(420.), px(320.)), size(px(1000.), px(700.))] {
            camera
                .bounds
                .set(Bounds::new(point(px(40.), px(60.)), dimensions));
            for preset in 0..4 {
                camera.set_view(preset);
                let points = [[-0.8, -0.6, -0.4], [0.7, 0.5, 0.9]];
                for (i, (at, _)) in camera.projected(&points, 1.).iter().enumerate() {
                    assert_eq!(camera.pick(&points, 1., *at), Some(i));
                }
            }
        }
        camera.set_view(2);
        let points = [[0., 0.8, 0.], [0., -0.8, 0.]];
        assert_eq!(
            camera.pick(&points, 1., camera.bounds.get().center()),
            Some(1)
        );
    }

    #[test]
    fn clicks_do_not_rotate_and_drags_do_not_pick() {
        let mut camera = TimbreCamera::default();
        let start = point(px(200.), px(150.));
        camera.press(start);
        assert!(!camera.drag(start + point(px(1.), px(1.)), true));
        assert!(camera.release());
        assert_eq!(camera.preset, Some(0));
        camera.press(start);
        assert!(camera.drag(start + point(px(30.), px(20.)), true));
        assert!(!camera.release());
        assert_eq!(camera.preset, None);
    }
}
