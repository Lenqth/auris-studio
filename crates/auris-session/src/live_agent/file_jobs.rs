//! Snapshot-based file operations; workers stage outputs and the owner publishes them.

use super::Command;
use crate::{Session, prelude::*};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// A complete mix or independently rendered source stems.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioDestination {
    /// One encoded WAV mix at a new absolute path.
    Mix {
        /// Absolute new WAV file path.
        path: String,
    },
    /// One WAV per source track; existing stem files are refused.
    Stems {
        /// Absolute destination folder.
        folder: String,
    },
}

/// An explicit contiguous arrangement range.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BarRange {
    /// First one-based arrangement bar.
    pub start_bar: u32,
    /// Positive number of bars.
    pub bars: u32,
}

/// Owned, cancellable work detached from the UI and document.
pub struct FileJob {
    revision: u64,
    owner: Arc<auris_core::PluginRegistry>,
    work: Work,
}

enum Work {
    Audio {
        job: crate::RenderJob,
        guard: crate::render::ExportGuard,
        targets: Vec<(PathBuf, Option<TrackId>)>,
        folder: Option<PathBuf>,
        options: OfflineOptions,
        settings: WavExportSettings,
    },
    Midi {
        job: crate::MidiExportJob,
        guard: crate::render::ExportGuard,
        path: PathBuf,
    },
    Verify {
        path: PathBuf,
        target_lufs: Option<f32>,
        ceiling_db: f32,
        tolerance_lu: f32,
        ending_rms_max_db: Option<f32>,
    },
}

/// A worker result retaining private files until its originating document accepts it.
pub struct FileResult {
    revision: u64,
    owner: Arc<auris_core::PluginRegistry>,
    outcome: Outcome,
}

enum Outcome {
    Audio(
        Vec<(
            PathBuf,
            auris_io::StagedAudioExport,
            crate::ExportInspection,
        )>,
    ),
    Midi(crate::MidiExportResult),
    Verified(String),
}

fn local_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute() || value.starts_with("\\\\") || value.starts_with("//") {
        return Err("Use an absolute local file path".into());
    }
    Ok(path)
}

impl FileJob {
    /// Perform expensive rendering/encoding/measurement without changing the live document.
    pub fn run(self, cancelled: &AtomicBool) -> Result<FileResult, String> {
        let check = || {
            if cancelled.load(Ordering::Relaxed) {
                Err("File operation cancelled".to_string())
            } else {
                Ok(())
            }
        };
        check()?;
        let outcome = match self.work {
            Work::Audio {
                mut job,
                guard,
                targets,
                folder,
                options,
                settings,
            } => {
                if let Some(folder) = &folder {
                    guard
                        .check_stems(
                            folder,
                            &targets
                                .iter()
                                .map(|(path, _)| path.clone())
                                .collect::<Vec<_>>(),
                        )
                        .map_err(|e| e.to_string())?;
                    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
                }
                let mut staged = Vec::with_capacity(targets.len());
                for (path, track) in targets {
                    check()?;
                    guard.check_file(&path).map_err(|e| e.to_string())?;
                    if path.exists() {
                        return Err(format!("Output already exists: {}", path.display()));
                    }
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    let mut progress = RenderProgress::default().cancelled_by(cancelled);
                    let encoded = job
                        .stage_audio_export(&path, &settings.into(), &options, track, &mut progress)
                        .map_err(|e| e.to_string())?;
                    check()?;
                    let inspection =
                        crate::inspect_export(encoded.staged_path()).map_err(|e| e.to_string())?;
                    check()?;
                    staged.push((path, encoded, inspection));
                }
                Outcome::Audio(staged)
            }
            Work::Midi { job, guard, path } => {
                guard.check_file(&path).map_err(|e| e.to_string())?;
                Outcome::Midi(
                    job.run(cancelled)
                        .map_err(|e| e.to_string())?
                        .ok_or("MIDI export cancelled")?,
                )
            }
            Work::Verify {
                path,
                target_lufs,
                ceiling_db,
                tolerance_lu,
                ending_rms_max_db,
            } => {
                let encoded = crate::inspect_export(&path).map_err(|e| e.to_string())?;
                check()?;
                let loudness_ok = target_lufs.is_none_or(|target| {
                    encoded
                        .lufs
                        .is_some_and(|value| (value - target).abs() <= tolerance_lu)
                });
                let peak_ok = encoded.true_peak_db <= ceiling_db && encoded.full_scale_samples == 0;
                let ending_ok =
                    ending_rms_max_db.is_none_or(|limit| encoded.ending_rms_db <= limit);
                Outcome::Verified(serde_json::json!({"path":path,"verdict":if loudness_ok&&peak_ok&&ending_ok {"PASS"}else{"FAIL"},"loudness_ok":loudness_ok,"peak_ok":peak_ok,"ending_ok":ending_ok,"encoded":encoded}).to_string())
            }
        };
        check()?;
        Ok(FileResult {
            revision: self.revision,
            owner: self.owner,
            outcome,
        })
    }
}

