//! A recognizer-independent acceptance boundary for pitched note drafts.

use crate::{NoteTranscription, Session};

/// Immutable recognized events accepted only through an explicit document edit.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TranscriptionDraft {
    /// Pitched notes with source timing, dynamics and declared provenance.
    Piano {
        /// Validated recognizer-independent note list.
        transcription: NoteTranscription,
    },
}

impl TranscriptionDraft {
    /// Validate before authorization/application, without certifying recognition accuracy.
    pub fn validate(&self) -> Result<(), String> {
        let Self::Piano { transcription } = self;
        transcription.validate().map_err(|e| e.to_string())
    }

    /// Number of potentially overlapping recognized events.
    pub fn note_count(&self) -> usize {
        let Self::Piano { transcription } = self;
        transcription.notes.len()
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
        let TranscriptionDraft::Piano { transcription } = draft;
        let (track, clip) = self
            .import_transcription(&transcription, start, &name)
            .map_err(|e| e.to_string())?;
        let created = serde_json::json!([{"track":track.0,"clip":clip.0}]);
        Ok(serde_json::json!({"created":created,"notes":count,"unsaved":self.is_dirty(),"revision":self.agent_document_revision()?,"status":"Imported a note draft. Review extra/missing notes, timing and playback instruments. Provenance claims do not establish musical accuracy."}).to_string())
    }
}
