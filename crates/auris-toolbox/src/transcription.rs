//! Saved-project import of externally computed, recognizer-independent note drafts.

use super::*;

/// Read note JSON and add a new physical-piano track to a saved project.
pub mod import_transcription {
    use super::*;
    /// Stable saved-project tool name.
    pub const NAME: &str = "import_transcription";
    /// Model-facing saved-project contract.
    pub const DESCRIPTION: &str = "Import a local recognizer-independent piano transcription JSON into an existing project as a new track, preserving source timing and overlaps. path is an absolute local JSON file at most 32 MiB with source:{method, optional model/artifact_sha256/audio_sha256}, seconds, notes:[{pitch,start,end,strength}], optional limitations. Pitch 21..108, strength 0..1; finite source duration up to ten minutes and 1..200000 notes. Provenance is caller-declared, not recognition verification. Keeps existing music and tempo; saves with a checkpoint. Do not invent notes from names, BPM or chord estimates.";
    /// Source draft, project and insertion options.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Args {
        /// Existing project path or open project handle.
        pub project: String,
        /// Absolute local JSON file containing a complete note draft.
        pub path: String,
        /// New track and clip name; defaults to Transcription.
        pub name: Option<String>,
        /// Insertion beat; defaults to zero, must be within 0..1000000.
        pub at_beat: Option<f64>,
    }

    /// Validate and read before publication, then use the canonical session command.
    pub fn run(args: &Args) -> Result<String, String> {
        let control =
            auris_session::AnalysisControl::cancelled_by(cancellation::current().flag_handle());
        let transcription = auris_session::read_transcription_file(Path::new(&args.path), &control)
            .map_err(|e| e.to_string())?;
        let mut session = opened(&args.project)?;
        let result =
            session.agent_command(auris_session::live_agent::Command::ImportTranscription {
                transcription,
                name: args.name.clone().unwrap_or_else(|| "Transcription".into()),
                at_beat: args.at_beat.unwrap_or(0.0),
            })?;
        save_checkpointed(&mut session)?;
        let mut result: serde_json::Value =
            serde_json::from_str(&result).map_err(|e| e.to_string())?;
        result["unsaved"] = false.into();
        result["project"] = serde_json::to_value(session.path()).map_err(|e| e.to_string())?;
        result["status"] = "Added a piano note draft and saved with a checkpoint. Review missing/extra notes and timing.".into();
        Ok(result.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_import_preserves_music_and_checkpoints_and_rejects_invalid_drafts() {
        let folder = tempfile::tempdir().unwrap();
        let mut session = headless().unwrap();
        session.set_tempo_at(Ticks::ZERO, 90.0);
        session.add_default_instrument_track("Existing").unwrap();
        session.save_as(&folder.path().join("Cover.auris")).unwrap();
        let project = session.path().unwrap().display().to_string();
        let original = session.project().tracks[0].clone();
        let tempo_map = session.project().tempo_map.clone();
        let path = folder.path().join("notes.json");
        std::fs::write(&path, r#"{"source":{"method":"external"},"seconds":2,"notes":[{"pitch":60,"start":0.25,"end":1.5,"strength":0.65}],"limitations":"Uniform dynamics"}"#).unwrap();
        let args = import_transcription::Args {
            project: project.clone(),
            path: path.display().to_string(),
            name: None,
            at_beat: Some(4.0),
        };
        assert!(writes_project(
            import_transcription::NAME,
            &serde_json::json!({})
        ));
        let report: serde_json::Value =
            serde_json::from_str(&import_transcription::run(&args).unwrap()).unwrap();
        assert_eq!(report["unsaved"], false);
        assert_eq!(report["provenance_verified"], false);
        let loaded = opened(&project).unwrap();
        assert_eq!(loaded.project().tracks.len(), 2);
        assert_eq!(loaded.project().tracks[0], original);
        assert_eq!(loaded.project().tempo_map, tempo_map);
        assert!(!loaded.checkpoints().unwrap().is_empty());
        let before = std::fs::read(&project).unwrap();
        std::fs::write(&path, "{}").unwrap();
        assert!(import_transcription::run(&args).is_err());
        assert_eq!(std::fs::read(&project).unwrap(), before);
    }
}
