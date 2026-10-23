use super::*;
use crate::live_agent::{AgentStep, Command};
use auris_core::Note;

#[test]
fn assigned_drums_keep_their_recipe_addresses_and_unassigned_drums_are_refused() {
    use auris_core::project::DrumRole;
    let (mut session, lead, _, _) = fixture();
    let drums = session
        .add_drum_track("Drums", crate::DEFAULT_INSTRUMENT)
        .unwrap();
    let request = || {
        serde_json::from_value(json!({"kind":"generate","parts":[
            {"track":drums.0,"name":"Kick","part":"kick","start_bar":1,"bars":4,"seed":3}
        ]}))
        .unwrap()
    };
    let before = session.project().clone();
    let command = make_command(&session, lead, request());
    assert!(session.agent_command(command).is_err());
    assert_eq!(session.project(), &before);
    session
        .set_drum_assignment(drums, DrumRole::Kick, Some(48))
        .unwrap();
    let report = execute(&mut session, lead, request());
    let id = ClipId(report["clips"][0]["clip"].as_u64().unwrap());
    let original = session.project.midi_clip(id).unwrap().1.clone();
    assert!(original.notes.iter().all(|note| note.pitch == 48));
    session
        .set_drum_assignment(drums, DrumRole::Kick, Some(49))
        .unwrap();
    execute(
        &mut session,
        lead,
        Operation::Regenerate {
            clips: vec![id.0],
            harmony: None,
        },
    );
    assert_eq!(session.project.midi_clip(id).unwrap().1, &original);
}

#[test]
fn looped_backing_cannot_be_overlapped_or_changed_outside_the_harmony_range() {
    let (mut session, lead, bass, chords) = fixture();
    let report = execute(&mut session, lead, generate(bass, chords));
    let id = ClipId(report["clips"][0]["clip"].as_u64().unwrap());
    session.project.midi_clip_mut(id).unwrap().loop_end = Ticks::QUARTER * 32;
    let before = session.project().clone();
    let command = make_command(&session, lead, changed_harmony(vec![id.0]));
    assert!(session.agent_command(command).is_err());
    let operation = serde_json::from_value(json!({"kind":"generate","parts":[
        {"track":bass.0,"name":"Overlapping loop","part":"bass","start_bar":5,"bars":4,"seed":1}
    ]}))
    .unwrap();
    let command = make_command(&session, lead, operation);
    assert!(session.agent_command(command).is_err());
    assert_eq!(session.project(), &before);
}

fn fixture() -> (Session, ClipId, TrackId, TrackId) {
    let mut session = Session::new(crate::SessionOptions::headless().with_balance(false)).unwrap();
    let lead = session.add_default_instrument_track("Lead").unwrap();
    let clip = session
        .add_midi_clip(lead, "Authored melody", Ticks::ZERO, Ticks::QUARTER * 16)
        .unwrap();
    session
        .add_note(clip, Note::new(72, Ticks(17), Ticks(997)))
        .unwrap();
    decorate(&mut session, clip);
    let bass = session.add_default_instrument_track("Bass").unwrap();
    let chords = session.add_default_instrument_track("Chords").unwrap();
    let chart = Chart::parse("| I | IV | V | I |").unwrap();
    session.stamp_progression(&chart, Ticks::ZERO, 4);
    session.forget_history();
    (session, clip, bass, chords)
}

fn decorate(session: &mut Session, id: ClipId) {
    let clip = session.project.midi_clip_mut(id).unwrap();
    clip.bend.push(auris_core::project::CurvePoint {
        at: Ticks(31),
        value: 0.25,
    });
    clip.controllers.insert(
        64,
        vec![auris_core::project::CurvePoint {
            at: Ticks(5),
            value: 1.0,
        }],
    );
    clip.transforms = auris_compose::clip_performance(ClipPreset::Chords, 71);
    clip.length_is_explicit = true;
}

fn generate(bass: TrackId, chords: TrackId) -> Operation {
    serde_json::from_value(json!({"kind":"generate","parts":[
        {"track":bass.0,"name":"Bass","part":"bass","start_bar":1,"bars":4,"seed":23,"settings":{"density":0.65}},
        {"track":chords.0,"name":"Bed","part":"pad","start_bar":1,"bars":4,"seed":42}
    ]})).unwrap()
}

fn make_command(session: &Session, lead: ClipId, operation: Operation) -> Command {
    Command::Accompaniment {
        expected_revision: session.agent_document_revision().unwrap(),
        protected_clips: vec![lead.0],
        operation,
    }
}

fn execute(session: &mut Session, lead: ClipId, operation: Operation) -> Value {
    let command = make_command(session, lead, operation);
    serde_json::from_str(&session.agent_command(command).unwrap()).unwrap()
}

