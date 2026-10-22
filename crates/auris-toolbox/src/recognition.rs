//! CPU music recognition commands shared by both model frontends.

use super::*;
use auris_session::{AnalysisControl, AudioOptions, ChordOptions};

fn analysis_control() -> AnalysisControl {
    AnalysisControl::cancelled_by(cancellation::current().flag_handle())
}

fn beats(value: f64) -> Result<Ticks, String> {
    if !value.is_finite() || !(0.0..=1_000_000.0).contains(&value) {
        return Err("beat positions must be finite and within 0..1000000".into());
    }
    Ok(Ticks::from_beats(value))
}

/// Optional local instrument-presence tagging without project edits.
pub mod analyze_instruments {
    use super::*;
    /// Wire name.
    pub const NAME: &str = "analyze_instruments";
    /// Model-facing command description.
    pub const DESCRIPTION: &str = "Estimates instrument and singing presence with an explicitly supplied local YAMNet ONNX export on CPU. Returns overlapping source-second windows, multiple candidate labels, raw event scores and model hash. Empty candidates mean unknown. Scores are not calibrated probabilities. No downloads, GPU, source separation, note assignment or project edits. Large results return an immutable report_id snapshot; use read_report for details instead of repeating analysis or edits.";
    /// Audio input and an explicitly prepared model.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    pub struct Args {
        /// Absolute audio-file path.
        pub audio: String,
        /// Absolute path to the ONNX file prepared by tools/music-models/export_yamnet.py.
        pub model: String,
        /// Minimum displayed instrument score in 0..1; defaults to 0.2.
        pub threshold: Option<f32>,
    }
    /// Runs local inference and serializes the read-only report.
    pub fn run(args: &Args) -> Result<String, String> {
        let report = auris_session::analyze_instrument_file(
            Path::new(&args.audio),
            Path::new(&args.model),
            args.threshold.unwrap_or(0.2),
            &analysis_control(),
        )
        .map_err(|e| e.to_string())?;
        reports::publish(&report)
    }
}

/// Written notes to chromatic chord candidates and optional explicit harmony edits.
pub mod analyze_chords {
    use super::*;
    /// Wire name.
    pub const NAME: &str = "analyze_chords";
    /// Model-facing command description.
    pub const DESCRIPTION: &str = "Recognizes chords from written notes on the CPU without rendering or models. Reports absolute-tick intervals, alternate chord symbols and unknown/silent regions. Known percussion is excluded. Apply explicitly replaces recognized harmony and clears silence while preserving unknown intervals and outside harmony; saves a checkpoint. Large results return an immutable report_id snapshot; use read_report for details instead of repeating analysis or edits.";
    /// Track selection, time range and optional acceptance.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    pub struct Args {
        /// Absolute project path.
        pub project: String,
        /// Track names or id:number selectors; empty selects all pitched tracks.
        #[serde(default)]
        pub tracks: Vec<String>,
        /// Zero-based quarter-note beat at which to start; defaults to zero.
        pub from_beat: Option<f64>,
        /// Exclusive zero-based end beat; defaults to the project end.
        pub to_beat: Option<f64>,
        /// Window size in quarter-note beats; defaults to one.
        pub window_beats: Option<f64>,
        /// Explicitly accept recognized candidates and save; false only reports.
        #[serde(default)]
        pub apply: bool,
    }
    /// Analyzes one immutable selection and optionally accepts its recognized harmony.
    pub fn run(args: &Args) -> Result<String, String> {
        let mut session = opened(&args.project)?;
        let tracks = args
            .tracks
            .iter()
            .map(|t| track_by_name(session.project(), t).map(|t| t.id))
            .collect::<Result<Vec<_>, _>>()?;
        let from = beats(args.from_beat.unwrap_or(0.0))?;
        let to = args
            .to_beat
            .map(beats)
            .transpose()?
            .unwrap_or_else(|| session.project().end_tick());
        let options = ChordOptions {
            window: beats(args.window_beats.unwrap_or(1.0))?,
        };
        let report = session
            .chord_analysis_job(&tracks, from, to, options)
            .and_then(|j| j.run(&analysis_control()))
            .map_err(|e| e.to_string())?;
        let response = reports::publish(&report)?;
        if args.apply {
            session
                .apply_chord_analysis(&report)
                .map_err(|e| e.to_string())?;
            save_checkpointed(&mut session)?;
        }
        Ok(response)
    }
}

