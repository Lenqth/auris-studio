//! Opt-in real-model evaluation through the actual window, worker and permission pipeline.

use super::*;
use auris_session::agent_policy::{Mode, Policy};
use serde::Deserialize;
use serde_json::{Value, json};
use std::fs;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evaluation {
    workspace: PathBuf,
    output: PathBuf,
    audio: Option<PathBuf>,
    prompt: PathBuf,
    model: String,
    effort: ReasoningEffort,
    timeout_seconds: u64,
    #[serde(default)]
    bundled_allowed: bool,
    #[serde(default)]
    natural_workflow: bool,
}

fn transcript(entries: &[ChatEntry]) -> Vec<Value> {
    entries
        .iter()
        .map(|entry| match entry {
            ChatEntry::You(text) => json!({"kind":"user","text":text}),
            ChatEntry::Agent(text) => json!({"kind":"assistant","text":text}),
            ChatEntry::Reasoning(text) => json!({"kind":"reasoning","text":text}),
            ChatEntry::Status(text) | ChatEntry::RestoredContext(text) => {
                json!({"kind":"status","text":text})
            }
            ChatEntry::Error(text) => json!({"kind":"error","text":text}),
            ChatEntry::Note(key) => json!({"kind":"note","key":format!("{key:?}")}),
            ChatEntry::Tool {
                name,
                args,
                ok,
                line,
                detail,
            } => json!({"kind":"tool","name":name,"args":args,"ok":ok,"line":line,"detail":detail}),
        })
        .collect()
}

#[gpui::test]
#[ignore = "real Codex inference and automatic approval review; requires AURIS_CODEX_TRANSCRIPTION_EVAL config"]
fn installed_codex_transcribes_via_new_apis_in_the_window(cx: &mut gpui::TestAppContext) {
    run(cx, true);
}

#[gpui::test]
#[ignore = "real Codex inference; requires AURIS_CODEX_TRANSCRIPTION_EVAL config"]
fn installed_codex_composes_named_sections_in_the_window(cx: &mut gpui::TestAppContext) {
    run(cx, false);
}

