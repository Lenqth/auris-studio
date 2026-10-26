//! A recognizer-independent acceptance boundary for pitched and instrument-labeled drafts.

use crate::{MixtureNote, NoteTranscription, Session};

/// Immutable recognized events accepted only through an explicit document edit.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TranscriptionDraft {
    /// Versioned external multitrack notes with declared recognizer-independent provenance.
    Multitrack {
        /// Bounded source-time events; unsupported event types are rejected.
        transcription: MultitrackTranscription,
    },
    /// Pitched notes with source timing, dynamics and declared provenance.
    Piano {
        /// Validated recognizer-independent note list.
        transcription: NoteTranscription,
    },
    /// Optional noncommercial multi-instrument recognizer result.
    Mixture {
        /// Original source duration, 0..600 seconds.
        seconds: f64,
        /// Explicit instrument labels and overlapping events.
        notes: Vec<MixtureNote>,
        /// SHA-256 of the actual decoder file used.
        model_sha256: String,
        /// Original model checkpoint hash from the converter.
        checkpoint_sha256: String,
    },
}

impl TranscriptionDraft {
    /// Validate before authorization/application, without certifying recognition accuracy.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Multitrack { transcription } => transcription.validate(),
            Self::Piano { transcription } => transcription.validate().map_err(|e| e.to_string()),
            Self::Mixture {
                seconds,
                notes,
                model_sha256,
                checkpoint_sha256,
            } => {
                if !seconds.is_finite()
                    || !(0.0..=600.0).contains(seconds)
                    || *seconds == 0.0
                    || notes.is_empty()
                    || notes.len() > 200000
                    || [model_sha256, checkpoint_sha256]
                        .iter()
                        .any(|s| s.len() != 64 || !s.bytes().all(|c| c.is_ascii_hexdigit()))
                    || notes.iter().any(|n| {
                        n.pitch > 127
                            || !n.start.is_finite()
                            || !n.end.is_finite()
                            || n.start < 0.0
                            || n.end <= n.start
                            || n.end > *seconds
                            || n.instrument.is_empty()
                            || n.instrument.len() > 128
                    })
                {
                    return Err("Invalid mixture draft: bounded notes, duration, instruments and model hashes are required".into());
                }
                Ok(())
            }
        }
    }

    /// Number of potentially overlapping recognized events.
    pub fn note_count(&self) -> usize {
        match self {
            Self::Multitrack { transcription } => {
                transcription.tracks.iter().map(|t| t.notes.len()).sum()
            }
            Self::Piano { transcription } => transcription.notes.len(),
            Self::Mixture { notes, .. } => notes.len(),
        }
    }
}

impl Session {
    /// Add a complete recognized draft while retaining existing music and timeline maps.
    pub fn agent_import_draft(
        &mut self,
        draft: TranscriptionDraft,
        name: String,
        at_tick: i64,
    ) -> Result<String, String> {
        draft.validate()?;
        if at_tick < 0 || name.trim().is_empty() || name.len() > 256 {
            return Err("Use nonnegative at_tick and a nonempty name up to 256 bytes".into());
        }
        let count = draft.note_count();
        let start = crate::prelude::Ticks(at_tick);
        let created = match draft {
            TranscriptionDraft::Multitrack { transcription } => {
                return self.import_multitrack_transcription(&transcription, &name, at_tick, false);
            }
            TranscriptionDraft::Piano { transcription } => {
                let (track, clip) = self
                    .import_transcription(&transcription, start, &name)
                    .map_err(|e| e.to_string())?;
                serde_json::json!([{"track":track.0,"clip":clip.0}])
            }
            TranscriptionDraft::Mixture {
                seconds,
                notes,
                model_sha256,
                checkpoint_sha256,
            } => {
                let report = crate::MixtureAnalysis {
                    algorithm: "muscriptor-small-cpu",
                    model_license: "CC-BY-NC-4.0",
                    model_sha256,
                    checkpoint_sha256,
                    seconds,
                    notes,
                };
                let tracks = self
                    .create_mixture_tracks(&report, start)
                    .map_err(|e| e.to_string())?;
                serde_json::json!(
                    tracks
                        .iter()
                        .map(|track| serde_json::json!({"track":track.0}))
                        .collect::<Vec<_>>()
                )
            }
        };
        Ok(serde_json::json!({"created":created,"notes":count,"unsaved":self.is_dirty(),"revision":self.agent_document_revision()?,"status":"Imported a note draft. Review extra/missing notes, timing and playback instruments. Provenance claims do not establish musical accuracy."}).to_string())
    }
}

