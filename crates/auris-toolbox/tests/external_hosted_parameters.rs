//! Opt-in checks through the same flat tool dispatch used by the AI agent and MCP.

use std::path::PathBuf;

use auris_session::prelude::*;
use auris_session::{Session, SessionOptions};
use serde_json::{Value, json};

fn live(session: &mut Session, tool: &str, args: Value) -> Result<Value, String> {
    let command = auris_toolbox::live_agent::command(tool, &args)?.expect("document tool");
    serde_json::from_str(&session.agent_command(command)?).map_err(|error| error.to_string())
}

fn saved(project: &str, tool: &str, mut args: Value) -> Result<Value, String> {
    args["project"] = project.into();
    let reply =
        auris_toolbox::runtime::call_saved(&auris_toolbox::ToolGroups::default(), tool, args)?;
    serde_json::from_str(&reply).map_err(|error| error.to_string())
}

fn render(session: &mut Session) -> AudioBuffer {
    let mut job = session.render_job();
    assert!(
        session.take_vst3_render_error().is_none(),
        "hosted renderer must load"
    );
    let audio = job
        .render(
            &OfflineOptions::whole_project(),
            &mut RenderProgress::default(),
        )
        .unwrap();
    assert!(
        audio
            .iter_channels()
            .flatten()
            .all(|sample| sample.is_finite())
    );
    audio
}