fn run(cx: &mut gpui::TestAppContext, cover: bool) {
    let config =
        std::env::var_os("AURIS_CODEX_TRANSCRIPTION_EVAL").expect("explicit evaluation config");
    let config: Evaluation = serde_json::from_slice(&fs::read(config).unwrap()).unwrap();
    assert!(
        config.workspace.is_absolute()
            && config.output.is_absolute()
            && config
                .audio
                .as_ref()
                .is_none_or(|audio| audio.is_absolute())
    );
    fs::create_dir(&config.output).expect("a fresh output directory");
    let prompt = fs::read_to_string(&config.prompt).unwrap();
    let (app, cx) = crate::harness::open(cx);
    let before = app.update(cx, |this, cx| {
        this.session.new_project();
        this.session
            .save_as(&config.workspace.join("Cover.auris"))
            .unwrap();
        this.settings.agent = AgentPreferences {
            backend: "codex".into(),
            model: config.model.clone(),
            effort: config.effort,
            policy: Policy {
                mode: Mode::AutoReview,
                ..Default::default()
            },
            ..Default::default()
        };
        this.agent_chat
            .load_preferences(&this.settings.agent.clone());
        this.panels.show(crate::dock::Panel::Agent);
        if let Some(audio) = &config.audio {
            this.agent_attach_files(std::slice::from_ref(audio), cx);
        }
        assert!(!cover || config.audio.is_some());
        assert!(this.agent_chat.attachment_error.is_none());
        this.agent_chat.input = TextField::new(prompt);
        let before = this.session.project().clone();
        this.agent_send();
        before
    });
    crate::harness::paint(&app, cx);
    let started = Instant::now();
    let mut last_snapshot = Instant::now() - Duration::from_secs(3);
    let mut last_progress = Instant::now() - Duration::from_secs(20);
    let mut failure = None;
    loop {
        app.update(cx, |this, cx| this.drain_agent(cx));
        cx.run_until_parked();
        if last_snapshot.elapsed() >= Duration::from_secs(2) {
            let snapshot=app.read_with(cx, |this,_| {
                json!({"elapsed_seconds":started.elapsed().as_secs_f64(),"busy":this.agent_chat.busy,
                    "dirty":this.session.is_dirty(),"project":this.session.path(),
                    "pending":this.agent_chat.has_pending_approval(),
                    "review_running":this.agent_chat.controls.review.is_some(),
                    "entries":transcript(&this.agent_chat.entries)})
            });
            fs::write(
                config.output.join("transcript.json"),
                serde_json::to_vec_pretty(&snapshot).unwrap(),
            )
            .unwrap();
            last_snapshot = Instant::now();
            if last_progress.elapsed() >= Duration::from_secs(20) {
                let last_tool = snapshot["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|e| e["kind"] == "tool");
                eprintln!(
                    "evaluation {:.1}s busy={} last_tool={}",
                    started.elapsed().as_secs_f64(),
                    snapshot["busy"],
                    last_tool.map_or("none", |t| t["name"].as_str().unwrap_or("unknown"))
                );
                last_progress = Instant::now();
            }
        }
        let state = app.read_with(cx, |this, _| {
            let answered = this
                .agent_chat
                .entries
                .iter()
                .any(|entry| matches!(entry, ChatEntry::Agent(_)));
            let error = this.agent_chat.entries.iter().find_map(|entry| {
                if let ChatEntry::Error(text) = entry {
                    Some(text.clone())
                } else {
                    None
                }
            });
            let pending = this.agent_chat.controls.pending.is_some()
                && this.agent_chat.controls.review.is_none();
            (answered && !this.agent_chat.busy, error, pending)
        });
        if let Some(error) = state.1 {
            failure = Some(error);
            break;
        }
        if state.2 {
            failure =
                Some("Automatic review needs a human decision; approval was not bypassed".into());
            break;
        }
        if state.0 {
            break;
        }
        if started.elapsed() > Duration::from_secs(config.timeout_seconds) {
            failure = Some("Evaluation timed out".into());
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let report=app.update(cx,|this,cx| {
        let entries=transcript(&this.agent_chat.entries);
        let imported=entries.iter().any(|e|e["name"]=="import_transcription" && e["ok"]==true);
        let exported=entries.iter().any(|e|e["name"]=="export_audio_attachment" && e["ok"]==true);
        let used_basic_pitch=entries.iter().any(|e|e["name"]=="create_piano_cover");
        let cover_added=imported || used_basic_pitch;
        let import_index=entries.iter().position(|e| matches!(e["name"].as_str(),Some("import_transcription"|"create_piano_cover")) && e["ok"]==true);
        let readback=import_index.is_some_and(|index|entries[index+1..].iter().any(|e|e["name"]=="read_notes" && e["ok"]==true && e["detail"].as_str().is_some_and(|s|!s.is_empty())));
        let inspected=import_index.is_some_and(|index|entries[index+1..].iter().any(|e|e["name"]=="inspect_audio" && e["ok"]==true && e["detail"].as_str().is_some_and(|s|!s.is_empty())));
        let after=this.session.project().clone();
        let dirty=this.session.is_dirty();
        fs::write(config.output.join("project-before.json"),serde_json::to_vec_pretty(&before).unwrap()).unwrap();
        fs::write(config.output.join("project-after.json"),serde_json::to_vec_pretty(&after).unwrap()).unwrap();
        this.agent_stop(cx);
        if cover && cover_added {this.session.undo();}
        let undo_restored=this.session.project()==&before;
        if cover && cover_added {this.session.redo();}
        let redo_restored=this.session.project()==&after;
        if cover_added || !cover && failure.is_none() {this.session.save_in_place().unwrap();}
        json!({"elapsed_seconds":started.elapsed().as_secs_f64(),"model":config.model,"effort":config.effort,
            "policy":"isolated in-memory AutoReview; actual app reviewer handles native commands",
            "failure":failure,"exported":exported,"imported":imported,"used_basic_pitch":used_basic_pitch,"cover_added":cover_added,"readback_after_import":readback,"inspected_after_import":inspected,
            "dirty_before_host_save":dirty,"one_undo_restored":undo_restored,"redo_restored":redo_restored,
            "sections":after.sections,"tracks":after.tracks.len(),"project":this.session.path(),"entries":entries})
    });
    fs::write(
        config.output.join("result.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(failure.is_none(), "{failure:?}");
    if cover {
        if config.bundled_allowed {
            assert_eq!(
                report["cover_added"], true,
                "an actual cover operation is required"
            );
        } else {
            assert_eq!(
                report["exported"], true,
                "actual attachment export required"
            );
            assert_eq!(report["imported"], true, "actual generic import required");
            assert_eq!(
                report["used_basic_pitch"], false,
                "independent recognition required"
            );
        }
        assert_eq!(report["dirty_before_host_save"], true);
        assert_eq!(report["readback_after_import"], true);
        assert_eq!(report["inspected_after_import"], true);
        if !config.natural_workflow {
            assert_eq!(report["one_undo_restored"], true);
        }
        assert_eq!(report["redo_restored"], true);
    } else {
        assert_eq!(report["dirty_before_host_save"], true);
        assert!(
            report["sections"]["points"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|point| point["label"].is_string())
                .count()
                >= 2,
            "named form required: {report}"
        );
        assert!(report["tracks"].as_u64().unwrap() > before.tracks.len() as u64);
    }
}
