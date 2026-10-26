//! Shared read-only recognition and bounded immutable drafts for explicit acceptance.

use auris_session::{AnalysisControl, NoteTranscription, TranscriptionDraft, TranscriptionSource};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

/// One explicit recognition backend; local models are never downloaded implicitly.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Backend {
    /// Deterministic monophonic pitch tracking, best for one isolated pitched voice.
    Yin,
    /// Bundled Basic Pitch ONNX model, supports overlapping pitched notes.
    BasicPitch,
}

/// Recognition arguments are identical across every transport.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    /// Absolute local source audio path; supply exactly one of path or attachment.
    pub path: Option<String>,
    /// Conversation-local attachment_id; available only in a live conversation.
    pub attachment: Option<String>,
    /// Explicit recognition algorithm.
    pub backend: Backend,
}

/// Single public recognition entry point.
pub const NAME: &str = "transcribe_audio";
/// Shared model-facing instructions.
pub const DESCRIPTION: &str = "Recognize a local audio file or live attachment with an explicit backend: yin for isolated monophonic audio or basic_pitch for overlapping pitched notes. No automatic download or audio upload. Recognition never edits the project. Returns an immutable draft_id with bounded preview; import_transcription explicitly accepts it. Drafts expire after 30 minutes, restart or eviction (8 drafts, 64 MiB total). Review uncertainty and never claim faithful transcription from analysis alone.";

struct Entry {
    id: String,
    created: Instant,
    bytes: usize,
    draft: Arc<TranscriptionDraft>,
}
static DRAFTS: OnceLock<Mutex<Vec<Entry>>> = OnceLock::new();

impl Args {
    /// Validate explicit source selection before file access.
    pub fn validate(&self) -> Result<(), String> {
        if self.path.is_some() == self.attachment.is_some() {
            return Err("Supply exactly one of path or attachment".into());
        }
        if let Some(path) = &self.path {
            validate_path(path)?;
        }
        Ok(())
    }
}

fn validate_path(value: &str) -> Result<(), String> {
    if !Path::new(value).is_absolute()
        || value.len() > 4096
        || value.starts_with("\\\\")
        || value.starts_with("//")
    {
        return Err("Use an absolute local file path".into());
    }
    Ok(())
}

/// Retain validated events and publish an opaque reference with a small preview.
pub fn retain(draft: TranscriptionDraft) -> Result<String, String> {
    draft.validate()?;
    let TranscriptionDraft::Piano { transcription } = &draft;
    let preview = serde_json::json!({"source":transcription.source,"seconds":transcription.seconds,"notes":transcription.notes.iter().take(16).collect::<Vec<_>>(),"limitations":transcription.limitations});
    let bytes = serde_json::to_vec(&draft).map_err(|e| e.to_string())?.len();
    if bytes > 32 * 1024 * 1024 {
        return Err("Recognized draft exceeds 32 MiB".into());
    }
    let count = draft.note_count();
    let report: serde_json::Value =
        serde_json::from_str(&crate::reports::publish(&draft)?).map_err(|e| e.to_string())?;
    let id = auris_session::transient_id::transient_id("draft");
    let mut entries = DRAFTS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Draft cache lock failed")?;
    entries.retain(|entry| entry.created.elapsed() < Duration::from_secs(1800));
    while entries.len() >= 8
        || entries.iter().map(|e| e.bytes).sum::<usize>() + bytes > 64 * 1024 * 1024
    {
        entries.remove(0);
    }
    entries.push(Entry {
        id: id.clone(),
        created: Instant::now(),
        bytes,
        draft: Arc::new(draft),
    });
    Ok(serde_json::json!({"draft_id":id,"note_count":count,"preview":preview,"report":report,"document_unchanged":true,"next":"Review the draft, then explicitly accept with import_transcription using draft_id."}).to_string())
}

/// Resolve a retained immutable draft, refusing unknown or expired references.
pub fn resolve(id: &str) -> Result<Arc<TranscriptionDraft>, String> {
    let mut entries = DRAFTS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Draft cache lock failed")?;
    entries.retain(|entry| entry.created.elapsed() < Duration::from_secs(1800));
    entries
        .iter()
        .find(|e| e.id == id)
        .map(|e| Arc::clone(&e.draft))
        .ok_or_else(|| "Unknown or expired draft_id; run transcribe_audio again".into())
}

/// Run actual local inference on a caller-authorized immutable source reference.
pub fn recognize(
    path: &Path,
    backend: Backend,
    control: &AnalysisControl,
) -> Result<String, String> {
    let notes =
        |seconds, notes, method: &str, model, hash, limitations: &str| TranscriptionDraft::Piano {
            transcription: NoteTranscription {
                source: TranscriptionSource {
                    method: method.into(),
                    model,
                    artifact_sha256: hash,
                    audio_sha256: None,
                },
                seconds,
                notes,
                limitations: Some(limitations.into()),
            },
        };
    let draft = match backend {
        Backend::Yin => {
            let report = auris_session::analyze_audio_file(
                path,
                auris_session::AudioOptions { transcribe: true },
                control,
            )
            .map_err(|e| e.to_string())?;
            notes(
                report.seconds,
                report.notes,
                "yin",
                None,
                None,
                "Monophonic pitch estimates; overlapping voices and percussion are not separated.",
            )
        }
        Backend::BasicPitch => {
            let report =
                auris_session::transcribe_piano_file(path, control).map_err(|e| e.to_string())?;
            notes(
                report.seconds,
                report.notes,
                "basic_pitch",
                Some("bundled Basic Pitch".into()),
                Some(report.model_sha256),
                "Pitched note estimates; dense mixtures can produce extra/missing notes. Relative strengths are not original MIDI velocities.",
            )
        }
    };
    if control.is_cancelled() {
        return Err("Transcription cancelled".into());
    }
    retain(draft)
}

/// Saved-project runtime recognition does not require or open a project.
pub fn run(args: &Args) -> Result<String, String> {
    args.validate()?;
    let path = args
        .path
        .as_ref()
        .ok_or("Attachments require a live conversation; provide an absolute local path")?;
    let backend = args.backend.clone();
    recognize(
        Path::new(path),
        backend,
        &AnalysisControl::cancelled_by(crate::cancellation::current().flag_handle()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recognition_supports_only_yin_and_basic_pitch() {
        for kind in ["yin", "basic_pitch"] {
            assert!(serde_json::from_value::<Backend>(json!({"kind":kind})).is_ok());
        }
        assert!(serde_json::from_value::<Backend>(json!({"kind":"muscriptor"})).is_err());
    }

    #[test]
    fn recognition_never_accepts_write_destinations() {
        for extra in ["project", "apply", "midi_output", "at_beat"] {
            let mut args = json!({"path":"missing.wav","backend":{"kind":"yin"}});
            args[extra] = true.into();
            assert!(serde_json::from_value::<Args>(args).is_err(), "{extra}");
        }
        assert!(resolve("invented").is_err());
    }
}
