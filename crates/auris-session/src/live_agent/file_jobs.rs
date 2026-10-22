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
    ImportAudio {
        path: PathBuf,
        folder: PathBuf,
        name: String,
        at: Ticks,
        rate: f64,
    },
    ImportMidi {
        job: crate::MidiImportJob,
        path: PathBuf,
        at: Ticks,
    },
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
    ImportedAudio {
        file: tempfile::NamedTempFile,
        buffer: auris_core::AudioBuffer,
        folder: PathBuf,
        name: String,
        at: Ticks,
        bytes: u64,
    },
    ImportedMidi(Box<crate::MidiImportResult>, Ticks),
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
            Work::ImportMidi { job, path, at } => {
                let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
                if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
                    return Err("MIDI input must be a regular file up to 32 MiB".into());
                }
                Outcome::ImportedMidi(
                    Box::new(
                        job.run(cancelled)
                            .map_err(|e| e.to_string())?
                            .ok_or("MIDI import cancelled")?,
                    ),
                    at,
                )
            }
            Work::ImportAudio {
                path,
                folder,
                name,
                at,
                rate,
            } => {
                use std::io::{Read, Write};
                let mut input = std::fs::File::open(&path).map_err(|e| e.to_string())?;
                if !input.metadata().map_err(|e| e.to_string())?.is_file() {
                    return Err("Audio input must be a regular file".into());
                }
                let directory = folder.join(auris_io::AUDIO_DIR);
                std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                let mut file = tempfile::Builder::new()
                    .prefix("import-")
                    .suffix(&format!(
                        ".{}",
                        path.extension().and_then(|e| e.to_str()).unwrap_or("audio")
                    ))
                    .tempfile_in(&directory)
                    .map_err(|e| e.to_string())?;
                let mut chunk = [0u8; 65536];
                let mut bytes = 0u64;
                loop {
                    check()?;
                    let size = input.read(&mut chunk).map_err(|e| e.to_string())?;
                    if size == 0 {
                        break;
                    }
                    bytes += size as u64;
                    if bytes > 512 * 1024 * 1024 {
                        return Err("Audio input exceeds 512 MiB".into());
                    }
                    file.write_all(&chunk[..size]).map_err(|e| e.to_string())?;
                }
                file.flush().map_err(|e| e.to_string())?;
                let buffer = crate::decode_audio(file.path(), rate).map_err(|e| e.to_string())?;
                check()?;
                Outcome::ImportedAudio {
                    file,
                    buffer,
                    folder,
                    name,
                    at,
                    bytes,
                }
            }
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
            Command::ImportAudio {
                path,
                at_tick,
                name,
            } => {
                if at_tick < 0
                    || name
                        .as_ref()
                        .is_some_and(|n| n.trim().is_empty() || n.len() > 256)
                {
                    return Err(
                        "Use nonnegative at_tick and a nonempty name up to 256 bytes".into(),
                    );
                }
                let path = local_path(&path)?;
                let name = name.unwrap_or_else(|| {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                });
                Work::ImportAudio {
                    path,
                    folder: self
                        .project_folder()
                        .ok_or("Missing project folder")?
                        .to_owned(),
                    name,
                    at: Ticks(at_tick),
                    rate: self.project().sample_rate,
                }
            }
            Command::ImportMidi { path, at_tick } => {
                if at_tick < 0 {
                    return Err("at_tick must be nonnegative".into());
                }
                let path = local_path(&path)?;
                Work::ImportMidi {
                    job: self.begin_midi_import(&path).map_err(|e| e.to_string())?,
                    path,
                    at: Ticks(at_tick),
                }
            }
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
        &mut self,
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
            Outcome::ImportedMidi(result, at) => self.append_agent_midi(*result, at),
            Outcome::ImportedAudio {
                file,
                buffer,
                folder,
                name,
                at,
                bytes,
            } => {
                if self.project_folder() != Some(folder.as_path())
                    || buffer.sample_rate() != self.project().sample_rate
                {
                    return Err("Document folder/rate changed during audio import".into());
                }
                self.adopt_agent_audio(file, buffer, name, at, bytes)
            }
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

    #[test]
    fn audio_and_midi_import_append_without_replacing_music_and_undo_once() {
        let directory = tempfile::tempdir().unwrap();
        let mut live = fixture();
        let midi = directory.path().join("source.mid");
        live.export_midi(&midi).unwrap();
        let original = live.project().clone();
        let cancelled = AtomicBool::new(false);
        let job = live
            .begin_agent_file_job(Command::ImportMidi {
                path: midi.to_string_lossy().into_owned(),
                at_tick: 997,
            })
            .unwrap();
        let result = job.run(&cancelled).unwrap();
        assert_eq!(live.project(), &original);
        live.continue_agent_file_job(result, &cancelled).unwrap();
        assert_eq!(live.project().tracks.len(), original.tracks.len() + 1);
        assert_eq!(live.project().tempo_map, original.tempo_map);
        assert_eq!(live.project().tracks[0], original.tracks[0]);
        let added = live
            .project()
            .tracks
            .last()
            .unwrap()
            .kind
            .as_instrument()
            .unwrap();
        assert_eq!(added.clips[0].start, Ticks(997));
        live.undo();
        assert_eq!(live.project(), &original);

        let audio = directory.path().join("source.wav");
        let mut writer = hound::WavWriter::create(
            &audio,
            hound::WavSpec {
                channels: 1,
                sample_rate: 24_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..2400 {
            writer.write_sample::<i16>(1234).unwrap();
        }
        writer.finalize().unwrap();
        let job = live
            .begin_agent_file_job(Command::ImportAudio {
                path: audio.to_string_lossy().into_owned(),
                at_tick: 13,
                name: Some("Recording".into()),
            })
            .unwrap();
        let result = job.run(&cancelled).unwrap();
        assert_eq!(live.project(), &original);
        live.continue_agent_file_job(result, &cancelled).unwrap();
        let source = live.project().audio_sources.values().next().unwrap();
        let stored = source.path.resolve(live.project_folder()).unwrap();
        assert_eq!(
            std::fs::read(&audio).unwrap(),
            std::fs::read(&stored).unwrap()
        );
        assert!(source.path.is_inside());
        let added = live.project().tracks.last().unwrap();
        assert_eq!(added.name, "Recording");
        assert_eq!(added.kind.as_audio().unwrap().clips[0].start, Ticks(13));
        live.undo();
        assert_eq!(live.project(), &original);
    }

    #[test]
    fn cancelled_and_stale_audio_imports_discard_private_source_copies() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.wav");
        let mut writer = hound::WavWriter::create(
            &source,
            hound::WavSpec {
                channels: 1,
                sample_rate: 24_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..240 {
            writer.write_sample::<i16>(1).unwrap();
        }
        writer.finalize().unwrap();
        let mut live = fixture();
        let cancelled = AtomicBool::new(false);
        for cancel in [false, true] {
            let job = live
                .begin_agent_file_job(Command::ImportAudio {
                    path: source.to_string_lossy().into_owned(),
                    at_tick: 0,
                    name: None,
                })
                .unwrap();
            let result = job.run(&cancelled).unwrap();
            let Outcome::ImportedAudio { file, .. } = &result.outcome else {
                panic!("audio")
            };
            let private = file.path().to_owned();
            if cancel {
                cancelled.store(true, Ordering::Relaxed);
            } else {
                live.add_bus_track("Concurrent edit");
            }
            let before = live.project().clone();
            assert!(live.continue_agent_file_job(result, &cancelled).is_err());
            assert!(!private.exists());
            assert_eq!(live.project(), &before);
        }
        assert!(source.exists());
    }

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