fn changed_harmony(clips: Vec<u64>) -> Operation {
    Operation::Regenerate {
        clips,
        harmony: Some(HarmonyChange {
            start_bar: 1,
            bars: 4,
            key: Some("D major".into()),
            chords: "| Imaj7 | IVsus4 | viidim7 | V7 |".into(),
        }),
    }
}

#[test]
fn generated_backing_preserves_the_complete_melody_and_undo_redo_are_atomic() {
    let (mut session, lead, bass, chords) = fixture();
    let before = session.project().clone();
    let old_revision = session.agent_document_revision().unwrap();
    let report = execute(&mut session, lead, generate(bass, chords));
    assert_eq!(session.project.midi_clip(lead), before.midi_clip(lead));
    assert_eq!(session.project.harmony, before.harmony);
    assert_eq!(report["clips"].as_array().unwrap().len(), 2);
    for item in report["clips"].as_array().unwrap() {
        assert!(item["note_count"].as_u64().unwrap() > 0);
        assert!(item.get("notes").is_none());
        let clip = session
            .project
            .midi_clip(ClipId(item["clip"].as_u64().unwrap()))
            .unwrap()
            .1;
        let recipe = clip.recipe.as_ref().unwrap();
        assert_eq!(
            clip.notes,
            project_phrase(&before, clip.start, clip.length, recipe, None)
        );
        assert_eq!(recipe.text_digest, auris_core::notes_digest(&clip.notes));
    }
    let after = session.project().clone();
    assert!(
        session
            .agent_command(Command::Accompaniment {
                expected_revision: old_revision,
                protected_clips: vec![lead.0],
                operation: generate(bass, chords)
            })
            .is_err()
    );
    assert_eq!(session.project(), &after);
    session.undo();
    assert_eq!(session.project(), &before);
    assert!(!session.can_undo());
    session.redo();
    assert_eq!(session.project(), &after);
}

#[test]
fn harmony_and_only_selected_backing_change_together_and_retain_identity_and_performance() {
    let (mut session, lead, bass, chords) = fixture();
    let generated = execute(&mut session, lead, generate(bass, chords));
    let id = ClipId(generated["clips"][0]["clip"].as_u64().unwrap());
    decorate(&mut session, id);
    let before = session.project().clone();
    session.forget_history();
    let report = execute(&mut session, lead, changed_harmony(vec![id.0]));
    assert_eq!(report["harmony_changed"], true);
    assert_eq!(session.project.midi_clip(lead), before.midi_clip(lead));
    assert_eq!(session.project.track(chords), before.track(chords));
    let original = before.midi_clip(id).unwrap().1;
    let updated = session.project.midi_clip(id).unwrap().1;
    assert_ne!(original.notes, updated.notes);
    let mut expected = original.clone();
    expected.notes = updated.notes.clone();
    expected.recipe.as_mut().unwrap().text_digest = updated.recipe.as_ref().unwrap().text_digest;
    assert_eq!(
        &expected, updated,
        "Only notes and their recipe digest may change"
    );
    for (bar, expected) in ["Dmaj7", "Gsus4", "C#dim7", "A7"].iter().enumerate() {
        assert_eq!(
            session
                .project
                .harmony
                .chord_at(Ticks::QUARTER * (bar as i64 * 4))
                .unwrap()
                .name_in(MusicalKey::parse("D major").unwrap()),
            *expected
        );
    }
    assert_eq!(
        session.project.harmony.key_at(Ticks::QUARTER * 16),
        before.harmony.key_at(Ticks::QUARTER * 16)
    );
    let after = session.project().clone();
    session.undo();
    assert_eq!(session.project(), &before);
    assert!(!session.can_undo());
    session.redo();
    assert_eq!(session.project(), &after);
}

#[test]
fn an_invalid_later_part_and_empty_generation_leave_project_and_redo_untouched() {
    let (mut session, lead, bass, chords) = fixture();
    execute(&mut session, lead, generate(bass, chords));
    session.undo();
    let before = session.project().clone();
    for operation in [generate(bass, TrackId(u64::MAX)), generate(bass, bass)] {
        let command = make_command(&session, lead, operation);
        assert!(session.agent_command(command).is_err());
        assert_eq!(session.project(), &before);
        assert!(session.can_redo());
        assert!(!session.can_undo());
    }
    session
        .project
        .harmony
        .chords
        .clear_range(Ticks::ZERO, Ticks::QUARTER * 16);
    let before = session.project().clone();
    let command = make_command(&session, lead, generate(bass, chords));
    assert!(session.agent_command(command).is_err());
    assert_eq!(session.project(), &before);
}