impl Session {
    /// Capture an immutable document snapshot for audio/MIDI export or file verification.
    pub fn begin_agent_file_job(&mut self, command: Command) -> Result<FileJob, String> {
        let owner = Arc::clone(self.registry());
        let work = match command {
            Command::Render {
                destination,
                range,
                bit_depth,
                include_tail,
            } => {
                let options = self.agent_export_range(range, include_tail)?;
                let settings = WavExportSettings {
                    bit_depth: match bit_depth.unwrap_or(24) {
                        16 => WavBitDepth::Int16,
                        24 => WavBitDepth::Int24,
                        32 => WavBitDepth::Float32,
                        _ => return Err("bit_depth must be 16,24 or 32".into()),
                    },
                    ..Default::default()
                };
                let job = self.render_job();
                let (targets, folder) = match destination {
                    AudioDestination::Mix { path } => (vec![(local_path(&path)?, None)], None),
                    AudioDestination::Stems { folder } => {
                        let folder = local_path(&folder)?;
                        let tracks = job.stem_tracks();
                        if tracks.is_empty() {
                            return Err("No source tracks to export".into());
                        }
                        let paths = job.stem_paths(&folder, auris_io::AudioExportFormat::Wav);
                        (
                            paths
                                .into_iter()
                                .zip(tracks)
                                .map(|(path, (track, _))| (path, Some(track)))
                                .collect(),
                            Some(folder),
                        )
                    }
                };
                Work::Audio {
                    job,
                    guard: self.export_guard(),
                    targets,
                    folder,
                    options,
                    settings,
                }
            }
            Command::Preview { range } => {
                let mut options = self.agent_export_range(Some(range), false)?;
                let duration = options
                    .end_frames
                    .unwrap_or(0)
                    .saturating_sub(options.start_frames) as f64
                    / self.project().sample_rate;
                if duration > 120.0 {
                    return Err("Preview must fit within 120 seconds".into());
                }
                // Keep the time range exact when changing the audition rate.
                let scale = 24_000.0 / self.project().sample_rate;
                options.start_frames = (options.start_frames as f64 * scale).round() as u64;
                options.end_frames = options
                    .end_frames
                    .map(|end| (end as f64 * scale).round() as u64);
                options.sample_rate = Some(24_000.0);
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                let path = self
                    .project_folder()
                    .ok_or("Missing document working folder")?
                    .join(".auris-previews")
                    .join(format!(
                        "preview-{}-{}.wav",
                        std::process::id(),
                        NEXT.fetch_add(1, Ordering::Relaxed)
                    ));
                Work::Audio {
                    job: self.render_job(),
                    guard: self.export_guard(),
                    targets: vec![(path, None)],
                    folder: None,
                    options,
                    settings: WavExportSettings::default(),
                }
            }
            Command::ExportMidi { output } => {
                let path = local_path(&output)?;
                Work::Midi {
                    job: self.begin_midi_export(&path),
                    guard: self.export_guard(),
                    path,
                }
            }
            Command::VerifyRender {
                path,
                target_lufs,
                ceiling_db,
                tolerance_lu,
                ending_rms_max_db,
            } => {
                let ceiling_db = ceiling_db.unwrap_or(-1.0);
                let tolerance_lu = tolerance_lu.unwrap_or(1.0);
                if !(-12.0..=0.0).contains(&ceiling_db)
                    || !(0.0..=12.0).contains(&tolerance_lu)
                    || target_lufs.is_some_and(|v| !(-60.0..=-6.0).contains(&v))
                    || ending_rms_max_db.is_some_and(|v| !(-120.0..=0.0).contains(&v))
                {
                    return Err("Invalid delivery target, tolerance or ending limit".into());
                }
                Work::Verify {
                    path: local_path(&path)?,
                    target_lufs,
                    ceiling_db,
                    tolerance_lu,
                    ending_rms_max_db,
                }
            }
            _ => return Err("Expected render, preview, export_midi or verify_render".into()),
        };
        Ok(FileJob {
            revision: self.revision(),
            owner,
            work,
        })
    }

