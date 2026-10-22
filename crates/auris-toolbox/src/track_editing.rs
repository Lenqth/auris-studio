//! Saved instrument-to-audio conversion.

use super::*;

/// Replaces a software instrument, drum or singer track with its rendered audio.
pub mod convert_track_to_audio {
    use super::*;

    /// The tool's wire name.
    pub const NAME: &str = "convert_track_to_audio";
    /// The model-facing description.
    pub const DESCRIPTION: &str = "Renders an instrument, drum or singer track and replaces it with an audio track at the same position, preserving its ID, mixer, effects and routing. Instrument automation is baked; mixer automation stays editable. A singer uses its current take or generates a fresh one through its chosen voice. Saves with a checkpoint so the original score can be restored. Refuses audio tracks, buses, empty tracks and unavailable sounds.";

    /// One track to convert in an existing project.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Args {
        /// Absolute project path.
        pub project: String,
        /// Stable numeric track ID from inspect_project.
        pub track: u64,
    }

    /// Converts the track and checkpoints the previous saved score.
    pub fn run(args: &Args) -> Result<String, String> {
        let mut session = opened(&args.project)?;
        let track = TrackId(args.track);
        let cancellation = cancellation::current();
        let conversion = session
            .convert_track_job(track)
            .and_then(|job| job.render(&mut |_| {}, cancellation.flag()))
            .map_err(|error| error.to_string())?;
        cancellation.begin_commit()?;
        let clip = session
            .land_track_conversion(conversion)
            .map_err(|error| error.to_string())?;
        save_checkpointed(&mut session)?;
        Ok(serde_json::json!({"track":track.0, "clip": clip.0, "kind": "audio"}).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    struct Fixture {
        root: PathBuf,
        path: String,
        lead: TrackId,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let root = std::env::temp_dir()
                .join(format!("auris-track-tools-{}-{label}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            let mut session = Session::new(SessionOptions::headless()).unwrap();
            let lead = session.add_default_instrument_track("Lead").unwrap();
            let path = root.join("Song.auris");
            session.save(&path).unwrap();
            Self {
                root,
                path: path.to_string_lossy().into_owned(),
                lead,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn conversion_tool_saves_audio_with_a_checkpoint_of_the_score() {
        let fixture = Fixture::new("conversion");
        let mut session = opened(&fixture.path).unwrap();
        let clip = session
            .add_midi_clip(fixture.lead, "Phrase", Ticks::ZERO, Ticks::QUARTER)
            .unwrap();
        session
            .add_note(clip, Note::new(60, Ticks::ZERO, Ticks::QUARTER))
            .unwrap();
        session.save(std::path::Path::new(&fixture.path)).unwrap();
        let args = convert_track_to_audio::Args {
            project: fixture.path.clone(),
            track: fixture.lead.0,
        };
        let response: Value =
            serde_json::from_str(&convert_track_to_audio::run(&args).unwrap()).unwrap();
        assert_eq!(response["kind"], "audio");
        let reopened = opened(&fixture.path).unwrap();
        assert!(
            reopened
                .project()
                .track(fixture.lead)
                .unwrap()
                .kind
                .as_audio()
                .is_some()
        );
        assert!(!reopened.checkpoints().unwrap().is_empty());
        let saved = std::fs::read(&fixture.path).unwrap();
        assert!(convert_track_to_audio::run(&args).is_err());
        assert_eq!(std::fs::read(&fixture.path).unwrap(), saved);
    }
}
