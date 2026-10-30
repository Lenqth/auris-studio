//! Data preparation for the Atelier 3-D timbre plot.
//!
//! Plot projection, picking, depth ordering, and orbit gestures belong to [`atelier::plot::Plot3d`].
//! This module only converts session coordinates and keeps the view angles used by the controls.

use atelier::plot::ViewPreset;

/// The current view angles and selected preset for the timbre map.
#[derive(Clone, Copy, Debug)]
pub(super) struct TimbreView {
    pub azimuth: f32,
    pub elevation: f32,
    pub preset: Option<usize>,
}

/// The views the segmented control offers, in its order: 3-D, top, front, side. The angles are
/// Atelier's, so a preset here is exactly the view [`ViewPreset`] names.
pub(super) const VIEWS: [ViewPreset; 4] = [
    ViewPreset::Oblique,
    ViewPreset::Top,
    ViewPreset::Front,
    ViewPreset::Side,
];

impl Default for TimbreView {
    fn default() -> Self {
        let (azimuth, elevation) = VIEWS[0].angles();
        Self {
            azimuth,
            elevation,
            preset: Some(0),
        }
    }
}

impl TimbreView {
    pub fn set_view(&mut self, preset: usize) {
        let preset = preset.min(VIEWS.len() - 1);
        (self.azimuth, self.elevation) = VIEWS[preset].angles();
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
