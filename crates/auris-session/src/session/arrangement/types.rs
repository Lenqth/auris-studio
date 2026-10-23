//! Bounded, typed arrangement proposals shared by every agent transport.

use crate::live_agent::{automation, performance::PerformancePatch};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Inspect a passage, validate an immutable proposal, or apply its exact prepared result.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    /// Read instrument capabilities, phrase conditions and diagnostics without editing.
    Inspect {
        /// One-based first bar.
        start_bar: u32,
        /// Passage length, 1..128 bars.
        bars: u32,
        /// Zero-based track page; at most sixteen tracks per response.
        #[serde(default)]
        offset: usize,
    },
    /// Prepare all changes on a snapshot. Audio copies and plugin loading run on a worker.
    Validate {
        /// Current content revision from inspect_project or arrangement inspection.
        expected_revision: String,
        /// One to sixteen changes, each with a musical purpose.
        #[schemars(length(min = 1, max = 16))]
        changes: Vec<Change>,
    },
    /// Commit a validated, unexpired proposal as one Undo edit. Exact retries are no-ops.
    Apply {
        /// Exact plan_id returned by validation; scoped to this document and process.
        plan_id: String,
        /// Original revision bound to that plan, including on an identical retry.
        expected_revision: String,
    },
}

impl Operation {
    /// Whether the operation changes the document.
    pub fn is_mutating(&self) -> bool {
        matches!(self, Self::Apply { .. })
    }

    /// Whether the operation accesses external files or commits a compound edit.
    pub fn requires_confirmation(&self) -> bool {
        match self {
            Self::Inspect { .. } => false,
            Self::Apply { .. } => true,
            Self::Validate { changes, .. } => changes.iter().any(|change| {
                matches!(
                    change.edit,
                    Action::AudioSfx { .. } | Action::Revoice { .. }
                )
            }),
        }
    }
}

/// One explicitly motivated edit; notes, harmony, lyrics and timeline maps are retained.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Change {
    /// Short musical purpose, for example "Lift the second chorus without changing the melody".
    pub purpose: String,
    /// Exact edit with stable document IDs and timeline ticks.
    pub edit: Action,
}

/// Edits supported by an atomic arrangement proposal.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Change only the clip's performance; authored notes and controllers remain intact.
    Performance {
        /// Existing MIDI clip ID.
        clip: u64,
        /// Exact drum writer name, or omit for the whole clip.
        drum_voice: Option<String>,
        /// Partial settings, as in set_performance.
        settings: Box<PerformancePatch>,
    },
    /// Shape one parameter inside a passage while retaining its exact curve outside it.
    Automation {
        /// Existing parameter address from automation/inspect_instrument/mixer_graph.
        target: automation::ControlTarget,
        /// Inclusive start boundary, in absolute ticks.
        start_tick: i64,
        /// Exclusive end boundary; both boundary values retain the original automation/static value.
        end_tick: i64,
        /// One to 128 unique points strictly inside the boundaries, in native units.
        #[schemars(length(min = 1, max = 128))]
        points: Vec<automation::Point>,
    },
    /// Move or layer complete section clips on a new sound, preserving the source track's mix/routing.
    Revoice {
        /// One to sixteen clips from the same instrument or drum track. Split at phrase boundaries first.
        #[schemars(length(min = 1, max = 16))]
        clips: Vec<u64>,
        /// Exact, current sound ID from search_instruments.
        sound_id: String,
        /// New track name; its clips retain their own names, positions, notes and curves.
        name: String,
        /// True layers a copy; false transfers these clips while retaining every other source clip.
        #[serde(default)]
        layer: bool,
        /// Optional performance adjustment applied to the new clips only, suitable for the new sound.
        settings: Option<Box<PerformancePatch>>,
    },
    /// Place an asset-free drum hit or bowed crescendo at a section boundary.
    Transition {
        /// Riser ends at at_tick; crash/impact begin there.
        transition: Transition,
        /// Nonnegative absolute boundary tick.
        at_tick: i64,
        /// Positive duration in ticks, up to sixteen quarter notes.
        duration_ticks: i64,
        /// New track name.
        name: String,
        /// Track gain in dB, -60..0.
        gain_db: f32,
    },
    /// Copy a local audio effect, align its start/peak/end, and prepare fades off the UI thread.
    AudioSfx {
        /// Absolute local file; at most 32 MiB and thirty seconds after decoding.
        path: String,
        /// Absolute tick for the selected alignment point.
        at_tick: i64,
        /// Which part of the source lands at at_tick.
        alignment: Alignment,
        /// New track and clip name.
        name: String,
        /// Clip gain in dB, -60..0.
        gain_db: f32,
        /// Source-time fade-in in milliseconds.
        fade_in_ms: f32,
        /// Source-time fade-out in milliseconds.
        fade_out_ms: f32,
    },
    /// Introduce or restore a rest without deleting the clip or its score.
    ClipMute {
        /// Existing MIDI or audio clip ID.
        clip: u64,
        /// Whether this clip is silent.
        muted: bool,
    },
}

/// Native transition sounds requiring no external assets.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    /// A sustained bowed note with a rising track-gain envelope, ending at the boundary.
    Riser,
    /// A cymbal attack beginning at the boundary.
    Crash,
    /// A kick and low tom beginning at the boundary.
    Impact,
}

/// Source position aligned to a musical boundary; audio retains its original playback speed.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    /// First source frame.
    Start,
    /// Largest absolute sample across all channels; first occurrence wins ties.
    Peak,
    /// Exclusive source end.
    End,
}
