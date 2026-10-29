//! Data preparation for the Atelier 3-D timbre plot.
//!
//! Plot projection, picking, depth ordering, and orbit gestures belong to [`atelier::plot::Plot3d`].
//! This module only converts session coordinates and keeps the view angles used by the controls.

/// The current view angles and selected preset for the timbre map.
#[derive(Clone, Copy, Debug)]
pub(super) struct TimbreCamera {
    pub azimuth: f32,
    pub elevation: f32,
    pub preset: Option<usize>,
}

impl Default for TimbreCamera {
    fn default() -> Self {
        Self {
            azimuth: -37.5,
            elevation: 30.,
            preset: Some(0),
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
    }

    pub fn turn(&mut self, azimuth: f32, elevation: f32) {
        self.azimuth = (self.azimuth + azimuth + 180.).rem_euclid(360.) - 180.;
        self.elevation = (self.elevation + elevation).clamp(-90., 90.);
        self.preset = None;
    }
}

pub(super) fn plot_data(positions: &[[f64; 3]]) -> (Vec<[f32; 3]>, f32) {
    let points: Vec<_> = positions.iter().map(|p| p.map(|v| v as f32)).collect();
    let extent = points.iter().flatten().fold(1f32, |a, b| a.max(b.abs())) * 1.1;
    (points, extent)
}
