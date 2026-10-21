//! Compact comparison data for the state first produced by composition.
use super::{ClipId, ClipRecipe, NoteTransform, PresetRef, SingerVoice, TrackId};
use crate::{AssetPath, Harmony, SectionMap, SignatureMap, TempoMap, Ticks};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Initial composition state used to explain subsequent edits, never to regenerate a score.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompositionBaseline {
    /// Original musical and bus tracks.
    pub tracks: Vec<CompositionTrackBaseline>,
    /// Original key and chord maps.
    pub harmony: Harmony,
    /// Original tempo map.
    pub tempo: TempoMap,
    /// Original meter map.
    pub meter: SignatureMap,
    /// Original section map.
    pub sections: SectionMap,
}

/// Source, controls and authored-clip fingerprints at composition time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompositionTrackBaseline {
    /// Stable track ID.
    pub id: TrackId,
    /// Original track name.
    pub name: String,
    /// Track kind label, used only for comparison.
    pub kind: String,
    /// Instrument identifier, empty on audio tracks and buses.
    pub instrument_id: String,
    /// Hosted instrument file, when selected.
    pub instrument_file: Option<AssetPath>,
    /// Selected SoundFont bank and patch, if any.
    pub preset: Option<PresetRef>,
    /// Selected singing voice, if any.
    pub voice: Option<SingerVoice>,
    /// Whether descriptor-backed controls could be read at capture time.
    pub parameters_available: bool,
    /// Static controls including the effective defaults at composition time.
    pub parameters: BTreeMap<String, f32>,
    /// Original authored clip state, with note content represented by a stable digest.
    pub clips: Vec<CompositionClipBaseline>,
}

/// Initial authored text identity and editable recipe of a note clip.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompositionClipBaseline {
    /// Stable clip ID.
    pub id: ClipId,
    /// Original clip name.
    pub name: String,
    /// Original song position.
    pub start: Ticks,
    /// Original clip length.
    pub length: Ticks,
    /// Stable digest of the authored notes, using `notes_digest`.
    pub notes_digest: u64,
    /// Original regeneration recipe, if any.
    pub recipe: Option<ClipRecipe>,
    /// Original non-destructive performance transforms.
    pub transforms: Vec<NoteTransform>,
}
