//! Confidence-gated attacks followed by bounded, same-pitch residual recovery.
//!
//! Calibrated on frozen composer/audio pairs; see docs/piano-transcription-evaluation.md.
//! This remains an Auris decoder, not a claim of Spotify postprocessor parity.

use super::{KEYS, frame_time};
use crate::{AnalysisControl, AnalysisError, audio::TranscribedNote};

const ONSET: f32 = 0.5;
const FRAME: f32 = 0.3;
const GAP: usize = 4;
const MIN_FRAMES: usize = 6;
const MIN_MEAN: f32 = 0.4;
const RECOVERY_FRAMES: usize = 8;
const RECOVERY_MEAN: f32 = 0.45;
const RECOVERY_PEAK: f32 = 0.5;

struct Event {
    start: usize,
    end: usize,
    level: f32,
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len().max(1) as f32
}

pub(super) fn decode(
    frames: &[f32],
    onsets: &[f32],
    seconds: f64,
    control: &AnalysisControl,
) -> Result<Vec<TranscribedNote>, AnalysisError> {
    if frames.len() != onsets.len() || !frames.len().is_multiple_of(KEYS) {
        return Err(AnalysisError::Invalid(
            "invalid Basic Pitch activation dimensions",
        ));
    }
    let count = frames.len() / KEYS;
    let mut notes = Vec::new();
    for key in 0..KEYS {
        control.check(0.85 + key as f32 / KEYS as f32 * 0.15)?;
        let values: Vec<_> = frames.iter().skip(key).step_by(KEYS).copied().collect();
        let attack: Vec<_> = onsets.iter().skip(key).step_by(KEYS).copied().collect();
        let mut peaks: Vec<usize> = Vec::new();
        for at in 0..count {
            let before = at.checked_sub(1).map_or(0.0, |i| attack[i]);
            let after = attack.get(at + 1).copied().unwrap_or(0.0);
            if attack[at] < ONSET || attack[at] <= before || attack[at] < after {
                continue;
            }
            if let Some(&previous) = peaks.last()
                && at - previous < MIN_FRAMES
            {
                if attack[at] > attack[previous] {
                    peaks.pop();
                    peaks.push(at);
                }
            } else {
                peaks.push(at);
            }
        }
        let mut events = Vec::new();
        for (index, &start) in peaks.iter().enumerate() {
            let limit = peaks.get(index + 1).copied().unwrap_or(count);
            let mut end = start;
            for (at, &value) in values.iter().enumerate().take(limit).skip(start) {
                if value >= FRAME {
                    end = at + 1;
                }
                if at.saturating_sub(end) >= GAP {
                    break;
                }
            }
            if end - start >= MIN_FRAMES {
                let level = mean(&values[start..end]);
                if level >= MIN_MEAN {
                    events.push(Event { start, end, level });
                }
            }
        }
        let mut remaining = values.clone();
        for event in &events {
            // A slowly growing attack or fading release must not become a second note.
            remaining[event.start.saturating_sub(8)..(event.end + 4).min(count)].fill(0.0);
        }
        let active: Vec<_> = remaining
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| (v >= FRAME).then_some(i))
            .collect();
        let mut index = 0;
        while index < active.len() {
            let first = active[index];
            let mut last = first;
            index += 1;
            while index < active.len() && active[index] - last <= GAP + 1 {
                last = active[index];
                index += 1;
            }
            let end = last + 1;
            if end - first < RECOVERY_FRAMES {
                continue;
            }
            let span = &remaining[first..end];
            let peak = span.iter().copied().fold(0.0_f32, f32::max);
            if peak < RECOVERY_PEAK || mean(span) < RECOVERY_MEAN {
                continue;
            }
            let mut start = first;
            while start > first.saturating_sub(4) && remaining[start - 1] >= 0.1 {
                start -= 1;
            }
            events.push(Event {
                start,
                end,
                level: mean(&values[start..end]),
            });
        }
        for event in events {
            let start = frame_time(event.start).min(seconds);
            let end = frame_time(event.end).min(seconds);
            if end <= start {
                continue;
            }
            notes.push(TranscribedNote {
                pitch: key as u8 + 21,
                start,
                end,
                strength: event.level.clamp(0.1, 1.0),
            });
            if notes.len() > 200_000 {
                return Err(AnalysisError::Invalid("too many transcribed notes"));
            }
        }
    }
    notes.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    Ok(notes)
}
