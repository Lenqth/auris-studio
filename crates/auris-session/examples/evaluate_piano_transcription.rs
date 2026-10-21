//! Generate composer score/audio pairs, or transcribe an existing manifest with the live backend.
//! Usage: `generate <new-directory>`, `run <manifest> <new-directory>`,
//! or `cover <result.json> <new-directory>` after the example executable name.

use auris_session::{
    PianoTranscription, Session, SessionOptions, prelude::*, transcribe_piano_file,
};
use serde_json::{Value, json};
use std::{error::Error, fs, path::Path, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, output] if command == "generate" => generate(Path::new(output)),
        [command, manifest, output] if command == "run" => run(Path::new(manifest), Path::new(output)),
        [command, notes, output] if command == "cover" => cover(Path::new(notes), Path::new(output)),
        _ => Err("usage: evaluate_piano_transcription generate <new-directory> | run <manifest> <new-directory> | cover <result.json> <new-directory>".into()),
    }
}

fn cover(notes: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    if output.exists() {
        return Err("output directory already exists".into());
    }
    let result: Value = serde_json::from_slice(&fs::read(notes)?)?;
    let draft: PianoTranscription = serde_json::from_value(result["transcription"].clone())?;
    let mut session = Session::new(SessionOptions::headless().with_balance(false))?;
    let before = session.project().clone();
    session.create_piano_cover(&draft, Ticks::ZERO, "Piano cover")?;
    let expected = session.project().clone();
    session.undo();
    if session.project() != &before {
        return Err("cover failed one-step Undo".into());
    }
    session.redo();
    if session.project() != &expected {
        return Err("cover failed Redo".into());
    }
    fs::create_dir_all(output)?;
    let saved = session.save_as(&output.join("cover.auris"))?;
    let options = OfflineOptions {
        sample_rate: Some(48000.0),
        end_frames: Some(((draft.seconds + 1.0) * 48000.0).ceil() as u64),
        include_tail: false,
        ..Default::default()
    };
    session.render_job().render_to_wav(
        &output.join("cover.wav"),
        &WavExportSettings {
            sample_rate: 48000,
            bit_depth: WavBitDepth::Float32,
            dither: false,
        },
        &options,
        &mut RenderProgress::default(),
    )?;
    fs::write(
        output.join("acceptance.json"),
        serde_json::to_vec_pretty(
            &json!({"project":saved.document,"one_undo_restored":true,"redo_restored":true,"notes":draft.notes.len()}),
        )?,
    )?;
    Ok(())
}

