//! Bundled Basic Pitch ONNX inference and polyphonic piano-note decoding on the CPU.
//!
//! The model and overlapping-window convention are from Spotify's Apache-2.0 Basic Pitch.
//! See `NOTICE-BASIC-PITCH.md` for the pinned source, license and decoding differences.

use crate::{AnalysisControl, AnalysisError, audio::TranscribedNote};
use auris_core::AudioBuffer;
use ort::{ep::CPU, session::Session, value::Tensor};
use serde::{Deserialize, Serialize};

/// Input rate required after the session's band-limited resampling.
pub const SAMPLE_RATE: f64 = 22_050.0;
const WINDOW: usize = 43_844;
const HOP: usize = WINDOW - 30 * 256;
const FRAMES: usize = 172;
const KEYS: usize = 88;
const TRIM: usize = 15;
const STRIDE: usize = FRAMES - 2 * TRIM;
/// SHA-256 of the bundled, unmodified Spotify ICASSP 2022 ONNX model.
pub const MODEL_SHA256: &str = "2c3c1d144bfa61ad236e92e169c13535c880469a12a047d4e73451f2c059a0ec";
const MODEL: &[u8] = include_bytes!("basic-pitch.onnx");

/// A polyphonic note draft, independent of document tempo and playback sound.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PianoTranscription {
    /// Exact bundled model hash for provenance.
    pub model_sha256: String,
    /// Source duration, excluding inference padding.
    pub seconds: f64,
    /// Potentially overlapping note estimates, with MIDI pitches 21..108.
    pub notes: Vec<TranscribedNote>,
}

impl PianoTranscription {
    /// Validates a nonempty bounded draft before any document change.
    pub fn validate(&self) -> Result<(), AnalysisError> {
        if self.model_sha256 != MODEL_SHA256
            || !self.seconds.is_finite()
            || self.seconds <= 0.0
            || self.seconds > 600.0
            || self.notes.is_empty()
            || self.notes.len() > 200_000
            || self.notes.iter().any(|n| {
                !(21..=108).contains(&n.pitch)
                    || !n.start.is_finite()
                    || !n.end.is_finite()
                    || n.start < 0.0
                    || n.end <= n.start
                    || n.end > self.seconds
                    || !n.strength.is_finite()
                    || !(0.0..=1.0).contains(&n.strength)
            })
        {
            return Err(AnalysisError::Invalid("no usable Basic Pitch note draft"));
        }
        Ok(())
    }
}

fn model_error(error: impl ToString) -> AnalysisError {
    AnalysisError::Model(format!("Basic Pitch: {}", error.to_string()))
}

/// Transcribes up to ten minutes/eight channels without downloading models or editing a song.
///
/// Run on a cancellable worker. Channels are averaged as in the reference implementation;
/// anti-phase stereo can cancel. This estimates pitched notes, not instrument separation or
/// an engraved/playability-constrained arrangement. Piano recordings work best.
pub fn transcribe(
    audio: &AudioBuffer,
    control: &AnalysisControl,
) -> Result<PianoTranscription, AnalysisError> {
    control.check(0.0)?;
    let seconds = audio.duration_seconds();
    if audio.sample_rate() != SAMPLE_RATE
        || audio.channel_count() == 0
        || audio.channel_count() > 8
        || !seconds.is_finite()
        || !(0.0..=600.0).contains(&seconds)
        || audio.frame_count() == 0
    {
        return Err(AnalysisError::Invalid(
            "Basic Pitch requires 22050 Hz, 1..8 channels and at most ten minutes",
        ));
    }
    let mut session = Session::builder()
        .map_err(model_error)?
        .with_intra_threads(2)
        .map_err(model_error)?
        .with_execution_providers([CPU::default().build().error_on_failure()])
        .map_err(model_error)?
        .commit_from_memory(MODEL)
        .map_err(model_error)?;
    let count = (audio.frame_count() + TRIM * 256).div_ceil(HOP);
    let expected_frames = (audio.frame_count() as f64 / HOP as f64 * STRIDE as f64) as usize;
    let mut frames = Vec::with_capacity(expected_frames * KEYS);
    let mut onsets = Vec::with_capacity(expected_frames * KEYS);
    for window in 0..count {
        control.check(window as f32 / count as f32 * 0.85)?;
        let mut waveform = vec![0.0_f32; WINDOW];
        for (i, sample) in waveform.iter_mut().enumerate() {
            let source = (window * HOP + i).checked_sub(TRIM * 256);
            if let Some(source) = source.filter(|&s| s < audio.frame_count()) {
                for channel in audio.iter_channels() {
                    let value = channel[source];
                    if !value.is_finite() {
                        return Err(AnalysisError::Invalid("audio contains non-finite samples"));
                    }
                    *sample += value / audio.channel_count() as f32;
                }
            }
        }
        let input = Tensor::from_array(([1, WINDOW, 1], waveform)).map_err(model_error)?;
        let outputs = session
            .run(ort::inputs!["serving_default_input_2:0" => input])
            .map_err(model_error)?;
        for (name, target) in [
            ("StatefulPartitionedCall:1", &mut frames),
            ("StatefulPartitionedCall:2", &mut onsets),
        ] {
            let output = outputs
                .get(name)
                .ok_or_else(|| model_error("missing activation output"))?;
            let (shape, scores) = output.try_extract_tensor::<f32>().map_err(model_error)?;
            if **shape != [1, FRAMES as i64, KEYS as i64]
                || scores
                    .iter()
                    .any(|s| !s.is_finite() || !(0.0..=1.0).contains(s))
            {
                return Err(model_error("invalid activation tensor"));
            }
            let remaining = expected_frames.saturating_sub(target.len() / KEYS);
            let take = STRIDE.min(remaining);
            target.extend_from_slice(&scores[TRIM * KEYS..(TRIM + take) * KEYS]);
        }
    }
    let notes = decode(&frames, &onsets, seconds, control)?;
    control.check(1.0)?;
    Ok(PianoTranscription {
        model_sha256: MODEL_SHA256.into(),
        seconds,
        notes,
    })
}

