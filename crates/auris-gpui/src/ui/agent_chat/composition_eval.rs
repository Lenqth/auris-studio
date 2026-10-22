//! Opt-in, multi-turn composition trials through the real built-in Codex host.

use super::*;
use auris_session::{
    agent_policy::{Mode, Policy},
    prelude::*,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evaluation {
    model: String,
    output: PathBuf,
    timeout_seconds: u64,
    fixture: String,
    prompts: Vec<String>,
}

fn transcript(entries: &[ChatEntry]) -> Vec<Value> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            ChatEntry::You { text, attachments } => {
                Some(json!({"kind":"user","text":text,"attachments":attachments}))
            }
            ChatEntry::Agent(text) => Some(json!({"kind":"assistant","text":text})),
            ChatEntry::Error(text) => Some(json!({"kind":"error","text":text})),
            ChatEntry::Tool {
                name,
                args,
                ok,
                detail,
                ..
            } => Some(json!({"kind":"tool","name":name,"args":args,"ok":ok,"detail":detail})),
            _ => None,
        })
        .collect()
}

#[gpui::test]
#[ignore = "real Codex inference; requires AURIS_COMPOSITION_EVAL config"]
fn codex_composes_and_revises_the_open_document(cx: &mut gpui::TestAppContext) {
    let path = std::env::var_os("AURIS_COMPOSITION_EVAL").expect("explicit evaluation config");
    let config: Evaluation = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(config.output.is_absolute());
    assert!(!config.prompts.is_empty());
    fs::create_dir(&config.output).expect("a fresh evidence folder");
    let (app, cx) = crate::harness::open(cx);
    app.update(cx, |this, _| {
        this.session.new_project();
        match config.fixture.as_str() {
            "blank" => {}
            "melody_six_eight" => {
                this.session.set_tempo_at(Ticks::ZERO, 90.0);
                this.session
                    .set_signature_at(Ticks::ZERO, "6/8".parse().unwrap());
                let track = this
                    .session
                    .add_default_instrument_track("手書きのメロディー")
                    .unwrap();
                let clip = this
                    .session
                    .add_midi_clip(track, "残したい旋律", Ticks::ZERO, Ticks(11520))
                    .unwrap();
                let notes = (0..8)
                    .flat_map(|bar| {
                        let pitches =
                            [[69, 72, 76], [67, 71, 74], [65, 69, 72], [64, 68, 71]][bar % 4];
                        pitches.into_iter().enumerate().map(move |(i, pitch)| {
                            let mut note = Note::new(
                                pitch,
                                Ticks(bar as i64 * 1440 + i as i64 * 480),
                                Ticks(420),
                            );
                            note.velocity = 0.61 + i as f32 * 0.07;
                            note
                        })
                    })
                    .collect();
                this.session.replace_notes(clip, notes).unwrap();
                this.session
                    .set_curve_point(clip, ClipCurve::Controller(1), Ticks(333), 0.37);
            }
            other => panic!("unknown fixture {other}"),
        }
        this.settings.agent = AgentPreferences {
            backend: "codex".into(),
            model: config.model.clone(),
            // Fixture-only trials isolate tool selection from interactive approval timing.
            policy: Policy {
                mode: Mode::Bypass,
                ..Default::default()
            },
            ..Default::default()
        };
        this.agent_chat
            .load_preferences(&this.settings.agent.clone());
        this.panels.show(crate::dock::Panel::Agent);
        fs::write(
            config.output.join("project-00.json"),
            serde_json::to_vec_pretty(this.session.project()).unwrap(),
        )
        .unwrap();
    });
    crate::harness::paint(&app, cx);
    let started = Instant::now();
    let mut turns = Vec::new();
    for (index, prompt) in config.prompts.iter().enumerate() {
        let offset = app.update(cx, |this, _| {
            let offset = this.agent_chat.entries.len();
            this.agent_chat.input =
                TextField::new(prompt.replace("{output}", &config.output.to_string_lossy()));
            this.agent_send();
            offset
        });
        let turn_started = Instant::now();
        let mut snapshot = Instant::now() - Duration::from_secs(5);
        let failure = loop {
            app.update(cx, |this, cx| this.drain_agent(cx));
            cx.run_until_parked();
            let (finished, error, pending) = app.read_with(cx, |this, _| {
                let entries = &this.agent_chat.entries[offset..];
                (
                    !this.agent_chat.busy
                        && entries.iter().any(|e| matches!(e, ChatEntry::Agent(_))),
                    entries.iter().find_map(|e| {
                        if let ChatEntry::Error(text) = e {
                            Some(text.clone())
                        } else {
                            None
                        }
                    }),
                    this.agent_chat.has_pending_approval(),
                )
            });
            if snapshot.elapsed() >= Duration::from_secs(5) {
                let entries = app.read_with(cx, |this, _| transcript(&this.agent_chat.entries));
                fs::write(
                    config.output.join("transcript.json"),
                    serde_json::to_vec_pretty(&entries).unwrap(),
                )
                .unwrap();
                eprintln!(
                    "turn {}: {:.1}s, {} entries",
                    index + 1,
                    turn_started.elapsed().as_secs_f64(),
                    entries.len()
                );
                snapshot = Instant::now();
            }
            if error.is_some() {
                break error;
            }
            if pending {
                break Some("Unexpected approval in isolated bypass fixture".into());
            }
            if finished {
                break None;
            }
            if turn_started.elapsed() > Duration::from_secs(config.timeout_seconds) {
                break Some("Real-model turn timed out".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        app.read_with(cx, |this, _| {
            fs::write(
                config.output.join(format!("project-{:02}.json", index + 1)),
                serde_json::to_vec_pretty(this.session.project()).unwrap(),
            )
            .unwrap();
            fs::write(
                config.output.join("transcript.json"),
                serde_json::to_vec_pretty(&transcript(&this.agent_chat.entries)).unwrap(),
            )
            .unwrap();
        });
        turns.push(json!({"turn":index+1,"elapsed_seconds":turn_started.elapsed().as_secs_f64(),"failure":failure}));
        if failure.is_some() {
            break;
        }
    }
    let report = app.update(cx, |this, cx| {
        this.agent_stop(cx);
        let entries = transcript(&this.agent_chat.entries);
        let calls = entries.iter().filter(|e|e["kind"]=="tool").collect::<Vec<_>>();
        let wav = config.output.join("result.wav");
        let midi = config.output.join("result.mid");
        let midi_notes = {
            let mut check = Session::new(auris_session::SessionOptions::headless()).unwrap();
            check.import_midi(&midi).ok().map(|report|report.notes)
        };
        json!({"model":config.model,"fixture":config.fixture,"elapsed_seconds":started.elapsed().as_secs_f64(),
            "policy":"isolated generated fixture, explicit bypass; real application host",
            "turns":turns,"unsaved":this.session.path().is_none(),"midi_notes":midi_notes,
            "encoded":auris_session::inspect_export(&wav).ok(),
            "argument_or_tool_errors":calls.iter().filter(|e|e["ok"]==false).count(),"transcript":entries})
    });
    fs::write(
        config.output.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(
        report["turns"]
            .as_array()
            .unwrap()
            .iter()
            .all(|turn| turn["failure"].is_null()),
        "see evaluation report: {}",
        config.output.display()
    );
}