    fn agent_export_range(
        &self,
        range: Option<BarRange>,
        include_tail: bool,
    ) -> Result<OfflineOptions, String> {
        let Some(range) = range else {
            return Ok(OfflineOptions {
                include_tail,
                ..Default::default()
            });
        };
        if range.start_bar == 0 || range.bars == 0 || range.bars > 4096 {
            return Err("Use start_bar>=1 and bars within 1..4096".into());
        }
        let end_bar = range
            .start_bar
            .checked_add(range.bars)
            .ok_or("Bar range overflow")?;
        let from = self.project().signatures.bar_start(range.start_bar);
        let to = self.project().signatures.bar_start(end_bar);
        self.render_range_options(from, to, include_tail)
            .ok_or("Export range must lie inside the arrangement".into())
    }

    /// Publish only a still-current, uncancelled worker result; stale stages are deleted on drop.
    pub fn continue_agent_file_job(
        &self,
        result: FileResult,
        cancelled: &AtomicBool,
    ) -> Result<String, String> {
        if cancelled.load(Ordering::Relaxed)
            || self.revision() != result.revision
            || !Arc::ptr_eq(self.registry(), &result.owner)
        {
            return Err(
                "Document changed or export cancelled; no staged output was published".into(),
            );
        }
        match result.outcome {
            Outcome::Verified(text) => Ok(text),
            Outcome::Midi(result) => {
                let (path, notes) = self
                    .continue_midi_export(result, cancelled)
                    .map_err(|e| e.to_string())?
                    .ok_or("MIDI export became stale")?;
                Ok(
                    serde_json::json!({"path":path,"notes":notes,"document_unchanged":true})
                        .to_string(),
                )
            }
            Outcome::Audio(staged) => {
                for (path, _, _) in &staged {
                    self.export_guard()
                        .check_file(path)
                        .map_err(|e| e.to_string())?;
                    if path.exists() {
                        return Err(format!("Output already exists: {}", path.display()));
                    }
                }
                let mut files = Vec::with_capacity(staged.len());
                for (path, encoded, inspection) in staged {
                    encoded.publish_noclobber().map_err(|e| {
                        format!("Publication failed: {e}; already published: {files:?}")
                    })?;
                    files.push(serde_json::json!({"path":path,"encoded":inspection}));
                }
                Ok(serde_json::json!({"files":files,"document_unchanged":true}).to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Session {
        let mut session = Session::new(crate::SessionOptions::headless()).unwrap();
        let track = session.add_default_instrument_track("Lead").unwrap();
        let clip = session
            .add_midi_clip(track, "Phrase", Ticks::ZERO, Ticks::QUARTER * 4)
            .unwrap();
        session
            .replace_notes(clip, vec![Note::new(60, Ticks::ZERO, Ticks::QUARTER * 2)])
            .unwrap();
        session
    }

    fn render(path: &std::path::Path) -> Command {
        Command::Render {
            destination: AudioDestination::Mix {
                path: path.to_string_lossy().into(),
            },
            range: None,
            bit_depth: Some(24),
            include_tail: false,
        }
    }

    #[test]
    fn unsaved_mix_is_staged_measured_then_published_without_editing_the_score() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mix.wav");
        let mut session = fixture();
        let before = session.project().clone();
        let cancelled = AtomicBool::new(false);
        let job = session.begin_agent_file_job(render(&path)).unwrap();
        let result = job.run(&cancelled).unwrap();
        assert!(!path.exists());
        let text = session.continue_agent_file_job(result, &cancelled).unwrap();
        let report: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            report["files"][0]["encoded"]["sample_rate"],
            session.project().sample_rate as u32
        );
        assert_eq!(report["files"][0]["encoded"]["bit_depth"], 24);
        assert!(report["files"][0]["encoded"]["frames"].as_u64().unwrap() > 0);
        let actual = crate::inspect_export(&path).unwrap();
        assert_eq!(report["files"][0]["encoded"]["sha256"], actual.sha256);
        assert_eq!(session.project(), &before);
        assert!(session.path().is_none());
        let verify = session
            .begin_agent_file_job(Command::VerifyRender {
                path: path.to_string_lossy().into(),
                target_lufs: Some(-6.0),
                ceiling_db: Some(0.0),
                tolerance_lu: Some(0.0),
                ending_rms_max_db: None,
            })
            .unwrap()
            .run(&cancelled)
            .unwrap();
        let report: serde_json::Value =
            serde_json::from_str(&session.continue_agent_file_job(verify, &cancelled).unwrap())
                .unwrap();
        assert_eq!(report["verdict"], "FAIL");
    }

    #[test]
    fn stale_cancelled_or_raced_exports_preserve_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        for mode in ["stale", "cancelled", "raced"] {
            let path = directory.path().join(format!("{mode}.wav"));
            let mut session = fixture();
            let cancelled = AtomicBool::new(false);
            let result = session
                .begin_agent_file_job(render(&path))
                .unwrap()
                .run(&cancelled)
                .unwrap();
            match mode {
                "stale" => {
                    session.add_bus_track("Changed");
                }
                "cancelled" => cancelled.store(true, Ordering::Relaxed),
                _ => std::fs::write(&path, b"another writer's bytes").unwrap(),
            }
            assert!(session.continue_agent_file_job(result, &cancelled).is_err());
            if mode == "raced" {
                assert_eq!(std::fs::read(&path).unwrap(), b"another writer's bytes");
            } else {
                assert!(!path.exists());
            }
        }
    }

    #[test]
    fn midi_and_stems_use_the_unsaved_snapshot_and_preview_uses_24khz() {
        let directory = tempfile::tempdir().unwrap();
        let mut session = fixture();
        let before = session.project().clone();
        let cancelled = AtomicBool::new(false);
        let midi = directory.path().join("score.mid");
        let result = session
            .begin_agent_file_job(Command::ExportMidi {
                output: midi.to_string_lossy().into(),
            })
            .unwrap()
            .run(&cancelled)
            .unwrap();
        assert!(!midi.exists());
        session.continue_agent_file_job(result, &cancelled).unwrap();
        assert!(std::fs::read(&midi).unwrap().starts_with(b"MThd"));
        let result = session
            .begin_agent_file_job(Command::Preview {
                range: BarRange {
                    start_bar: 1,
                    bars: 1,
                },
            })
            .unwrap()
            .run(&cancelled)
            .unwrap();
        let report: serde_json::Value =
            serde_json::from_str(&session.continue_agent_file_job(result, &cancelled).unwrap())
                .unwrap();
        assert_eq!(report["files"][0]["encoded"]["sample_rate"], 24000);
        let result = session
            .begin_agent_file_job(Command::Render {
                destination: AudioDestination::Stems {
                    folder: directory.path().join("stems").to_string_lossy().into(),
                },
                range: None,
                bit_depth: None,
                include_tail: false,
            })
            .unwrap()
            .run(&cancelled)
            .unwrap();
        let report: serde_json::Value =
            serde_json::from_str(&session.continue_agent_file_job(result, &cancelled).unwrap())
                .unwrap();
        assert_eq!(report["files"].as_array().unwrap().len(), 1);
        assert_eq!(session.project(), &before);
    }
}
