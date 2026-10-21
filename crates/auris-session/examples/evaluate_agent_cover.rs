//! Exercise the live agent's piano-cover backend against an independently supplied WAV.
//!
//! Usage: `cargo run -p auris-session --example evaluate_agent_cover -- source.wav output.auris`.
//! Prints the immutable transcription and acceptance/undo checks as JSON. This bypasses the
//! attachment host and permission UI; transport coverage belongs to the agent host tests.

use std::{error::Error, path::Path, time::Instant};

use auris_session::{
    Session, SessionOptions, live_agent::Command, prelude::*, transcribe_piano_file,
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("expected source.wav and a new output.auris path".into());
    }
    let source = Path::new(&args[0]);
    let destination = Path::new(&args[1]);
    if destination.exists() {
        return Err("output already exists".into());
    }
    let started = Instant::now();
    let draft = transcribe_piano_file(source, &Default::default())?;
    let inference_seconds = started.elapsed().as_secs_f64();
    let mut session = Session::new(SessionOptions::headless())?;
    session.set_bpm(100.0);
    let existing = session.add_default_instrument_track("Existing music")?;
    let clip = session.add_midi_clip(existing, "Sentinel", Ticks::ZERO, Ticks::QUARTER)?;
    session.add_note(clip, Note::new(48, Ticks::ZERO, Ticks::QUARTER))?;
    let before = session.project().clone();
    let command = || Command::CreatePianoCover {
        transcription: draft.clone(),
        name: "Audio piano cover".into(),
        at_beat: 4.0,
    };
    let response: serde_json::Value = serde_json::from_str(&session.agent_command(command())?)?;
    let retained = session.project().tracks.first() == before.tracks.first();
    let tempo_retained = session.project().tempo_map == before.tempo_map;
    session.undo();
    let undo_restored = session.project() == &before;
    if !retained || !tempo_retained || !undo_restored {
        return Err("cover changed existing music/tempo or failed one-step Undo".into());
    }
    session.agent_command(command())?;
    let saved = session.save_as(destination)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "source": source,
            "project": saved.document,
            "inference_seconds": inference_seconds,
            "existing_music_retained": retained,
            "tempo_retained": tempo_retained,
            "one_undo_restored": undo_restored,
            "insertion_beat": 4,
            "response": response,
            "transcription": draft,
        }))?
    );
    Ok(())
}