/// File-based tempo and major/minor chord analysis without a project edit.
pub mod analyze_audio {
    use super::*;
    /// Wire name.
    pub const NAME: &str = "analyze_audio";
    /// Model-facing command description.
    pub const DESCRIPTION: &str = "Analyzes an audio file on the CPU without models or GPU: constant BPM alternatives, beat timestamps and half-second major/minor chord windows. Scores are template/periodicity agreement, not calibrated probabilities. No project is changed. Does not identify instruments. Large results return an immutable report_id snapshot; use read_report for details instead of repeating analysis or edits.";
    /// Audio input.
    #[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
    pub struct Args {
        /// Absolute audio-file path.
        pub audio: String,
    }
    /// Decodes and reports in source seconds.
    pub fn run(args: &Args) -> Result<String, String> {
        let report = auris_session::analyze_audio_file(
            Path::new(&args.audio),
            AudioOptions::default(),
            &analysis_control(),
        )
        .map_err(|e| e.to_string())?;
        reports::publish(&report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_tools_report_without_writing_then_export_only_the_draft_and_apply() {
        let root = std::env::temp_dir().join(format!(
            "auris-recognition-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let wav = root.join("mono.wav");
        // An independently encoded, 16-bit 22050 Hz WAV exercises the real decoder/resampler.
        let rate = 22_050u32;
        let samples: Vec<i16> = (0..rate)
            .map(|i| {
                let t = f64::from(i) / f64::from(rate);
                if (0.2..0.8).contains(&t) {
                    ((std::f64::consts::TAU * 440.0 * t).sin() * 12000.0) as i16
                } else {
                    0
                }
            })
            .collect();
        let bytes = samples.len() as u32 * 2;
        let mut wav_bytes = Vec::new();
        wav_bytes.extend_from_slice(b"RIFF");
        wav_bytes.extend_from_slice(&(36 + bytes).to_le_bytes());
        wav_bytes.extend_from_slice(b"WAVEfmt ");
        wav_bytes.extend_from_slice(&16u32.to_le_bytes());
        for value in [1u16, 1u16] {
            wav_bytes.extend_from_slice(&value.to_le_bytes());
        }
        wav_bytes.extend_from_slice(&rate.to_le_bytes());
        wav_bytes.extend_from_slice(&(rate * 2).to_le_bytes());
        for value in [2u16, 16u16] {
            wav_bytes.extend_from_slice(&value.to_le_bytes());
        }
        wav_bytes.extend_from_slice(b"data");
        wav_bytes.extend_from_slice(&bytes.to_le_bytes());
        for value in samples {
            wav_bytes.extend_from_slice(&value.to_le_bytes());
        }
        std::fs::write(&wav, &wav_bytes).unwrap();
        let project = root.join("Song.auris");
        let mut s = Session::new(SessionOptions::headless()).unwrap();
        let track = s.add_default_instrument_track("Existing").unwrap();
        let clip = s
            .add_midi_clip(track, "Triad", Ticks::ZERO, Ticks::QUARTER)
            .unwrap();
        for pitch in [60, 64, 67] {
            s.add_note(clip, Note::new(pitch, Ticks::ZERO, Ticks::QUARTER))
                .unwrap();
        }
        s.save(&project).unwrap();
        let original = std::fs::read(&project).unwrap();
        let chord_args = analyze_chords::Args {
            project: project.display().to_string(),
            tracks: vec![],
            from_beat: None,
            to_beat: None,
            window_beats: None,
            apply: false,
        };
        let chord_json: serde_json::Value =
            serde_json::from_str(&analyze_chords::run(&chord_args).unwrap()).unwrap();
        assert_eq!(
            chord_json["segments"][0]["reading"]["candidates"][0]["symbol"],
            "C"
        );
        assert_eq!(std::fs::read(&project).unwrap(), original);
        let args = crate::transcription_tools::Args {
            path: Some(wav.display().to_string()),
            attachment: None,
            backend: crate::transcription_tools::Backend::Yin,
        };
        let recognized: serde_json::Value =
            serde_json::from_str(&crate::transcription_tools::run(&args).unwrap()).unwrap();
        assert_eq!(recognized["note_count"], 1);
        assert_eq!(recognized["preview"]["notes"][0]["pitch"], 69);
        assert_eq!(std::fs::read(&project).unwrap(), original);
        let accepted = crate::runtime::call_saved(
            &crate::ToolGroups::default(),
            "import_transcription",
            serde_json::json!({"project":project,"draft_id":recognized["draft_id"],"at_tick":7680}),
        )
        .unwrap();
        let accepted: serde_json::Value = serde_json::from_str(&accepted).unwrap();
        assert_eq!(accepted["notes"], 1);
        let saved = opened(project.to_str().unwrap()).unwrap();
        assert_eq!(saved.project().tracks.len(), 2);
        assert_eq!(saved.project().midi_clip(clip).unwrap().1.notes.len(), 3);
        assert_eq!(std::fs::read(&wav).unwrap(), wav_bytes);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
