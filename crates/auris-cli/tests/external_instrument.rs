//! Opt-in end-to-end rendering with an installed VST3 instrument.

use std::path::PathBuf;

use auris_session::prelude::{Note, OfflineOptions, PluginKind, RenderProgress, Ticks};
use auris_session::{Session, SessionOptions};

#[test]
#[ignore = "requires AURIS_VST3_SMOKE_PLUGIN to name an installed VST3 instrument"]
fn an_external_vst3_instrument_sounds_in_a_session_render() {
    let path = PathBuf::from(
        std::env::var_os("AURIS_VST3_SMOKE_PLUGIN")
            .expect("AURIS_VST3_SMOKE_PLUGIN must name a VST3 instrument"),
    );
    let mut session = Session::new(SessionOptions::headless()).unwrap();
    let track = session.add_default_instrument_track("VST3 probe").unwrap();
    let clip = session
        .add_midi_clip(track, "Note", Ticks::ZERO, Ticks::QUARTER)
        .unwrap();
    session
        .add_note(clip, Note::new(60, Ticks::ZERO, Ticks::QUARTER))
        .unwrap();

    let info = session
        .vst3_plugins_in(&path)
        .unwrap()
        .into_iter()
        .find(|info| info.kind == PluginKind::Instrument)
        .expect("the selected bundle must contain an instrument");
    eprintln!("rendering through {} {}", info.vendor, info.name);
    session
        .set_vst3_instrument(track, &path, &info.class_id)
        .unwrap();
    let mut job = session.render_job();
    assert!(
        session.take_vst3_render_error().is_none(),
        "the VST3 instrument must be placed into the render graph"
    );
    let audio = job
        .render(&OfflineOptions::default(), &mut RenderProgress::default())
        .unwrap();
    assert!(
        audio
            .iter_channels()
            .flatten()
            .all(|sample| sample.is_finite()),
        "the instrument output must be finite"
    );
    eprintln!("instrument peak={:.6}", audio.peak());
    assert!(audio.peak() > 1.0e-5, "the instrument must return audio");
}
