//! Bounded local JSON loading for recognizer-independent note drafts.

use crate::{AnalysisControl, NoteTranscription, SessionError};
use std::{fs::File, io::Read, path::Path};

/// Read and validate a regular local JSON file on a cancellable worker.
///
/// Accepts at most 32 MiB and an absolute path. The caller authorizes file access before
/// calling this function and separately authorizes the exact resulting document command.
pub fn read_transcription_file(
    path: &Path,
    control: &AnalysisControl,
) -> Result<NoteTranscription, SessionError> {
    let draft: NoteTranscription = read_draft_file(path, control)?;
    draft
        .validate()
        .map_err(|e| SessionError::MusicAnalysis(e.to_string()))?;
    Ok(draft)
}

/// Read a versioned multitrack or legacy piano draft through the same bounded file gate.
pub fn read_external_transcription_file(
    path: &Path,
    control: &AnalysisControl,
) -> Result<crate::TranscriptionDraft, SessionError> {
    let value: serde_json::Value = read_draft_file(path, control)?;
    let failure = |e: serde_json::Error| SessionError::MusicAnalysis(e.to_string());
    let draft = if value.get("schema_version").is_some() {
        crate::TranscriptionDraft::Multitrack {
            transcription: serde_json::from_value(value).map_err(failure)?,
        }
    } else if value
        .get("kind")
        .is_some_and(|kind| kind == "piano" || kind == "multitrack")
    {
        serde_json::from_value(value).map_err(failure)?
    } else {
        crate::TranscriptionDraft::Piano {
            transcription: serde_json::from_value(value).map_err(failure)?,
        }
    };
    draft.validate().map_err(SessionError::MusicAnalysis)?;
    Ok(draft)
}

fn read_draft_file<T: serde::de::DeserializeOwned>(
    path: &Path,
    control: &AnalysisControl,
) -> Result<T, SessionError> {
    let failure = |message: &str| SessionError::MusicAnalysis(message.into());
    let check = || {
        if control.is_cancelled() {
            Err(failure("Transcription import cancelled"))
        } else {
            Ok(())
        }
    };
    check()?;
    let text = path.to_string_lossy();
    if !path.is_absolute()
        || text.len() > 4096
        || text.starts_with("\\\\")
        || text.starts_with("//")
    {
        return Err(failure(
            "transcription path must be an absolute local file path",
        ));
    }
    const LIMIT: u64 = 32 * 1024 * 1024;
    let file = File::open(path).map_err(|e| failure(&e.to_string()))?;
    let metadata = file.metadata().map_err(|e| failure(&e.to_string()))?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err(failure(
            "transcription JSON must be a regular file at most 32 MiB",
        ));
    }
    let mut file = file.take(LIMIT + 1);
    let mut bytes = Vec::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        check()?;
        let count = file
            .read(&mut buffer)
            .map_err(|e| failure(&e.to_string()))?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() as u64 > LIMIT {
            return Err(failure("transcription JSON exceeds 32 MiB"));
        }
    }
    let draft: T = serde_json::from_slice(&bytes).map_err(|e| failure(&e.to_string()))?;
    check()?;
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_file_loader_checks_schema_budget_and_cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.json");
        let valid = serde_json::json!({"source":{"method":"external"},"seconds":2,
            "notes":[{"pitch":60,"start":0,"end":1,"strength":0.65}]});
        std::fs::write(&path, valid.to_string()).unwrap();
        assert_eq!(
            read_transcription_file(&path, &AnalysisControl::default())
                .unwrap()
                .notes
                .len(),
            1
        );
        let cancelled = AnalysisControl::default();
        cancelled.cancel();
        assert!(
            read_transcription_file(&path, &cancelled)
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
        for invalid in [serde_json::json!({"unexpected":true}), {
            let mut value = valid.clone();
            value["notes"][0]["end"] = 3.into();
            value
        }] {
            std::fs::write(&path, invalid.to_string()).unwrap();
            assert!(read_transcription_file(&path, &AnalysisControl::default()).is_err());
        }
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(32 * 1024 * 1024 + 1).unwrap();
        assert!(
            read_transcription_file(&path, &AnalysisControl::default())
                .unwrap_err()
                .to_string()
                .contains("32 MiB")
        );
        assert!(
            read_transcription_file(Path::new("relative.json"), &AnalysisControl::default())
                .is_err()
        );
    }
}
