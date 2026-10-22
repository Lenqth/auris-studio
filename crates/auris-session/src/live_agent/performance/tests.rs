use super::*;
use serde_json::{Value, json};

fn fixture() -> (Session, ClipId) {
    let mut session = Session::new(crate::SessionOptions::headless().with_balance(false)).unwrap();
    let track = session.add_default_instrument_track("Lead").unwrap();
    session
        .agent_edit_harmony(
            1,
            4,
            Some("C major"),
            Some("| I | IV | V | I |"),
            false,
            None,
        )
        .unwrap();
    let clip = session
        .generate_clip(
            track,
            Ticks::ZERO,
            Ticks::QUARTER * 16,
            ClipRecipe::new(ClipPreset::Lead, 42),
        )
        .unwrap();
    session
        .replace_notes(
            clip,
            vec![
                Note::new(60, Ticks::ZERO, Ticks::QUARTER),
                Note::new(64, Ticks(240), Ticks::QUARTER),
            ],
        )
        .unwrap();
    (session, clip)
}

fn edit(
    session: &mut Session,
    clip: ClipId,
    settings: Value,
    voice: Option<&str>,
) -> Result<Value, String> {
    let command = serde_json::from_value(
        json!({"action":"set_performance","clip":clip.0,"drum_voice":voice,"settings":settings}),
    )
    .map_err(|e| e.to_string())?;
    let text = session.agent_command(command)?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[test]
fn edits_change_the_performance_preserve_the_complete_score_and_undo_separately() {
    let (mut session, clip) = fixture();
    session.set_curve_point(clip, auris_core::project::ClipCurve::Bend, Ticks(200), 0.7);
    let contour = VolumeContour::new(vec![auris_core::project::CurvePoint {
        at: Ticks(2300),
        value: 0.22,
    }]);
    session
        .set_clip_transforms(
            clip,
            vec![
                NoteTransform::Pitch {
                    settings: PitchPerformance {
                        vibrato: 0.4,
                        volume_contour: contour.clone(),
                        ..Default::default()
                    },
                },
                NoteTransform::Expression {
                    settings: Expression {
                        group: 99,
                        seed: 777,
                        ..Default::default()
                    },
                },
            ],
        )
        .unwrap();
    let before = session.project().clone();
    let patch = json!({"gate":0.5,"transpose":12,"lean_ticks":24,"expression":{"velocity":0.4},"pitch":{"scoop":1.5}});
    let result = edit(&mut session, clip, patch.clone(), None).unwrap();
    assert_eq!(result["settings"]["expression"]["group"], 99);
    assert_eq!(result["settings"]["expression"]["seed"], 777);
    let actual = session.project().midi_clip(clip).unwrap().1;
    let mut expected = before.midi_clip(clip).unwrap().1.clone();
    expected.transforms = actual.transforms.clone();
    assert_eq!(
        actual, &expected,
        "notes, recipe, source curves, placement and loop state are preserved"
    );
    let held_pitch = actual
        .transforms
        .iter()
        .find_map(|stage| match stage {
            NoteTransform::Pitch { settings } => Some(settings),
            _ => None,
        })
        .unwrap();
    assert_eq!(held_pitch.volume_contour, contour);
    assert_eq!(held_pitch.vibrato, 0.4);
    let performed = auris_core::performed_notes(
        actual.playable_notes().collect(),
        &actual.transforms,
        auris_core::PerformanceContext {
            bpm: 120.0,
            pass: 0,
            start: actual.start,
            length: actual.length,
            signatures: &session.project().signatures,
        },
    );
    assert_eq!(performed[0].pitch, 72);
    assert_eq!(performed[0].length, Ticks(480));
    assert_eq!(performed[0].start, Ticks(24));
    let after_first = session.project().clone();
    edit(&mut session, clip, patch, None).unwrap();
    assert_eq!(
        session.project(),
        &after_first,
        "an identical retry is a no-op"
    );
    edit(&mut session, clip, json!({"gate":0.25}), None).unwrap();
    session.undo();
    assert_eq!(
        session.project(),
        &after_first,
        "successive tool calls are separate Undo steps"
    );
    session.undo();
    assert_eq!(session.project(), &before);
    assert!(session.path().is_none());
}

#[test]
fn partial_edits_upgrade_legacy_stages_without_losing_their_controls_or_order() {
    let (mut session, clip) = fixture();
    let before = vec![
        NoteTransform::Humanize {
            amount: 0.2,
            seed: 9,
        },
        NoteTransform::Transpose { semitones: 7 },
        NoteTransform::Stroke {
            spread_ms: 23.0,
            direction: StrokeDirection::HighToLow,
        },
        NoteTransform::Brush { amount: 0.3 },
        NoteTransform::Gate { amount: 0.8 },
    ];
    session.set_clip_transforms(clip, before.clone()).unwrap();
    let result = edit(
        &mut session,
        clip,
        json!({"expression":{"timing":0.4},"strum":{"up_velocity":0.6},"ghost":{"length_ms":30.0}}),
        None,
    )
    .unwrap();
    assert_eq!(result["settings"]["expression"]["velocity"], json!(0.2f32));
    assert_eq!(result["settings"]["expression"]["seed"], 9);
    assert_eq!(result["settings"]["strum"]["spread_ms"], 23.0);
    assert_eq!(result["settings"]["strum"]["direction"], "high_to_low");
    assert_eq!(result["settings"]["strum"]["clock"], "attacks");
    assert_eq!(result["settings"]["ghost"]["density"], 1.0);
    assert_eq!(result["settings"]["ghost"]["pattern"], "sixteenths");
    assert_eq!(result["settings"]["ghost"]["preserve_rests"], false);
    assert!((result["settings"]["ghost"]["velocity"].as_f64().unwrap() - 0.09).abs() < 1e-6);
    let actual = session.clip_transforms(clip).unwrap();
    assert_eq!(actual[1], before[1]);
    assert_eq!(actual[4], before[4]);
    assert_eq!(
        actual.iter().filter_map(family).collect::<Vec<_>>(),
        before.iter().filter_map(family).collect::<Vec<_>>()
    );
}

#[test]
fn invalid_batches_and_unknown_fields_leave_document_and_history_untouched() {
    let (mut session, clip) = fixture();
    let before = session.project().clone();
    for settings in [
        json!({}),
        json!({"gate":0.5,"strum":{}}),
        json!({"reset":vec!["gate";13]}),
        json!({"gate":0.5,"reset":["expression"],"ghost":{"length_ms":500}}),
        json!({"swing_percent":49}),
        json!({"pitch":{"vibrato_hz":12}}),
        json!({"expression":{"typo":0.5}}),
        json!({"strum":{"direction":"sideways"}}),
        json!({"ghost":{"target_pitch":128}}),
        json!({"ghost":{"target_pitch":42,"clear_target_pitch":true}}),
        json!({"groove":{"reference_clip":u64::MAX}}),
        json!({"groove":{"timing":0.5}}),
    ] {
        assert!(
            edit(&mut session, clip, settings.clone(), None).is_err(),
            "{settings}"
        );
        assert_eq!(session.project(), &before);
    }
    let patch = PerformancePatch {
        gate: Some(f32::NAN),
        ..Default::default()
    };
    assert!(session.agent_set_performance(clip, None, patch).is_err());
    assert_eq!(session.project(), &before);
    session.undo();
    assert_ne!(
        session.project(),
        &before,
        "failed calls added no empty Undo step"
    );
}

#[test]
fn scoped_drum_edits_preserve_other_writers_global_stages_and_stored_notes() {
    let mut session = Session::new(crate::SessionOptions::headless().with_balance(false)).unwrap();
    let track = session.add_default_drum_track("Beat").unwrap();
    let clip = session
        .add_midi_clip(track, "Kit", Ticks::ZERO, Ticks::QUARTER * 4)
        .unwrap();
    let mut kick = Note::new(36, Ticks::ZERO, Ticks::QUARTER);
    kick.drum_voice = "kick".into();
    let mut hat = Note::new(42, Ticks::ZERO, Ticks::QUARTER);
    hat.drum_voice = "hat".into();
    session.replace_notes(clip, vec![kick, hat]).unwrap();
    let kick_stage = NoteTransform::ForDrumVoice {
        voice: "kick".into(),
        transforms: vec![NoteTransform::Gate { amount: 0.9 }],
    };
    let global = NoteTransform::Lean { ticks: 7 };
    session
        .set_clip_transforms(
            clip,
            vec![
                kick_stage.clone(),
                global.clone(),
                NoteTransform::ForDrumVoice {
                    voice: "hat".into(),
                    transforms: vec![NoteTransform::Humanize {
                        amount: 0.1,
                        seed: 33,
                    }],
                },
            ],
        )
        .unwrap();
    let before = session.project().clone();
    let result = edit(
        &mut session,
        clip,
        json!({"gate":0.5,"expression":{"timing":0.2}}),
        Some("hat"),
    )
    .unwrap();
    assert_eq!(result["settings"]["expression"]["seed"], 33);
    assert_eq!(result["settings"]["gate"], 0.5);
    assert_eq!(session.clip_transforms(clip).unwrap()[0], kick_stage);
    assert_eq!(session.clip_transforms(clip).unwrap()[1], global);
    assert_eq!(
        session.project().midi_clip(clip).unwrap().1.notes,
        before.midi_clip(clip).unwrap().1.notes
    );
    let after = session.project().clone();
    assert!(edit(&mut session, clip, json!({"gate":0.8}), Some("unknown")).is_err());
    assert!(
        edit(
            &mut session,
            clip,
            json!({"pitch":{"scoop":1.0}}),
            Some("hat")
        )
        .is_err()
    );
    assert_eq!(session.project(), &after);
    let result = edit(
        &mut session,
        clip,
        json!({"reset":["expression","gate"]}),
        Some("hat"),
    )
    .unwrap();
    assert_eq!(result["settings"]["gate"], 1.0);
    assert!(result["settings"]["expression"].is_null());
    assert_eq!(session.clip_transforms(clip).unwrap()[0], kick_stage);
    session.undo();
    assert_eq!(session.project(), &after);
}

#[test]
fn groove_capture_uses_the_performed_reference_and_partial_edits_keep_the_snapshot() {
    let (mut session, target) = fixture();
    let track = session.project().midi_clip(target).unwrap().0;
    let source = session
        .add_midi_clip(track, "Reference", Ticks::QUARTER * 16, Ticks::QUARTER * 4)
        .unwrap();
    session
        .add_note(source, Note::new(60, Ticks::ZERO, Ticks::QUARTER))
        .unwrap();
    session
        .set_clip_transforms(source, vec![NoteTransform::Lean { ticks: 60 }])
        .unwrap();
    edit(
        &mut session,
        target,
        json!({"groove":{"reference_clip":source.0,"timing":0.8}}),
        None,
    )
    .unwrap();
    let held = session
        .clip_transforms(target)
        .unwrap()
        .iter()
        .find_map(|s| match s {
            NoteTransform::Groove { template, .. } => Some(template.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(held.points[0].offset, Ticks(60));
    session
        .replace_notes(source, vec![Note::new(72, Ticks(200), Ticks::QUARTER)])
        .unwrap();
    let result = edit(
        &mut session,
        target,
        json!({"groove":{"velocity":0.2}}),
        None,
    )
    .unwrap();
    assert_eq!(result["settings"]["groove"]["timing"], json!(0.8f32));
    let actual = session
        .clip_transforms(target)
        .unwrap()
        .iter()
        .find_map(|s| match s {
            NoteTransform::Groove { template, .. } => Some(template),
            _ => None,
        })
        .unwrap();
    assert_eq!(actual, &held);
}

#[test]
fn performance_inspection_is_read_only_and_exposes_panel_units() {
    let (mut session, clip) = fixture();
    session.set_clip_transforms(clip, Vec::new()).unwrap();
    let before = session.project().clone();
    let result: Value = serde_json::from_str(
        &session
            .agent_command(Command::InspectPerformance {
                clip: clip.0,
                drum_voice: None,
            })
            .unwrap(),
    )
    .unwrap();
    assert_eq!(result["settings"]["swing_percent"], 50);
    assert_eq!(result["settings"]["gate"], 1.0);
    assert_eq!(result["units"]["times"], "milliseconds");
    assert_eq!(session.project(), &before);
}
