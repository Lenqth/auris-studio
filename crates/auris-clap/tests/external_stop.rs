use std::path::PathBuf;

use auris_clap::ClapLibrary;
use auris_core::{AudioBuffer, Instrument, NoteEvent, PluginKind, PrepareContext, ProcessContext};

#[test]
#[ignore = "requires AURIS_CLAP_SMOKE_PLUGIN to point to an external CLAP instrument"]
fn stopping_an_external_instrument_releases_held_notes() {
    let path = PathBuf::from(
        std::env::var_os("AURIS_CLAP_SMOKE_PLUGIN")
            .expect("AURIS_CLAP_SMOKE_PLUGIN must name a CLAP instrument"),
    );
    // SAFETY: the person running this ignored test explicitly selected the library.
    let library = unsafe { ClapLibrary::load(path) }.expect("the library should load");
    let info = library
        .plugins()
        .unwrap()
        .into_iter()
        .find(|info| info.kind == PluginKind::Instrument)
        .expect("the library should contain an instrument");
    let mut plugin = library.instantiate(&info.clap_id).unwrap();
    let prepare = PrepareContext::new(48_000.0, 512, 2);
    let mut instrument = plugin.activate_instrument(&prepare).unwrap();
    let mut out = AudioBuffer::stereo(512, 48_000.0);
    let playing = ProcessContext::realtime(48_000.0, 512, 0, 120.0, true);
    for _ in 0..16 {
        instrument.process(&[], &mut out, &playing);
    }
    instrument.process(
        &[NoteEvent::NoteOn {
            frame: 0,
            pitch: 60,
            velocity: 0.8,
        }],
        &mut out,
        &playing,
    );
    let mut peak = out.peak();
    for _ in 0..64 {
        instrument.process(&[], &mut out, &playing);
        peak = peak.max(out.peak());
    }
    assert!(peak > 1.0e-5, "the instrument must sound before stopping");

    instrument.reset();
    instrument.reset(); // Stop followed by returning the playhead to zero.
    let stopped = ProcessContext {
        is_playing: false,
        ..playing
    };
    for _ in 0..1_000 {
        instrument.process(&[], &mut out, &stopped);
    }
    assert!(!instrument.processing_failed());
    assert!(
        out.peak() < 1.0e-5,
        "{} kept sounding after stop: {}",
        info.name,
        out.peak()
    );

    // Seeking while playing must let the newly chased note sound after the reset.
    instrument.reset();
    instrument.process(
        &[NoteEvent::NoteOn {
            frame: 0,
            pitch: 64,
            velocity: 0.8,
        }],
        &mut out,
        &playing,
    );
    let mut peak = out.peak();
    for _ in 0..64 {
        instrument.process(&[], &mut out, &playing);
        peak = peak.max(out.peak());
    }
    assert!(peak > 1.0e-5, "the new note must survive the pending reset");
    plugin.deactivate_instrument(instrument);
}
