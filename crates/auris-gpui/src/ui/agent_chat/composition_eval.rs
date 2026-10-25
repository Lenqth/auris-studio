//! Opt-in, multi-turn composition trials through the real application agent hosts.

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
    #[serde(default = "codex_backend")]
    backend: String,
    #[serde(default = "context_tokens")]
    context_tokens: u32,
    model: String,
    output: PathBuf,
    timeout_seconds: u64,
    fixture: String,
    prompts: Vec<String>,
    #[serde(default)]
    prompt_condition: Option<PromptCondition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromptCondition {
    expected_prefix: String,
    replacement_prefix: String,
    skill_context: String,
}

impl PromptCondition {
    fn apply(&self, current: &str) -> Result<String, &'static str> {
        // Git checkouts may use CRLF; compare the content rather than checkout encoding.
        let current = current.replace("\r\n", "\n");
        let expected = self.expected_prefix.replace("\r\n", "\n");
        if expected.trim().is_empty() || self.replacement_prefix.trim().is_empty() {
            return Err("Prompt condition prefixes must be nonempty");
        }
        let suffix = current
            .strip_prefix(&expected)
            .ok_or("Frozen prompt prefix does not match this evaluation binary")?;
        let mut prompt = self.replacement_prefix.replace("\r\n", "\n");
        prompt.push_str(suffix);
        if !self.skill_context.trim().is_empty() {
            prompt.push_str("\n\n<composition_skill_context>\n");
            prompt.push_str(&self.skill_context.replace("\r\n", "\n"));
            prompt.push_str("\n</composition_skill_context>\n");
        }
        Ok(prompt)
    }
}

#[test]
fn prompt_condition_preserves_operational_suffix_and_freezes_skill_context() {
    let condition = PromptCondition {
        expected_prefix: "Current composition\n".into(),
        replacement_prefix: "Baseline composition\n".into(),
        skill_context: "Frozen skill\r\nReference".into(),
    };
    assert_eq!(
        condition
            .apply("Current composition\r\nTool and safety contract")
            .unwrap(),
        "Baseline composition\nTool and safety contract\n\n<composition_skill_context>\nFrozen skill\nReference\n</composition_skill_context>\n"
    );
}

#[test]
fn prompt_condition_rejects_stale_binary_or_empty_prefix() {
    let mut condition = PromptCondition {
        expected_prefix: "Current composition".into(),
        replacement_prefix: "Baseline composition".into(),
        skill_context: String::new(),
    };
    assert!(condition.apply("Different binary").is_err());
    condition.expected_prefix.clear();
    assert!(condition.apply("Current composition").is_err());
}

fn codex_backend() -> String {
    "codex".into()
}
fn context_tokens() -> u32 {
    65536
}

fn melody_fixture(quarter: Ticks) -> (Ticks, Vec<Note>) {
    let notes = (0..8)
        .flat_map(|bar| {
            let pitches = [[69, 72, 76], [67, 71, 74], [65, 69, 72], [64, 68, 71]][bar % 4];
            pitches.into_iter().enumerate().map(move |(i, pitch)| {
                let mut note = Note::new(
                    pitch,
                    Ticks((bar as i64 * 3 + i as i64) * quarter.0),
                    Ticks(quarter.0 * 7 / 8),
                );
                note.velocity = 0.61 + i as f32 * 0.07;
                note
            })
        })
        .collect();
    (Ticks(24 * quarter.0), notes)
}

#[test]
fn versioned_melody_fixture_spans_eight_bars_without_changing_legacy_data() {
    let signature: TimeSignature = "6/8".parse().unwrap();
    let (length, notes) = melody_fixture(Ticks::QUARTER);
    assert_eq!(length, signature.ticks_per_bar() * 8);
    assert_eq!(length.as_beats() * 60.0 / 90.0, 16.0);
    assert_eq!(notes.len(), 24);
    assert_eq!(notes[2].start, Ticks(1920));
    assert_eq!(notes[2].length, Ticks(840));
    assert_eq!(notes[2].pitch, 76);
    let (legacy_length, legacy_notes) = melody_fixture(Ticks(480));
    assert_eq!(legacy_length, Ticks(11520));
    assert_eq!(legacy_notes[2].start, Ticks(960));
    assert_eq!(legacy_notes[2].length, Ticks(420));
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
#[ignore = "real model inference; requires AURIS_COMPOSITION_EVAL config"]
fn codex_composes_and_revises_the_open_document(cx: &mut gpui::TestAppContext) {
    let path = std::env::var_os("AURIS_COMPOSITION_EVAL").expect("explicit evaluation config");
    let config: Evaluation = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(config.output.is_absolute());
    assert!(!config.prompts.is_empty());
    assert!(matches!(
        config.backend.as_str(),
        "codex" | "claude" | "rig"
    ));
    fs::create_dir(&config.output).expect("a fresh evidence folder");
    let system_prompt = config.prompt_condition.as_ref().map(|condition| {
        condition
            .apply(&auris_agent::default_system_prompt())
            .expect("an explicitly frozen prompt condition matching this binary")
    });
    fs::write(
        config.output.join("system-prompt.txt"),
        system_prompt
            .clone()
            .unwrap_or_else(auris_agent::default_system_prompt),
    )
    .unwrap();
    let (app, cx) = crate::harness::open(cx);
    app.update(cx, |this, _| {
        this.session.new_project();
        match config.fixture.as_str() {
            "blank" => {}
            "melody_six_eight" | "melody_six_eight_eight_bars" => {
                // Keep v1's four-bar seed unchanged for frozen paired comparisons.
                let quarter = if config.fixture == "melody_six_eight" {
                    Ticks(480)
                } else {
                    Ticks::QUARTER
                };
                let (length, notes) = melody_fixture(quarter);
                this.session.set_tempo_at(Ticks::ZERO, 90.0);
                this.session
                    .set_signature_at(Ticks::ZERO, "6/8".parse().unwrap());
                let track = this
                    .session
                    .add_default_instrument_track("手書きのメロディー")
                    .unwrap();
                let clip = this
                    .session
                    .add_midi_clip(track, "残したい旋律", Ticks::ZERO, length)
                    .unwrap();
                this.session.replace_notes(clip, notes).unwrap();
                this.session
                    .set_curve_point(clip, ClipCurve::Controller(1), Ticks(333), 0.37);
            }
            other => panic!("unknown fixture {other}"),
        }
        this.settings.agent = AgentPreferences {
            backend: config.backend.clone(),
            provider: "ollama".into(),
            context_tokens: Some(config.context_tokens),
            model: config.model.clone(),
            system_prompt: system_prompt.clone().unwrap_or_default(),
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
        let usage = app.read_with(cx, |this, _| json!({
            "last_reported_input_context_tokens":this.agent_chat.tokens_in,
            "conversation_reported_output_tokens":this.agent_chat.tokens_out,
            "interpretation":"Backend-reported counters; input is the last reported context, not cumulative billable input. Zero may mean unavailable."}));
        turns.push(json!({"turn":index+1,"elapsed_seconds":turn_started.elapsed().as_secs_f64(),"failure":failure,"usage":usage}));
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
        json!({"model":config.model,"backend":config.backend,"prompt_profile":if std::env::var("AURIS_COMPOSITION_WORKFLOWS").is_ok_and(|value| value == "1") {"modular"} else {"legacy"},"fixture":config.fixture,"elapsed_seconds":started.elapsed().as_secs_f64(),
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
