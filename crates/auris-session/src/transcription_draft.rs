//! A recognizer-independent acceptance boundary for pitched and instrument-labeled drafts.

use crate::{MixtureNote, NoteTranscription, Session};

/// Immutable recognized events accepted only through an explicit document edit.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TranscriptionDraft {
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
