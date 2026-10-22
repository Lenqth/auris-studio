//! Bounded first-pass measurements using the same performer as playback.

use super::*;
use auris_core::{PerformanceContext, performed_note_slots};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const NOTE_LIMIT: usize = 2048;
const STAGE_LIMIT: usize = 32;

pub(super) fn measure(
    clip: &MidiClip,
    stack: &[NoteTransform],
    voice: Option<&str>,
    project: &Project,
) -> Value {
    let mut notes: Vec<_> = clip
        .playable_notes()
        .filter(|note| voice.is_none_or(|name| note.drum_voice == name))
        .take(NOTE_LIMIT + 1)
        .collect();
    let mut complete = notes.len() <= NOTE_LIMIT && stack.len() <= STAGE_LIMIT;
    notes.truncate(NOTE_LIMIT);
    let sampled = notes.len();
    let context = PerformanceContext {
        bpm: project.tempo_map.bpm_at(clip.start),
        pass: 0,
        start: clip.start,
        length: clip.length,
        signatures: &project.signatures,
    };
    let mut stages = Vec::new();
    for stage in stack.iter().take(STAGE_LIMIT) {
        // Ghost generation scans the time grid, so a note-count cap alone cannot bound it.
        if matches!(
            stage,
            NoteTransform::Ghost { .. } | NoteTransform::Brush { .. }
        ) && clip.length > Ticks::QUARTER * 64
        {
            complete = false;
            stages.push(json!({"stage":family(stage),"measured":false,"warnings":["This generative stage spans more than 64 quarter notes. Measurement stops here to bound time-grid work; inspect a shorter clip to measure its effect."]}));
            break;
        }
        if matches!(stage, NoteTransform::ForDrumVoice { .. }) {
            complete = false;
            stages.push(json!({"stage":"for_drum_voice","warnings":["Select drum_voice to measure this writer's stages; the whole-clip measurement stops before nested scopes."]}));
            break;
        }
        let mut attacks = BTreeMap::new();
        for note in &notes {
            *attacks.entry(note.start.raw()).or_insert(0_usize) += 1;
        }
        let chord_groups = attacks.values().filter(|count| **count > 1).count();
        let slots = performed_note_slots(notes.clone(), std::slice::from_ref(stage), context);
        let paired = || {
            notes
                .iter()
                .zip(&slots)
                .filter_map(|(before, after)| after.as_ref().map(|after| (before, after)))
        };
        let onsets = paired().filter(|(a, b)| a.start != b.start).count();
        let durations = paired().filter(|(a, b)| a.length != b.length).count();
        let velocities = paired().filter(|(a, b)| a.velocity != b.velocity).count();
        let pitches = paired().filter(|(a, b)| a.pitch != b.pitch).count();
        let removed = slots
            .iter()
            .take(notes.len())
            .filter(|slot| slot.is_none())
            .count();
        let added = slots.len().saturating_sub(notes.len());
        let unchanged = onsets + durations + velocities + pitches + removed + added == 0;
        let strum = matches!(
            stage,
            NoteTransform::Strum { .. } | NoteTransform::Stroke { .. }
        );
        let mut warnings = Vec::new();
        if strum && chord_groups == 0 {
            warnings.push("Strum spreads simultaneous pitches; no simultaneous chord groups were found in the inspected notes. It cannot turn an arpeggio into chord strokes while preserving the score.");
        }
        if strum && unchanged && complete {
            warnings.push("This strum stage changes zero performed note events. Do not claim an effective stroke from the stored setting.");
        }
        stages.push(json!({"stage":family(stage),"measured":true,
            "input_notes":notes.len(),"eligible_chord_groups":chord_groups,
            "changed_onsets":onsets,"changed_durations":durations,"changed_velocities":velocities,
            "changed_pitches":pitches,"added_notes":added,"removed_notes":removed,
            "note_events_unchanged":unchanged,"warnings":warnings}));
        notes = slots.into_iter().flatten().take(NOTE_LIMIT + 1).collect();
        complete &= notes.len() <= NOTE_LIMIT;
        notes.truncate(NOTE_LIMIT);
    }
    json!({"representation":"First loop pass, selected scope, stages in order using the playback performer. Note events only: pitch curves, timbre and volume contours are not measured; unchanged note events do not imply inaudibility of those controls.",
        "sampled_authored_notes":sampled,"note_limit":NOTE_LIMIT,"stage_limit":STAGE_LIMIT,
        "complete":complete,"truncated":!complete,"stages":stages})
}
