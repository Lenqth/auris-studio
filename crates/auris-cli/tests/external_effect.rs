//! Opt-in end-to-end rendering with installed CLAP and VST3 effects.

use std::path::PathBuf;

use auris_session::prelude::{Note, OfflineOptions, PluginKind, RenderProgress, Ticks};
use auris_session::{Session, SessionOptions};

#[test]
#[ignore = "requires AURIS_CLAP_EFFECT_SMOKE_PLUGIN to name an installed CLAP effect"]
fn an_external_clap_effect_changes_a_session_render() {
    external_effect("AURIS_CLAP_EFFECT_SMOKE_PLUGIN", false);
}

#[test]
#[ignore = "requires AURIS_VST3_EFFECT_SMOKE_PLUGIN to name an installed VST3 effect"]
fn an_external_vst3_effect_changes_a_session_render() {
    external_effect("AURIS_VST3_EFFECT_SMOKE_PLUGIN", true);
}

fn external_effect(variable: &str, vst3: bool) {
    let path = PathBuf::from(std::env::var_os(variable).expect("the plugin path is required"));
    let mut session = Session::new(SessionOptions::headless()).unwrap();
    let track = session
        .add_default_instrument_track("Effect probe")
        .unwrap();
    let clip = session
        .add_midi_clip(track, "Note", Ticks::ZERO, Ticks::QUARTER)
        .unwrap();
    session
        .add_note(clip, Note::new(60, Ticks::ZERO, Ticks::QUARTER))
        .unwrap();
    let baseline = session
        .render_job()
        .render(&OfflineOptions::default(), &mut RenderProgress::default())
        .unwrap();
    assert!(baseline.peak() > 1.0e-5, "the source track must sound");

    if vst3 {
        let effect = session
            .vst3_plugins_in(&path)
            .unwrap()
            .into_iter()
            .find(|info| info.kind == PluginKind::Effect)
            .expect("the selected bundle must contain an effect");
        eprintln!("rendering through {} {}", effect.vendor, effect.name);
        session
            .add_vst3_effect(Some(track), &path, &effect.class_id)
            .unwrap();
    } else {
        let effect = session
            .hosted_plugins_in(&path)
            .unwrap()
            .into_iter()
            .find(|info| info.kind == PluginKind::Effect)
            .expect("the selected library must contain an effect");
        eprintln!("rendering through {} {}", effect.vendor, effect.name);
        session
            .add_hosted_effect(Some(track), &path, &effect.clap_id)
            .unwrap();
    }
    let processed = session
        .render_job()
        .render(&OfflineOptions::default(), &mut RenderProgress::default())
        .unwrap();
    assert!(
        processed
            .iter_channels()
            .flatten()
            .all(|sample| sample.is_finite()),
        "the effect output must be finite"
    );
    assert!(processed.peak() > 1.0e-5, "the effect must return audio");
    let difference = baseline
        .iter_channels()
        .flatten()
        .zip(processed.iter_channels().flatten())
        .map(|(original, changed)| (original - changed).abs())
        .fold(0.0_f32, f32::max);
    eprintln!(
        "source peak={:.6}, effect peak={:.6}, max difference={difference:.6}",
        baseline.peak(),
        processed.peak()
    );
    assert!(difference > 1.0e-4, "the effect did not change the render");
}
