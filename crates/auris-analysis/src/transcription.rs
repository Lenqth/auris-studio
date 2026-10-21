//! Recognizer-independent piano-note drafts and explicitly claimed provenance.

use crate::{AnalysisError, audio::TranscribedNote};
use serde::{Deserialize, Serialize};

/// Caller-declared provenance, not a certificate of model identity or musical accuracy.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranscriptionSource {
    /// Nonempty method identifier, up to 128 bytes, e.g. `codex.harmonic-fit`.
    pub method: String,
    /// Optional model name, up to 256 bytes; omit for deterministic DSP.
    pub model: Option<String>,
    /// Optional SHA-256 of the actual model or analysis program used.
    pub artifact_sha256: Option<String>,
    /// Optional SHA-256 of the source audio bytes, as returned by attachment export.
    pub audio_sha256: Option<String>,
}

/// An estimated piano performance in source seconds, independent of any recognizer.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteTranscription {
    /// Explicitly claimed method, model and optional artifact/source hashes.
    pub source: TranscriptionSource,
    /// Finite original source duration, greater than zero and at most ten minutes.
    pub seconds: f64,
    /// One to 200,000 potentially overlapping events; pitch 21..108 and strength 0..1.
    pub notes: Vec<TranscribedNote>,
    /// Optional uncertainty or simplification disclosure, up to 4,096 bytes.
    pub limitations: Option<String>,
}

impl NoteTranscription {
    /// Validate bounds before any document mutation, without trusting provenance claims.
    pub fn validate(&self) -> Result<(), AnalysisError> {
        let hash = |value: &Option<String>| {
            value
                .as_ref()
                .is_none_or(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        };
        if self.source.method.trim().is_empty()
            || self.source.method.len() > 128
            || self
                .source
                .model
                .as_ref()
                .is_some_and(|s| s.trim().is_empty() || s.len() > 256)
            || !hash(&self.source.artifact_sha256)
            || !hash(&self.source.audio_sha256)
            || self.limitations.as_ref().is_some_and(|s| s.len() > 4096)
        {
            return Err(AnalysisError::Invalid(
                "invalid transcription provenance or limitations",
            ));
        }
        if !self.seconds.is_finite()
            || self.seconds <= 0.0
            || self.seconds > 600.0
            || self.notes.is_empty()
            || self.notes.len() > 200_000
            || self.notes.iter().any(|n| {
                !(21..=108).contains(&n.pitch)
                    || !n.start.is_finite()
                    || !n.end.is_finite()
                    || n.start < 0.0
                    || n.end <= n.start
                    || n.end > self.seconds
                    || !n.strength.is_finite()
                    || !(0.0..=1.0).contains(&n.strength)
            })
        {
            return Err(AnalysisError::Invalid(
                "invalid or empty piano transcription; check duration, pitch, time and strength bounds",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_provenance_does_not_need_a_bundled_model_but_invalid_events_are_refused() {
        let mut draft: NoteTranscription = serde_json::from_value(serde_json::json!({
            "source":{"method":"spectral-fit"}, "seconds":2.0,
            "notes":[{"pitch":60,"start":0.0,"end":1.0,"strength":0.7}]
        }))
        .unwrap();
        draft.validate().unwrap();
        for event in [
            TranscribedNote {
                pitch: 20,
                ..draft.notes[0].clone()
            },
            TranscribedNote {
                start: f64::NAN,
                ..draft.notes[0].clone()
            },
            TranscribedNote {
                end: 3.0,
                ..draft.notes[0].clone()
            },
            TranscribedNote {
                strength: f32::INFINITY,
                ..draft.notes[0].clone()
            },
        ] {
            let mut invalid = draft.clone();
            invalid.notes[0] = event;
            assert!(invalid.validate().is_err());
        }
        draft.source.artifact_sha256 = Some("not-a-hash".into());
        assert!(draft.validate().is_err());
        draft.source.artifact_sha256 = None;
        draft.notes.clear();
        assert!(draft.validate().is_err());
    }
}
