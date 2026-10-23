//! Explicit score arrivals and instrument-specific ending advice, never a quality score.

use super::*;
use std::collections::HashMap;

pub(super) struct Landings {
    pub starts: HashMap<NoteAddress, i64>,
    pub replaced: HashSet<NoteAddress>,
    pub changes: Vec<Value>,
}

pub(super) fn guidance(project: &Project, track: &Track) -> Value {
    let Some(instrument) = track.kind.as_instrument() else {
        return Value::Null;
    };
    let family = diagnostics::family(instrument, false);
    let strategy = match family {
        "keys" | "guitar" | "bass" | "pitched_percussion" => {
            "Let the struck/plucked arrival decay while held; do not shorten it or fade its resonance merely to reduce note occupancy. Pedal and release can extend beyond note-off."
        }
        "winds" | "bowed_strings" => {
            "Hold the arrival, then ease breath/bow expression if supported. Budget the actual release before the next entry; a short written gap may still contain a long tail."
        }
        "synth" => {
            "Inspect the current attack, decay, sustain and release. Hold the arrival before note-off; sustained patches may benefit from a scoped taper while percussive patches should retain their decay."
        }
        _ => {
            "Patch decay and controllers are unknown. Measure a held/released note and inspect the instrument before choosing an ending; do not infer an envelope from its name."
        }
    };
    let configured: serde_json::Map<String, Value> = ["attack", "decay", "sustain", "release"]
        .into_iter()
        .filter_map(|key| {
            instrument
                .instrument_state
                .params
                .get(key)
                .map(|v| (key.into(), json!(v)))
        })
        .collect();
    json!({"family":family,"strategy":strategy,"saved_envelope_values":configured,
        "instrument_automation_present":project.automation.lanes().iter().any(|l| matches!(l.target, ParamTarget::Instrument { track: id, .. } if id == track.id)),
        "limitation":"Saved values omit defaults. Inspect current instrument parameters and their automation; release seconds describe a control, not guaranteed acoustic silence. Compare the entire ending and following entry, including effect tails."})
}

pub(super) struct LandingContext<'a> {
    pub project: &'a Project,
    pub clips: &'a [u64],
    pub rests: &'a [RestWindow],
    pub protections: &'a HashSet<NoteAddress>,
    pub omissions: &'a HashSet<NoteAddress>,
    pub attacks: &'a HashSet<(i64, u8)>,
    pub releases: &'a HashSet<(i64, u8)>,
}

pub(super) fn prepare(
    context: LandingContext<'_>,
    requests: &[PhraseLanding],
) -> Result<Landings, String> {
    let LandingContext {
        project,
        clips,
        rests,
        protections,
        omissions,
        attacks,
        releases,
    } = context;
    let mut result = Landings {
        starts: HashMap::new(),
        replaced: HashSet::new(),
        changes: Vec::new(),
    };
    let mut touched = HashSet::new();
    for request in requests {
        let address = &request.note;
        let (owner, clip) = project
            .midi_clip(ClipId(address.clip))
            .ok_or("Unknown landing clip")?;
        if !clips.contains(&address.clip)
            || clip.loop_end > clip.length
            || request.replace_notes.is_empty()
            || request.replace_notes.len() > 16
        {
            return Err("A landing requires a selected independent clip and 1..16 explicit approach indices; split loop passes into independent clips first".into());
        }
        let arrival = clip
            .notes
            .get(address.index)
            .ok_or("Unknown original arrival index")?;
        let old_start = (clip.start + arrival.start).raw();
        let old_end = (clip.start + arrival.end()).raw();
        if request.start_tick < clip.start.raw()
            || request.start_tick >= old_start
            || !rests.iter().any(|r| {
                r.start_tick > request.start_tick
                    && r.start_tick <= old_end
                    && r.end_tick >= old_start
            })
        {
            return Err("A landing must begin earlier inside its clip and lead directly into a requested rest".into());
        }
        let check = |index: usize| -> Result<(), String> {
            let note = clip
                .notes
                .get(index)
                .ok_or("Unknown original approach index")?;
            let key = NoteAddress {
                clip: address.clip,
                index,
            };
            let a = (clip.start + note.start).raw();
            let b = (clip.start + note.end()).raw();
            if protections.contains(&key)
                || omissions.contains(&key)
                || protected(note)
                || attacks.contains(&(b, note.pitch))
                || releases.contains(&(a, note.pitch))
            {
                return Err(
                    "Landing touches a protected, vocal, omitted or possibly tied note".into(),
                );
            }
            Ok(())
        };
        check(address.index)?;
        if !touched.insert(address.clone()) {
            return Err("Landing notes must be disjoint original indices".into());
        }
        let mut approach = Vec::new();
        for &index in &request.replace_notes {
            check(index)?;
            let key = NoteAddress {
                clip: address.clip,
                index,
            };
            if !touched.insert(key.clone()) {
                return Err("Landing notes must be disjoint original indices".into());
            }
            let note = &clip.notes[index];
            approach.push((
                (clip.start + note.start).raw(),
                (clip.start + note.end()).raw(),
                index,
            ));
            result.replaced.insert(key);
        }
        approach.sort_unstable();
        let mut cursor = request.start_tick;
        for &(a, b, _) in &approach {
            if a != cursor || b <= a || b > old_start {
                return Err("Replaced approach notes must exactly cover the interval before the original arrival, without gaps or overlaps".into());
            }
            cursor = b;
        }
        if cursor != old_start {
            return Err("Replaced approach must end at the original arrival".into());
        }
        // An earlier arrival must not acquire a new overlap with any other authored voice.
        let track = project.track(owner).ok_or("Missing landing track")?;
        for other in track
            .kind
            .note_clips()
            .ok_or("Missing landing clips")?
            .iter()
            .filter(|c| !c.muted)
        {
            for (offset, span) in loop_passes(other.length, other.loop_end) {
                for (index, note) in other
                    .notes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.start < span)
                {
                    let a = (other.start + offset + note.start).raw();
                    let b = (other.start + offset + note.end().min(span)).raw();
                    if a < old_start
                        && b > request.start_tick
                        && !(other.id == clip.id
                            && result.replaced.contains(&NoteAddress {
                                clip: address.clip,
                                index,
                            }))
                    {
                        return Err(
                            "Earlier arrival overlaps another note; select a monophonic approach"
                                .into(),
                        );
                    }
                }
            }
        }
        result.starts.insert(address.clone(), request.start_tick);
        result.changes.push(json!({"kind":"held_arrival","clip":address.clip,"index":address.index,
            "pitch":arrival.pitch,"velocity":arrival.velocity,"before_start_tick":old_start,
            "after_start_tick":request.start_tick,"replace_notes":request.replace_notes,
            "before":arrival,"replaced_notes":approach.iter().map(|(_,_,i)| json!({"index":i,"note":clip.notes[*i]})).collect::<Vec<_>>()}));
    }
    Ok(result)
}