// Assign each retained prediction to its window's exact source position, avoiding long-file
// drift from treating the 142 retained frames as 142 uncorrected 256-sample hops.
fn frame_time(frame: usize) -> f64 {
    let window = frame / STRIDE;
    let within = frame % STRIDE;
    (window * HOP) as f64 / SAMPLE_RATE + within as f64 * HOP as f64 / STRIDE as f64 / SAMPLE_RATE
}

fn decode(
    frames: &[f32],
    onsets: &[f32],
    seconds: f64,
    control: &AnalysisControl,
) -> Result<Vec<TranscribedNote>, AnalysisError> {
    let count = frames.len() / KEYS;
    let mut notes = Vec::new();
    for key in 0..KEYS {
        control.check(0.85 + key as f32 / KEYS as f32 * 0.15)?;
        let mut start = None;
        let mut last_active = 0;
        let mut level = 0.0;
        for frame in 0..=count {
            let energy = if frame < count {
                frames[frame * KEYS + key]
            } else {
                0.0
            };
            let onset = frame > 0
                && frame + 1 < count
                && onsets[frame * KEYS + key] >= 0.5
                && onsets[frame * KEYS + key] > onsets[(frame - 1) * KEYS + key]
                && onsets[frame * KEYS + key] >= onsets[(frame + 1) * KEYS + key];
            let finish = start.is_some()
                && (onset || frame == count || frame.saturating_sub(last_active) > 11);
            if finish {
                let first = start.take().unwrap_or(frame);
                let end = if onset { frame } else { last_active + 1 };
                if end.saturating_sub(first) >= 11 {
                    let from = frame_time(first).min(seconds);
                    let to = frame_time(end).min(seconds);
                    if to > from {
                        notes.push(TranscribedNote {
                            pitch: key as u8 + 21,
                            start: from,
                            end: to,
                            strength: (level / (end - first) as f32).clamp(0.1, 1.0),
                        });
                        if notes.len() > 200_000 {
                            return Err(AnalysisError::Invalid("too many transcribed notes"));
                        }
                    }
                }
                level = 0.0;
            }
            // Frame activity recovers soft notes whose learned onset is below threshold.
            if frame < count && (energy >= 0.3 || onset) {
                if start.is_none() {
                    start = Some(frame);
                    level = 0.0;
                }
                last_active = frame;
            }
            if start.is_some() {
                level += energy;
            }
        }
    }
    notes.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn bundled_model_matches_the_pinned_upstream_file() {
        assert_eq!(hex::encode(Sha256::digest(MODEL)), MODEL_SHA256);
    }

    #[test]
    fn decoder_keeps_polyphony_repeated_attacks_and_notes_across_window_boundaries() {
        let mut frames = vec![0.0; 400 * KEYS];
        let mut onsets = frames.clone();
        for key in [39, 43, 46] {
            for frame in 30..300 {
                frames[frame * KEYS + key] = 0.8;
            }
            onsets[30 * KEYS + key] = 0.9;
        }
        onsets[160 * KEYS + 39] = 0.9;
        let notes = decode(&frames, &onsets, 5.0, &Default::default()).unwrap();
        assert_eq!(notes.len(), 4);
        let c: Vec<_> = notes.iter().filter(|n| n.pitch == 60).collect();
        assert_eq!(c.len(), 2);
        assert!((c[0].end - c[1].start).abs() < 1e-9);
        assert!((notes.iter().find(|n| n.pitch == 64).unwrap().end - frame_time(300)).abs() < 1e-9);
        assert!((frame_time(STRIDE * 200) - 200.0 * HOP as f64 / SAMPLE_RATE).abs() < 1e-9);
    }

    #[test]
    fn actual_model_transcribes_a_chord_and_reports_silence_without_notes() {
        let mut audio = AudioBuffer::new(1, (SAMPLE_RATE * 2.5) as usize, SAMPLE_RATE);
        for (i, sample) in audio.channel_mut(0).iter_mut().enumerate() {
            let t = i as f64 / SAMPLE_RATE;
            if (0.25..1.8).contains(&t) {
                *sample = [261.6256, 329.6276, 391.9954]
                    .iter()
                    .map(|hz| {
                        (std::f64::consts::TAU * hz * t).sin() * (-(t - 0.25) * 0.7).exp() * 0.12
                    })
                    .sum::<f64>() as f32;
            }
        }
        let report = transcribe(&audio, &Default::default()).unwrap();
        for pitch in [60, 64, 67] {
            assert!(
                report
                    .notes
                    .iter()
                    .any(|n| n.pitch == pitch && n.start < 0.5 && n.end > 1.0),
                "missing {pitch}: {:?}",
                report.notes
            );
        }
        audio.channel_mut(0).fill(0.0);
        assert!(
            transcribe(&audio, &Default::default())
                .unwrap()
                .notes
                .is_empty()
        );
    }
}