#[test]
#[ignore = "requires AURIS_AGENT_SMOKE_PLUGIN and AURIS_AGENT_SMOKE_CONTROL (exact writable name); optional AURIS_AGENT_SMOKE_VALUE"]
fn external_parameters_round_trip_through_live_and_saved_agent_tools() {
    let file = PathBuf::from(std::env::var_os("AURIS_AGENT_SMOKE_PLUGIN").unwrap());
    let control = std::env::var("AURIS_AGENT_SMOKE_CONTROL").unwrap();
    let effect = std::env::var_os("AURIS_AGENT_SMOKE_EFFECT").is_some();
    let vst3 = file.extension().unwrap().eq_ignore_ascii_case("vst3");
    let mut session =
        Session::new(SessionOptions::headless().with_plugin_paths(vec![file.clone()])).unwrap();
    let track = session.add_default_instrument_track("Probe").unwrap();
    let clip = session
        .add_midi_clip(track, "Held note", Ticks::ZERO, Ticks::QUARTER * 4)
        .unwrap();
    session
        .add_note(clip, Note::new(60, Ticks::ZERO, Ticks::QUARTER * 3))
        .unwrap();
    let kind = if effect {
        PluginKind::Effect
    } else {
        PluginKind::Instrument
    };
    let slot = if vst3 {
        let info = session
            .vst3_plugins_in(&file)
            .unwrap()
            .into_iter()
            .find(|info| info.kind == kind)
            .unwrap();
        eprintln!("VST3 {} ({})", info.name, info.class_id);
        if effect {
            Some(
                session
                    .add_vst3_effect(Some(track), &file, &info.class_id)
                    .unwrap(),
            )
        } else {
            session
                .set_vst3_instrument(track, &file, &info.class_id)
                .unwrap();
            None
        }
    } else {
        let info = session
            .hosted_plugins_in(&file)
            .unwrap()
            .into_iter()
            .find(|info| info.kind == kind)
            .unwrap();
        eprintln!("CLAP {} ({})", info.name, info.clap_id);
        if effect {
            Some(
                session
                    .add_hosted_effect(Some(track), &file, &info.clap_id)
                    .unwrap(),
            )
        } else {
            session
                .set_hosted_instrument(track, &file, &info.clap_id)
                .unwrap();
            None
        }
    };
    session.poll();
    // Some installed effects start bypassed or without an active algorithm. Allow explicit
    // setup through the agent too, so the measured control is tested on an audible processor.
    if let Ok(setup) = std::env::var("AURIS_AGENT_SMOKE_SETUP") {
        let setup: Vec<Value> = serde_json::from_str(&setup).unwrap();
        for edit in setup {
            if let Some(slot) = slot {
                live(&mut session, "mixer_graph", json!({"operation":{"action":"set_parameter","node":format!("effect:{}",slot.0),"param":edit["param"],"value":edit["value"]}})).unwrap();
            } else {
                live(&mut session, "set_instrument_params", json!({"edits":[{"track":track.0,"param":edit["param"],"value":edit["value"]}]})).unwrap();
            }
        }
    }
    let descriptors = if let Some(slot) = slot {
        session.effect_descriptors(slot)
    } else {
        session.instrument_descriptors(track)
    };
    let matches: Vec<_> = descriptors
        .iter()
        .enumerate()
        .filter(|(_, descriptor)| descriptor.name.as_ref() == control && !descriptor.read_only)
        .collect();
    if matches.len() != 1 {
        for descriptor in descriptors.iter() {
            eprintln!(
                "{}: {} [{}..{}]",
                descriptor.key, descriptor.name, descriptor.min, descriptor.max
            );
        }
    }
    assert_eq!(matches.len(), 1, "select an unambiguous control name");
    let (index, descriptor) = matches[0];
    let inspect = if effect {
        "mixer_graph"
    } else {
        "inspect_instrument"
    };
    let inspect_args = slot.map_or_else(|| json!({"track":track.0,"offset":index}), |slot| json!({"operation":{"action":"parameters","node":format!("effect:{}",slot.0),"offset":index}}));
    let read = live(&mut session, inspect, inspect_args.clone()).unwrap();
    let parameter = &read["parameters"][0];
    assert_eq!(parameter["key"], descriptor.key.as_ref());
    assert_eq!(parameter["read_only"], false);
    assert_eq!(
        parameter["value_domain"],
        if vst3 { "normalized" } else { "native" }
    );
    assert_eq!(parameter["steps"], json!(descriptor.steps));
    let original = parameter["value"].as_f64().unwrap() as f32;
    let quiet = std::env::var("AURIS_AGENT_SMOKE_VALUE")
        .ok()
        .map(|value| value.parse::<f32>().unwrap())
        .unwrap_or(descriptor.min);
    assert_ne!(
        original, quiet,
        "select a level control initially above its minimum"
    );
    let edit = if effect {
        "mixer_graph"
    } else {
        "set_instrument_params"
    };
    let edit_args = |value: f32| {
        slot.map_or_else(|| json!({"edits":[{"track":track.0,"param":descriptor.key,"value":value}]}), |slot| json!({"operation":{"action":"set_parameter","node":format!("effect:{}",slot.0),"param":descriptor.key,"value":value}}))
    };

    let loud_audio = render(&mut session);
    let loud_rms = loud_audio.channel_rms(0);
    assert!(
        loud_rms > 1e-5,
        "the initial patch must produce audio: {loud_rms}"
    );
    session.forget_history();
    let before = session.project().clone();
    assert!(live(&mut session, edit, edit_args(descriptor.max + 100.0)).is_err());
    assert_eq!(session.project(), &before);
    live(&mut session, edit, edit_args(quiet)).unwrap();
    assert_eq!(
        live(&mut session, inspect, inspect_args.clone()).unwrap()["parameters"][0]["value"],
        json!(quiet)
    );
    let quiet_audio = render(&mut session);
    let quiet_rms = quiet_audio.channel_rms(0);
    eprintln!(
        "{} {} -> {}; RMS {loud_rms:.8} -> {quiet_rms:.8}",
        descriptor.key, original, quiet
    );
    let difference = loud_audio
        .iter_channels()
        .flatten()
        .zip(quiet_audio.iter_channels().flatten())
        .map(|(a, b)| (a - b).abs() as f64)
        .sum::<f64>()
        / (loud_audio.frame_count() * loud_audio.channel_count()) as f64;
    assert!(
        difference > f64::from(loud_rms) * 0.01,
        "parameter must change rendered audio: {difference}"
    );
    if std::env::var_os("AURIS_AGENT_SMOKE_VALUE").is_none() {
        assert!(
            quiet_rms < loud_rms * 0.8,
            "level must change rendered audio"
        );
    }
    live(&mut session, edit, edit_args(quiet)).unwrap();
    session.undo().expect("one parameter edit");
    assert_eq!(session.project(), &before);
    assert!(
        session.undo().is_none(),
        "retry and rejected edit must not add history"
    );
    session.redo().expect("restore quiet parameter");

    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("Probe.auris");
    session.save(&path).unwrap();
    let project = path.to_str().unwrap();
    assert_eq!(
        saved(project, inspect, inspect_args.clone()).unwrap()["parameters"][0]["value"],
        json!(quiet)
    );
    saved(project, edit, edit_args(original)).unwrap();
    assert_eq!(
        saved(project, inspect, inspect_args.clone()).unwrap()["parameters"][0]["value"],
        json!(original)
    );
    let bytes = std::fs::read(&path).unwrap();
    saved(project, edit, edit_args(original)).unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "saved retry must not save again"
    );
    let mut reopened =
        Session::new(SessionOptions::headless().with_plugin_paths(vec![file])).unwrap();
    reopened.open(&path).unwrap();
    let restored_rms = render(&mut reopened).channel_rms(0);
    assert!(
        restored_rms > loud_rms * 0.8,
        "saved dispatch must restore the sound: {restored_rms}"
    );
}