/// Declared source range; event times are absolute seconds in this source audio.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftAudioSource {
    /// SHA-256 of the original audio file bytes.
    pub audio_sha256: String,
    /// Inclusive source start, in seconds.
    pub start_seconds: f64,
    /// Exclusive source end, in seconds.
    pub end_seconds: f64,
}

/// Identifies the producer without attributing its claims to a bundled recognizer.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftRecognizer {
    /// Producer/recognizer identifier.
    pub name: String,
    /// Optional model identifier.
    pub model: Option<String>,
    /// Optional exact model artifact SHA-256.
    pub model_sha256: Option<String>,
}

/// A single potentially overlapping MIDI event in original source seconds.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftNote {
    /// Full MIDI pitch range, 0..127.
    pub pitch: u8,
    /// Inclusive onset in source seconds.
    pub start: f64,
    /// Exclusive end in source seconds.
    pub end: f64,
    /// Explicit velocity, 0..1.
    pub strength: f32,
    /// Optional declared confidence, 0..1.
    pub confidence: Option<f32>,
}

/// One external part; an absent instrument remains unknown in its stored provenance.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftTrack {
    /// Human-readable part name, 1..256 bytes.
    pub name: String,
    /// Declared instrument label, if recognized.
    pub instrument: Option<String>,
    /// Declared zero-based GM program, 0..127; does not select a playback library.
    pub program: Option<u8>,
    /// Optional musical role.
    pub role: Option<String>,
    /// True for a percussion part.
    pub drums: bool,
    /// Explicit playback sound ID from search_instruments. Omit to use a reported guide sound.
    pub sound_id: Option<String>,
    /// Bounded note list, preserving overlap and dynamics.
    pub notes: Vec<DraftNote>,
}

/// Version-one external note draft. Unknown fields, curves, tempo maps and lyrics are rejected.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MultitrackTranscription {
    /// Must be one. New incompatible schemas require explicit support.
    pub schema_version: u32,
    /// Original audio identity and selected range.
    pub source: DraftAudioSource,
    /// Declared recognizer/model identity.
    pub recognizer: DraftRecognizer,
    /// Up to 128 parts and 200000 notes in total.
    pub tracks: Vec<DraftTrack>,
    /// Declared recognition constraints, not executable instructions.
    #[serde(default)]
    pub limitations: Vec<String>,
    /// Optional declared provenance. Never establishes authenticity.
    #[serde(default)]
    pub provenance: std::collections::BTreeMap<String, String>,
}

impl MultitrackTranscription {
    /// Validate every event before preview, authorization or mutation.
    pub fn validate(&self) -> Result<(), String> {
        let hash = |s: &str| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit());
        let text = |s: &str| !s.trim().is_empty() && s.len() <= 256;
        let source = &self.source;
        let count: usize = self.tracks.iter().map(|t| t.notes.len()).sum();
        if self.schema_version != 1
            || !hash(&source.audio_sha256)
            || !source.start_seconds.is_finite()
            || !source.end_seconds.is_finite()
            || source.start_seconds < 0.0
            || source.end_seconds <= source.start_seconds
            || source.end_seconds - source.start_seconds > 600.0
            || source.end_seconds > 86_400.0
            || !text(&self.recognizer.name)
            || self.recognizer.model.as_deref().is_some_and(|s| !text(s))
            || self
                .recognizer
                .model_sha256
                .as_deref()
                .is_some_and(|s| !hash(s))
            || !(1..=128).contains(&self.tracks.len())
            || !(1..=200_000).contains(&count)
            || self.limitations.len() > 128
            || self.limitations.iter().any(|s| s.len() > 4096)
            || self.provenance.len() > 128
            || self
                .provenance
                .iter()
                .any(|(k, v)| !text(k) || v.len() > 4096)
        {
            return Err("Invalid multitrack schema, source, producer or size limits".into());
        }
        for track in &self.tracks {
            if !text(&track.name)
                || track.notes.is_empty()
                || track.program.is_some_and(|p| p > 127)
                || [&track.instrument, &track.role, &track.sound_id]
                    .into_iter()
                    .flatten()
                    .any(|s| !text(s))
                || track.notes.iter().any(|n| {
                    n.pitch > 127
                        || !n.start.is_finite()
                        || !n.end.is_finite()
                        || n.start < source.start_seconds
                        || n.end > source.end_seconds
                        || n.end <= n.start
                        || !(0.0..=1.0).contains(&n.strength)
                        || n.confidence.is_some_and(|c| !(0.0..=1.0).contains(&c))
                })
            {
                return Err("Invalid multitrack part or note: bounded MIDI pitch, source time, velocity and confidence required".into());
            }
        }
        Ok(())
    }
}
