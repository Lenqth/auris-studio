//! Toolkit-independent presentation of a project's mixer graph.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A node's position in graph coordinates, before zoom and canvas translation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MixerNodePosition {
    /// Horizontal graph coordinate.
    pub x: f32,
    /// Vertical graph coordinate.
    pub y: f32,
}

/// Project-local mixer layout and camera; independent of routing and audio processing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MixerGraphView {
    /// Positions keyed by stable mixer identities (`track:master`, `track:ID`, `effect:ID`).
    pub nodes: BTreeMap<String, MixerNodePosition>,
    /// Display scale, where 1 is 100%.
    pub zoom: f32,
    /// Horizontal canvas translation in logical pixels, excluding the panel's window origin.
    pub pan_x: f32,
    /// Vertical canvas translation in logical pixels, excluding the panel's window origin.
    pub pan_y: f32,
}

impl Default for MixerGraphView {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

impl MixerGraphView {
    /// Discards unusable positions and replaces invalid camera values with their defaults.
    pub fn sanitized(mut self) -> Self {
        self.nodes
            .retain(|_, position| position.x.is_finite() && position.y.is_finite());
        if !self.zoom.is_finite() || self.zoom <= 0.0 {
            self.zoom = 1.0;
        }
        if !self.pan_x.is_finite() {
            self.pan_x = 0.0;
        }
        if !self.pan_y.is_finite() {
            self.pan_y = 0.0;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;

    #[test]
    fn mixer_view_round_trips_and_older_projects_use_the_default() {
        let project = Project {
            mixer_graph_view: MixerGraphView {
                nodes: [(
                    "track:master".into(),
                    MixerNodePosition {
                        x: -180.0,
                        y: 900.0,
                    },
                )]
                .into(),
                zoom: 0.025,
                pan_x: -230.0,
                pan_y: 17.0,
            },
            ..Project::default()
        };
        let mut json = serde_json::to_value(&project).unwrap();
        assert_eq!(
            serde_json::from_value::<Project>(json.clone()).unwrap(),
            project
        );
        json.as_object_mut().unwrap().remove("mixer_graph_view");
        let legacy: Project = serde_json::from_value(json).unwrap();
        assert_eq!(legacy.mixer_graph_view, MixerGraphView::default());
    }

    #[test]
    fn invalid_camera_and_positions_do_not_reach_the_canvas() {
        let view = MixerGraphView {
            nodes: [
                (
                    "track:master".into(),
                    MixerNodePosition {
                        x: -180.0,
                        y: 900.0,
                    },
                ),
                (
                    "effect:2".into(),
                    MixerNodePosition {
                        x: f32::NAN,
                        y: 0.0,
                    },
                ),
            ]
            .into(),
            zoom: -1.0,
            pan_x: f32::INFINITY,
            pan_y: f32::NEG_INFINITY,
        }
        .sanitized();
        assert_eq!(view.zoom, 1.0);
        assert_eq!((view.pan_x, view.pan_y), (0.0, 0.0));
        assert_eq!(view.nodes.len(), 1);
        assert!(view.nodes.contains_key("track:master"));
        assert!(serde_json::to_string(&view).is_ok());
    }
}
