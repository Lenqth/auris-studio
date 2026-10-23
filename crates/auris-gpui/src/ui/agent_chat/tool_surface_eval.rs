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
    context_tokens: Option<u32>,
    thinking: Option<bool>,
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
#[ignore = "real arrangement inference; requires AURIS_ARRANGEMENT_HARNESS=1 and AURIS_TOOL_SURFACE_EVAL config"]
fn installed_agent_arranges_sections_with_performance_automation_and_sfx(
    cx: &mut gpui::TestAppContext,
) {
    assert!(
        auris_session::arrangement::enabled(),
        "explicit harness opt-in is required"
    );
    let path = std::env::var_os("AURIS_TOOL_SURFACE_EVAL").expect("explicit evaluation config");
    let config: Evaluation = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(matches!(
        config.backend.as_str(),
        "codex" | "claude" | "rig"
    ));
    assert!(config.output.is_absolute() && !config.output.exists());
    fs::create_dir_all(&config.output).unwrap();
    let sfx = config.output.join("source-effect.wav");
    let mut writer = hound::WavWriter::create(
        &sfx,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for frame in 0..4800 {
        writer
            .write_sample(
                ((frame as f32 * 0.07).sin() * 12000.0 * (1.0 - frame as f32 / 4800.0)) as i16,
            )
            .unwrap();
    }
    writer.finalize().unwrap();
    let (app, cx) = crate::harness::open(cx);
    let (keys,pad,chorus,before)=app.update(cx,|this,_| {
        let keys=this.session.add_instrument_track("Keys","auris.physical.piano").unwrap();
        let mut chorus=ClipId(0);
        for (index,label) in ["Verse 1","Chorus 1","Verse 2","Chorus 2"].iter().enumerate() {
            let start=Ticks::QUARTER*(index as i64*32);
            this.session.set_section(start,Some((*label).into()));
            let clip=this.session.add_midi_clip(keys,label.to_string(),start,Ticks::QUARTER*32).unwrap();
            for bar in 0..8 {
                for pitch in [60,64,67] {
                    this.session.add_note(clip,Note::new(pitch,Ticks::QUARTER*(bar*4),Ticks::QUARTER*2)).unwrap();
                }
            }
            if index==3 {chorus=clip;}
        }
        let pad=this.session.add_instrument_track("Pad","auris.synth.chiptune").unwrap();
        let clip=this.session.add_midi_clip(pad,"Held pad",Ticks::ZERO,Ticks::QUARTER*128).unwrap();
        this.session.add_note(clip,Note::new(48,Ticks::ZERO,Ticks::QUARTER*128)).unwrap();
        this.session.set_param(auris_session::ParamTarget::TrackGain(pad),-18.0);
        this.session.forget_history();
        let before=this.session.project().clone();
        this.settings.agent=auris_session::AgentPreferences {backend:config.backend.clone(),model:config.model.clone(),provider:config.provider.clone().unwrap_or_default(),url:config.url.clone().unwrap_or_default(),api_key_env:config.api_key_env.clone().unwrap_or_default(),context_tokens:config.context_tokens,thinking:config.thinking,policy:Policy {mode:Mode::Bypass,..Default::default()},..Default::default()};
        this.agent_chat.load_preferences(&this.settings.agent.clone());
        this.panels.show(crate::dock::Panel::Agent);
        this.agent_chat.input=TextField::new(format!(
            "編曲ハーネスの実動作確認です。この32小節の未保存曲を編曲してください。最初にinspect_projectとarrangement/inspectで確認し、25〜32小節の第2サビのクリップID {} だけを、search_instrumentsで見つけた内蔵アコースティックギターへ移してください。新しいクリップのストラム幅は25ms。他の3区間はピアノのまま、元の音符・歌詞・コード・曲構成・テンポは保持します。PadのトラックID {} は25〜32小節（絶対tick 92160〜122880）の範囲内だけトラックゲインを変化させ、内点tick 93120で-24dB、121920で-12dB、範囲の両端と外側は元の値を保持してください。第2サビ頭tick 92160へ、ローカル音声 {} のピークを合わせ、ゲイン-12dB、フェードイン2ms・フェードアウト5msで挿入してください。さらに、そのサビ頭へ向けて長さ1920tick、ゲイン-18dBの内蔵ライザーを置いてください。これら4変更を目的付きの一つのarrangement案にまとめ、現在のrevisionでvalidateし、返ったplan_idと同じrevisionでapplyしてください。既存ツールで別々に変更せず、この一括適用を確認します。適用後はarrangement/inspect、read_notes、inspect_performance、automation/readで独立して読み戻し、inspect_audioで25小節目の1小節を適用前後で測定してください。音声ファイルの書出し、元の曲の保存、外部検索は不要です。この試験ではlistenを呼ばず、inspect_audioによる測定だけを行ってください。測定と聴感評価は区別し、実際に確認できた内容だけを報告してください。",
            chorus.0,pad.0,serde_json::to_string(&sfx).unwrap()
        ));
        this.agent_send();
        (keys,pad,chorus,before)
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
                "{} arrangement {:.1}s, {} entries",
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
            break Some("Unexpected approval in isolated bypass fixture".to_owned());
        }
        if finished {
            break None;
        }
        if started.elapsed() > Duration::from_secs(config.timeout_seconds) {
            break Some("Arrangement agent timed out".to_owned());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let report=app.update(cx,|this,cx| {
        let entries=transcript(&this.agent_chat.entries);
        this.agent_stop(cx);
        let after=this.session.project().clone();
        let (new_track,midi)=after.midi_clip(chorus).unwrap();
        let revoiced=new_track!=keys && after.track(new_track).and_then(|t|t.kind.as_instrument()).is_some_and(|i|i.instrument_id=="auris.physical.guitar");
        let score_preserved=midi.notes==before.midi_clip(chorus).unwrap().1.notes && midi.controllers==before.midi_clip(chorus).unwrap().1.controllers;
        let strummed=midi.transforms.iter().any(|stage|matches!(stage,NoteTransform::Strum{settings} if settings.spread_ms==25.0));
        let mut expected_source=before.track(keys).unwrap().clone();
        expected_source.kind.note_clips_mut().unwrap().retain(|c|c.id!=chorus);
        let source_preserved=after.track(keys)==Some(&expected_source);
        let maps_preserved=after.tempo_map==before.tempo_map && after.signatures==before.signatures && after.sections==before.sections && after.harmony==before.harmony;
        let gain_target=auris_session::ParamTarget::TrackGain(pad);
        let automated=after.automation.lane(gain_target).is_some_and(|l|l.value_at(Ticks(93120)) == -24.0 && l.value_at(Ticks(121920)) == -12.0 && l.value_at(Ticks(90000)) == -18.0 && l.value_at(Ticks(123000)) == -18.0);
        let audio_sfx=after.audio_sources.len()==before.audio_sources.len()+1 && after.audio_sources.values().filter(|s|!before.audio_sources.contains_key(&s.id)).all(|s| {
            let copied=s.path.resolve(this.session.project_folder()).and_then(|path|fs::read(path).ok());
            copied==fs::read(&sfx).ok() && after.tracks.iter().filter_map(|t|t.kind.as_audio()).flat_map(|t|&t.clips).any(|c|c.source==s.id && c.gain_db == -12.0 && c.fade_in_frames==96 && c.fade_out_frames==240)
        });
        let riser=after.tracks.iter().any(|track|track.kind.as_instrument().is_some_and(|i|i.instrument_id=="auris.physical.violin" && i.clips.iter().any(|c|c.start==Ticks(90240) && c.end()==Ticks(92160))));
        let pad_preserved=after.track(pad)==before.track(pad);
        let new_tracks=after.tracks.len()==before.tracks.len()+3;
        let mut undo_steps=0;
        while this.session.project()!=&before && undo_steps<4 {if this.session.undo().is_none(){break;}undo_steps+=1;}
        let undo_restored=this.session.project()==&before;
        for _ in 0..undo_steps {this.session.redo();}
        let redo_restored=this.session.project()==&after;
        let calls:Vec<_>=entries.iter().filter(|e|e["kind"]=="tool").collect();
        let count=|name:&str|calls.iter().filter(|e|e["name"]==name && e["ok"]==true).count();
        let arguments=|entry:&Value| entry["args"].as_str().and_then(|text|serde_json::from_str::<Value>(text).ok()).unwrap_or_else(||entry["args"].clone());
        let applied=calls.iter().position(|entry|entry["name"]=="arrangement" && entry["ok"]==true && arguments(entry)["operation"]["kind"]=="apply");
        let independent_readback=applied.is_some_and(|index|["read_notes","inspect_performance","automation"].iter().all(|name|calls[index+1..].iter().any(|e|e["name"]==*name && e["ok"]==true)));
        let paired_audio=applied.is_some_and(|index| {
            let inspected=|entries:&[&Value]|entries.iter().any(|entry|entry["name"]=="inspect_audio" && entry["ok"]==true && arguments(entry)["start_bar"]==25 && arguments(entry)["bars"]==1);
            inspected(&calls[..index]) && inspected(&calls[index+1..])
        });
        for (label,project) in [("project-before.json",&before),("project-after.json",&after)] {fs::write(config.output.join(label),serde_json::to_vec_pretty(project).unwrap()).unwrap();}
        json!({"backend":config.backend,"model":config.model,"elapsed_seconds":started.elapsed().as_secs_f64(),"failure":failure,"revoiced":revoiced,"score_preserved":score_preserved,"source_preserved":source_preserved,"maps_preserved":maps_preserved,"strummed":strummed,"automated":automated,"audio_sfx":audio_sfx,"riser":riser,"pad_preserved":pad_preserved,"new_tracks":new_tracks,"independent_readback":independent_readback,"paired_audio":paired_audio,"undo_steps":undo_steps,"undo_restored":undo_restored,"redo_restored":redo_restored,"unsaved":this.session.path().is_none(),"successful_calls":{"arrangement":count("arrangement"),"inspect_audio":count("inspect_audio"),"read_notes":count("read_notes"),"inspect_performance":count("inspect_performance"),"automation":count("automation")},"tool_errors":calls.iter().filter(|e|e["ok"]==false).count(),"transcript":entries})
    });
    fs::write(
        config.output.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    let evidence = config.output.join("report.json");
    assert!(
        report["failure"].is_null(),
        "{}; evidence: {}",
        report["failure"],
        evidence.display()
    );
    for key in [
        "revoiced",
        "score_preserved",
        "source_preserved",
        "maps_preserved",
        "strummed",
        "automated",
        "audio_sfx",
        "riser",
        "pad_preserved",
        "new_tracks",
        "independent_readback",
        "paired_audio",
        "undo_restored",
        "redo_restored",
        "unsaved",
    ] {
        assert_eq!(report[key], true, "{key}; evidence: {}", evidence.display());
    }
    assert_eq!(report["undo_steps"], 1, "evidence: {}", evidence.display());
    assert!(
        report["successful_calls"]["arrangement"].as_u64().unwrap() >= 4,
        "evidence: {}",
        evidence.display()
    );
    assert!(
        report["successful_calls"]["inspect_audio"]
            .as_u64()
            .unwrap()
            >= 2,
        "evidence: {}",
        evidence.display()
    );
    for name in ["read_notes", "inspect_performance", "automation"] {
        assert!(
            report["successful_calls"][name].as_u64().unwrap() >= 1,
            "{name}; evidence: {}",
            evidence.display()
        );
    }
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
            context_tokens:config.context_tokens,
            thinking:config.thinking,
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
        json!({"backend":config.backend,"model":config.model,"context_tokens":config.context_tokens,"thinking":config.thinking,"elapsed_seconds":started.elapsed().as_secs_f64(),"failure":failure,
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
    assert_eq!(report["undo_steps"], 1, "{report}");
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