#[test]
fn regeneration_refuses_hand_edits_frozen_clips_and_protected_targets_atomically() {
    let (mut session, lead, bass, chords) = fixture();
    let generated = execute(&mut session, lead, generate(bass, chords));
    let id = ClipId(generated["clips"][0]["clip"].as_u64().unwrap());
    let second = generated["clips"][1]["clip"].as_u64().unwrap();
    session
        .add_note(id, Note::new(55, Ticks(13), Ticks(179)))
        .unwrap();
    session.forget_history();
    let before = session.project().clone();
    for targets in [
        vec![second, id.0],
        vec![lead.0],
        vec![second, second],
        vec![second, u64::MAX],
    ] {
        let command = make_command(&session, lead, changed_harmony(targets));
        assert!(session.agent_command(command).is_err());
        assert_eq!(session.project(), &before);
        assert!(!session.can_undo());
    }
    session.freeze_clip(id).unwrap();
    let before = session.project().clone();
    let command = make_command(&session, lead, changed_harmony(vec![id.0]));
    assert!(session.agent_command(command).is_err());
    assert_eq!(session.project(), &before);
}

#[test]
fn cancelled_stale_wrong_owner_and_open_gesture_results_cannot_publish() {
    let (mut session, lead, bass, chords) = fixture();
    let cancelled = AtomicBool::new(false);
    let command = make_command(&session, lead, generate(bass, chords));
    let job = session
        .begin_agent_job(command, crate::Settings::default())
        .unwrap();
    let result = job.run(&cancelled).unwrap();
    let before = session.project().clone();
    cancelled.store(true, Ordering::Relaxed);
    assert!(session.continue_agent_job(result, &cancelled).is_err());
    assert_eq!(session.project(), &before);
    cancelled.store(false, Ordering::Relaxed);
    for change in 0..3 {
        let command = make_command(&session, lead, generate(bass, chords));
        let job = session
            .begin_agent_job(command, crate::Settings::default())
            .unwrap();
        let result = job.run(&cancelled).unwrap();
        match change {
            0 => {
                session
                    .add_note(lead, Note::new(74, Ticks(1000), Ticks(100)))
                    .unwrap();
            }
            1 => session.begin_transaction(Edit::MoveClip),
            _ => {
                let (mut other, _, _, _) = fixture();
                let before = other.project().clone();
                assert!(other.continue_agent_job(result, &cancelled).is_err());
                assert_eq!(other.project(), &before);
                continue;
            }
        }
        let before = session.project().clone();
        assert!(session.continue_agent_job(result, &cancelled).is_err());
        assert_eq!(session.project(), &before);
        session.cancel_transaction();
    }
    let command = make_command(&session, lead, generate(bass, chords));
    let job = session
        .begin_agent_job(command, crate::Settings::default())
        .unwrap();
    assert!(job.run(&AtomicBool::new(true)).is_err());
}

#[test]
fn section_boundaries_and_partial_harmony_targets_require_explicit_splits() {
    let (mut session, lead, bass, chords) = fixture();
    let generated = execute(&mut session, lead, generate(bass, chords));
    let id = generated["clips"][0]["clip"].as_u64().unwrap();
    let mut operation = changed_harmony(vec![id]);
    if let Operation::Regenerate {
        harmony: Some(change),
        ..
    } = &mut operation
    {
        change.bars = 1;
    }
    let before = session.project().clone();
    let command = make_command(&session, lead, operation);
    assert!(session.agent_command(command).is_err());
    assert_eq!(session.project(), &before);
    session.set_section(Ticks::QUARTER * 8, Some("Bridge".into()));
    let before = session.project().clone();
    let command = make_command(&session, lead, changed_harmony(vec![id]));
    assert!(session.agent_command(command).is_err());
    assert_eq!(session.project(), &before);
}

#[test]
fn unchanged_regeneration_is_a_noop_and_worker_matches_direct_execution() {
    let (mut session, lead, bass, chords) = fixture();
    let command = make_command(&session, lead, generate(bass, chords));
    assert!(command.needs_worker());
    let job = session
        .begin_agent_job(command, crate::Settings::default())
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let AgentStep::Done(report) = session
        .continue_agent_job(job.run(&cancelled).unwrap(), &cancelled)
        .unwrap()
    else {
        panic!("generation should finish")
    };
    let report: Value = serde_json::from_str(&report).unwrap();
    session.forget_history();
    let before = session.project().clone();
    let report = execute(
        &mut session,
        lead,
        Operation::Regenerate {
            clips: vec![report["clips"][0]["clip"].as_u64().unwrap()],
            harmony: None,
        },
    );
    assert_eq!(report["changed"], false);
    assert_eq!(session.project(), &before);
    assert!(!session.can_undo());
}