fn generate(output: &Path) -> Result<(), Box<dyn Error>> {
    if output.exists() {
        return Err("output directory already exists".into());
    }
    fs::create_dir_all(output)?;
    let mut cases = Vec::new();
    for (split, seeds) in [
        ("development", [101, 102]),
        ("validation", [301, 302]),
        ("held_out", [901, 902]),
    ] {
        for preset_name in ["city-pop", "jazz-trio", "orchestral"] {
            for seed in seeds {
                let mut spec = preset(preset_name).ok_or("unknown preset")?.spec();
                spec.seed = seed;
                if spec.sections.contains_key("verse") {
                    spec.form = vec!["verse".into()];
                } else {
                    spec.form.truncate(1);
                }
                spec.sections.retain(|name, _| spec.form.contains(name));
                spec.ending = Ending::None;
                for section in spec.sections.values_mut() {
                    section.bars = 4;
                }
                for part in &mut spec.parts {
                    part.source = None;
                    part.program = None;
                }
                let mut source = Session::new(SessionOptions::headless().with_balance(false))?;
                source.compose(&compose(&spec))?;
                let project = source.project();
                let end = project.end_tick();
                // Freeze the exact performed events, not grid notes: timing, loops and velocity
                // transforms are resolved once and the renderer/annotations consume the same text.
                let tracks: Vec<_> = project
                    .tracks
                    .iter()
                    .filter_map(|track| {
                        let inner = track.kind.as_instrument()?;
                        let notes: Vec<_> = inner
                            .clips
                            .iter()
                            .filter(|c| !c.muted)
                            .flat_map(|clip| {
                                clip.sounding_notes_with_meter(
                                    project.tempo_map.bpm_at(clip.start),
                                    project.signatures.clone(),
                                )
                                .map(|mut note| {
                                    note.start += clip.start;
                                    note
                                })
                                .collect::<Vec<_>>()
                            })
                            .collect();
                        Some((
                            track.name.clone(),
                            inner.instrument_id.clone(),
                            track.kind.is_drum(),
                            notes,
                        ))
                    })
                    .collect();
                for variant in ["piano", "mixture"] {
                    let id = format!("{preset_name}-{seed}-{variant}");
                    let mut session = Session::new(SessionOptions::headless().with_balance(false))?;
                    session.set_bpm(spec.tempo);
                    session.set_signature_at(Ticks::ZERO, spec.meter);
                    session.set_key(Ticks::ZERO, spec.key);
                    let mut reference = Vec::new();
                    for (name, instrument, drum, notes) in &tracks {
                        if *drum && variant == "piano" {
                            continue;
                        }
                        let instrument = if variant == "piano" {
                            "auris.physical.piano"
                        } else {
                            instrument.as_str()
                        };
                        let track = if *drum {
                            session.add_drum_track(name, instrument)?
                        } else {
                            session.add_instrument_track(name, instrument)?
                        };
                        let clip =
                            session.add_midi_clip(track, "Frozen performance", Ticks::ZERO, end)?;
                        session.replace_notes(clip, notes.clone())?;
                        if !*drum {
                            for note in notes {
                                reference.push(json!({"pitch":note.pitch,"start":project.tempo_map.ticks_to_seconds(note.start).0,
                                    "end":project.tempo_map.ticks_to_seconds(note.start+note.length).0,"part":name,"velocity":note.velocity}));
                            }
                        }
                    }
                    let saved = session.save_as(&output.join(format!("{id}.auris")))?;
                    let folder = saved.document.parent().ok_or("project has no folder")?;
                    let seconds = project.tempo_map.ticks_to_seconds(end).0 + 1.0;
                    let options = OfflineOptions {
                        sample_rate: Some(22050.0),
                        end_frames: Some((seconds * 22050.0).ceil() as u64),
                        include_tail: false,
                        ..Default::default()
                    };
                    let mut audio = session
                        .render_job()
                        .render(&options, &mut RenderProgress::default())?;
                    let peak = audio
                        .iter_channels()
                        .flat_map(|c| c.iter())
                        .copied()
                        .map(f32::abs)
                        .fold(0.0_f32, f32::max);
                    if peak <= 0.0 || !peak.is_finite() {
                        return Err("silent or invalid render".into());
                    }
                    let gain = 0.7 / peak;
                    for channel in audio.channels_mut() {
                        for sample in channel {
                            *sample *= gain;
                        }
                    }
                    auris_io::write_wav(
                        &folder.join("source.wav"),
                        &audio,
                        &WavExportSettings {
                            sample_rate: 22050,
                            bit_depth: WavBitDepth::Float32,
                            dither: false,
                        },
                    )?;
                    fs::write(
                        folder.join("reference.json"),
                        serde_json::to_vec_pretty(&json!({"notes":reference}))?,
                    )?;
                    cases.push(json!({"id":id,"split":split,"preset":preset_name,"seed":seed,"variant":variant,
                        "audio":format!("{id}/source.wav"),"reference":format!("{id}/reference.json"),"project":format!("{id}/{id}.auris"),"gain":gain,"seconds":seconds}));
                    eprintln!("rendered {id}");
                }
            }
        }
    }
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({"schema":1,"suite":"composer-piano-cover-v1",
        "provenance":"Original deterministic compositions, repository license; frozen performed events, native instruments, dry mix, one second release margin, common peak 0.7. No recordings or training weights redistributed.","cases":cases}))?,
    )?;
    Ok(())
}

fn run(manifest: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    if output.exists() {
        return Err("output directory already exists".into());
    }
    fs::create_dir_all(output)?;
    let corpus: Value = serde_json::from_slice(&fs::read(manifest)?)?;
    let root = manifest.parent().ok_or("manifest has no folder")?;
    for case in corpus["cases"].as_array().ok_or("missing cases")? {
        let id = case["id"].as_str().ok_or("missing case id")?;
        let path = root.join(case["audio"].as_str().ok_or("missing audio")?);
        let started = Instant::now();
        let transcription = transcribe_piano_file(&path, &Default::default())?;
        let elapsed = started.elapsed().as_secs_f64();
        fs::write(
            output.join(format!("{id}.json")),
            serde_json::to_vec_pretty(
                &json!({"elapsed_seconds":elapsed,"transcription":transcription}),
            )?,
        )?;
        eprintln!("transcribed {id} in {elapsed:.2}s");
    }
    Ok(())
}
