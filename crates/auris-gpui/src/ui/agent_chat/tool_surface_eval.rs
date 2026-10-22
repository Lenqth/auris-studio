//! Opt-in real-model checks of canonical edits and unsaved delivery through the window.

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
    backend: String,
    model: String,
    output: PathBuf,
    timeout_seconds: u64,
    provider: Option<String>,
    url: Option<String>,
    api_key_env: Option<String>,
}

fn transcript(entries: &[ChatEntry]) -> Vec<Value> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            ChatEntry::You(text) => Some(json!({"kind":"user","text":text})),
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
#[ignore = "real Claude/Codex/rig inference; requires AURIS_TOOL_SURFACE_EVAL config"]
fn installed_agent_edits_and_exports_the_unsaved_document(cx: &mut gpui::TestAppContext) {
    let path = std::env::var_os("AURIS_TOOL_SURFACE_EVAL").expect("explicit evaluation config");
    let config: Evaluation = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(matches!(
        config.backend.as_str(),
        "claude" | "codex" | "rig"
    ));
    assert!(config.output.is_absolute());
    fs::create_dir(&config.output).expect("a fresh evidence folder");
    let midi = config.output.join("result.mid");
    let wav = config.output.join("result.wav");
    let (app, cx) = crate::harness::open(cx);
    let (track,clip,before)=app.update(cx,|this,_| {
        this.session.new_project();
        let track=this.session.add_default_instrument_track("Canonical tools trial").unwrap();
        let clip=this.session.add_midi_clip(track,"Original notes",Ticks::ZERO,Ticks(3840)).unwrap();
        this.session.replace_notes(clip,vec![Note::new(60,Ticks::ZERO,Ticks(480)),Note::new(64,Ticks(960),Ticks(480)),Note::new(67,Ticks(1920),Ticks(480))]).unwrap();
        this.session.set_curve_point(clip,ClipCurve::Controller(7),Ticks(333),0.4);
        let before=this.session.project().clone();
        this.settings.agent=AgentPreferences {
            backend:config.backend.clone(),model:config.model.clone(),
            provider:config.provider.clone().unwrap_or_else(||"openai".into()),
            url:config.url.clone().unwrap_or_default(),
            api_key_env:config.api_key_env.clone().unwrap_or_default(),
            // Explicitly isolated test fixture: delivery writes only new paths in its evidence folder.
            policy:Policy{mode:Mode::Bypass,..Default::default()},
            ..Default::default()
        };
        this.agent_chat.load_preferences(&this.settings.agent.clone());
        this.panels.show(crate::dock::Panel::Agent);
        this.agent_chat.input=TextField::new(format!(
            "共通ツールの実動作確認です。この未保存曲だけを次の通り編集してください。最初にinspect_projectを実行。トラックID {} のミキサーをgain_db=-6、pan=0.25にする。クリップID {} のread_notesを実行し、格納インデックス0の音符だけstart_tick=13、length_ticks=997、velocity=0.73125に更新する。ほかの音符は保持。さらにedit_clip_curveでCC64を読み、同じクリップのCC64を位置0で1.0、位置960で0.0の2点へreplaceする（ほかのCCは保持）。各編集は1回ずつ行い、変更後にmixer_graph/read_notes/edit_clip_curveで独立して読み戻してください。次にpreviewで1小節目だけを試聴用ファイルにし、export_midiで {} に出力。renderでdestination kind mix、path {}、range start_bar=1,bars=1、24bitで書き出し、verify_renderで実際のWAVを確認してください。ファイルはすべて新規です。必要ならtool_helpで正確な引数を確認してください。元の曲の保存、追加トラック、不要な編集、外部ネットワーク検索、音楽生成は不要です。最後に実際に得た結果だけを短く報告してください。",
            track.0,clip.0,serde_json::to_string(&midi).unwrap(),serde_json::to_string(&wav).unwrap()
        ));
        this.agent_send();
        (track,clip,before)
    });
    crate::harness::paint(&app, cx);
    let started = Instant::now();
    let mut snapshot = Instant::now() - Duration::from_secs(5);
    let failure = loop {
        app.update(cx, |this, cx| this.drain_agent(cx));
        cx.run_until_parked();
        let (finished, error, pending) = app.read_with(cx, |this, _| {
            (
                !this.agent_chat.busy
                    && this
                        .agent_chat
                        .entries
                        .iter()
                        .any(|e| matches!(e, ChatEntry::Agent(_))),
                this.agent_chat.entries.iter().find_map(|e| {
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
                "{} {:.1}s, {} entries",
                config.backend,
                started.elapsed().as_secs_f64(),
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
        if started.elapsed() > Duration::from_secs(config.timeout_seconds) {
            break Some("Real-model evaluation timed out".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let report=app.update(cx,|this,cx| {
        let entries=transcript(&this.agent_chat.entries);
        this.agent_stop(cx);
        let after=this.session.project().clone();
        let mut expected=before.clone();
        expected.track_mut(track).unwrap().mixer.gain_db = -6.0;
        expected.track_mut(track).unwrap().mixer.pan=0.25;
        let target=expected.midi_clip_mut(clip).unwrap();
        target.notes[0].start=Ticks(13);
        target.notes[0].length=Ticks(997);
        target.notes[0].velocity=0.73125;
        target.controllers.insert(64,vec![CurvePoint{at:Ticks::ZERO,value:1.0},CurvePoint{at:Ticks(960),value:0.0}]);
        let exact_requested_edit=after==expected;
        let mut undo_steps=0;
        while this.session.project()!=&before && undo_steps<8 {
            if this.session.undo().is_none() { break; }
            undo_steps+=1;
        }
        let undo_restored=this.session.project()==&before;
        for _ in 0..undo_steps { this.session.redo(); }
        let redo_restored=this.session.project()==&after;
        let calls=entries.iter().filter(|e|e["kind"]=="tool").collect::<Vec<_>>();
        let successful=|name:&str|calls.iter().filter(|e|e["name"]==name && e["ok"]==true).count();
        let measurements=auris_session::inspect_export(&wav).ok();
        let midi_notes={
            let mut check=Session::new(auris_session::SessionOptions::headless()).unwrap();
            check.import_midi(&midi).ok().map(|report|report.notes)
        };
        for (name,project) in [("project-before.json",&before),("project-after.json",&after)] { fs::write(config.output.join(name),serde_json::to_vec_pretty(project).unwrap()).unwrap(); }
        json!({"backend":config.backend,"model":config.model,"elapsed_seconds":started.elapsed().as_secs_f64(),"failure":failure,
            "policy":"explicit bypass for isolated generated fixture; current application host and permission path",
            "exact_requested_edit":exact_requested_edit,"undo_steps":undo_steps,"undo_restored":undo_restored,"redo_restored":redo_restored,
            "unsaved":this.session.path().is_none(),"midi_notes":midi_notes,"encoded":measurements,
            "successful_calls":{"edit_notes":successful("edit_notes"),"mixer_graph":successful("mixer_graph"),"read_notes":successful("read_notes"),"edit_clip_curve":successful("edit_clip_curve"),"preview":successful("preview"),"render":successful("render"),"export_midi":successful("export_midi"),"verify_render":successful("verify_render")},
            "argument_or_tool_errors":calls.iter().filter(|e|e["ok"]==false).count(),"transcript":entries})
    });
    fs::write(
        config.output.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert!(report["failure"].is_null(), "{report}");
    for key in [
        "exact_requested_edit",
        "undo_restored",
        "redo_restored",
        "unsaved",
    ] {
        assert_eq!(report[key], true, "{key}: {report}");
    }
    assert_eq!(report["undo_steps"], 3, "{report}");
    assert_eq!(report["midi_notes"], 3, "{report}");
    for name in ["preview", "render", "export_midi", "verify_render"] {
        assert_eq!(report["successful_calls"][name], 1, "{name}: {report}");
    }
    assert_eq!(report["successful_calls"]["edit_notes"], 1, "{report}");
    for name in ["mixer_graph", "read_notes", "edit_clip_curve"] {
        assert!(
            report["successful_calls"][name].as_u64().unwrap() >= 2,
            "{report}"
        );
    }
    assert_eq!(report["encoded"]["bit_depth"], 24, "{report}");
    assert_eq!(report["encoded"]["frames"], 96000, "{report}");
    assert_eq!(report["encoded"]["full_scale_samples"], 0, "{report}");
    assert!(
        report["encoded"]["peak_db"]
            .as_f64()
            .is_some_and(|peak| peak > -100.0 && peak < 0.0),
        "{report}"
    );
}
