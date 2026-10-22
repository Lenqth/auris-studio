//! Revision-checked, capability-aware note pronunciation and pitch ornaments.

use crate::{Edit, Session, prelude::*};
use serde::{Deserialize, Serialize};

/// One exact correction to a singer note; operations apply in array order.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VocalEdit {
    /// Set one note's lyric and derive its phonemes; existing duration pins are cleared.
    Lyric {
        /// Zero-based storage index from read_notes.
        index: usize,
        /// Lyric text, including + for continuation, up to 16384 bytes. Empty clears the lyric.
        text: String,
    },
    /// Correct IPA tokens only when the selected backend supports manual phonemes.
    Phonemes {
        /// Zero-based storage index.
        index: usize,
        /// Explicit phoneme sequence; [] clears it. Existing duration pins are cleared.
        tokens: Vec<String>,
    },
    /// Pin a supported phoneme duration in seconds.
    Pin {
        /// Zero-based note storage index.
        index: usize,
        /// Zero-based phoneme position within that note.
        phoneme: usize,
        /// Finite duration, at least one model frame and at most ten seconds.
        seconds: f64,
    },
    /// Remove one duration pin, leaving other phonemes unchanged.
    Unpin {
        /// Zero-based note storage index.
        index: usize,
        /// Zero-based phoneme position.
        phoneme: usize,
    },
    /// Remove all duration pins on one note.
    ClearTiming {
        /// Zero-based note storage index.
        index: usize,
    },
    /// Add or replace a rise into the note.
    Scoop {
        /// Zero-based note storage index.
        index: usize,
        /// Pitch depth, 0..12 semitones.
        depth_semitones: f32,
        /// Duration, at least one model frame and at most two seconds.
        seconds: f64,
    },
    /// Add or replace a drop away from the note.
    Fall {
        /// Zero-based note storage index.
        index: usize,
        /// Pitch depth, 0..12 semitones.
        depth_semitones: f32,
        /// Duration, at least one model frame and at most two seconds.
        seconds: f64,
    },
    /// Add or replace a note's vibrato.
    Vibrato {
        /// Zero-based note storage index.
        index: usize,
        /// Pitch depth, 0..12 semitones.
        depth_semitones: f32,
        /// Cycles per second, 0.1..12.
        rate_hz: f32,
        /// Delay from note start, 0..10 seconds.
        delay_seconds: f64,
        /// Fade-in span, 0..10 seconds.
        fade_in_seconds: f64,
    },
    /// Remove only the selected ornament family.
    ClearOrnament {
        /// Zero-based note storage index.
        index: usize,
        /// Explicit ornament family to clear.
        ornament: Ornament,
    },
}

/// An explicit ornament-removal target.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Ornament {
    /// Rise into the note.
    Scoop,
    /// Drop at the note end.
    Fall,
    /// Periodic pitch sway.
    Vibrato,
    /// All three pitch ornaments.
    All,
}

impl Session {
    /// Apply bounded singer-note corrections atomically and reject unsupported backend controls.
    pub fn agent_edit_vocal_notes(
        &mut self,
        clip: u64,
        expected_revision: String,
        edits: Vec<VocalEdit>,
    ) -> Result<String, String> {
        if edits.is_empty() || edits.len() > 256 {
            return Err("Supply 1..256 vocal-note edits".into());
        }
        if expected_revision != self.agent_document_revision()? {
            return Err("Document changed; read_notes again before editing vocal notes".into());
        }
        let clip = ClipId(clip);
        let track = self.track_of_clip(clip).ok_or("Unknown clip ID")?;
        let singer = self
            .project()
            .track(track)
            .and_then(|t| t.kind.as_singer())
            .ok_or("Expected singer clip")?;
        let frame_hop = singer.frame_hop;
        let depth = |value: f32| {
            if (0.0..=auris_core::project::BEND_LIMIT).contains(&value) {
                Ok(value)
            } else {
                Err("Pitch depth must be finite within 0..12 semitones".to_string())
            }
        };
        let span = |value: f64, maximum: f64| {
            if (frame_hop..=maximum).contains(&value) {
                Ok(value)
            } else {
                Err(format!(
                    "Duration must be within {frame_hop}..{maximum} seconds"
                ))
            }
        };
        let error = |error: crate::SessionError| error.to_string();
        self.begin_transaction(Edit::ExternalChanges);
        let result = (|| -> Result<(), String> {
            for edit in edits {
                let index = match &edit {
                    VocalEdit::Lyric { index, .. }
                    | VocalEdit::Phonemes { index, .. }
                    | VocalEdit::Pin { index, .. }
                    | VocalEdit::Unpin { index, .. }
                    | VocalEdit::ClearTiming { index }
                    | VocalEdit::Scoop { index, .. }
                    | VocalEdit::Fall { index, .. }
                    | VocalEdit::Vibrato { index, .. }
                    | VocalEdit::ClearOrnament { index, .. } => *index,
                };
                let note = self
                    .midi_clip(clip)
                    .and_then(|clip| clip.notes.get(index))
                    .ok_or("Unknown note index; read_notes again")?;
                if let VocalEdit::Pin { phoneme, .. } | VocalEdit::Unpin { phoneme, .. } = &edit
                    && *phoneme >= note.phonemes.len()
                {
                    return Err("Unknown phoneme index; read_notes again".into());
                }
                match edit {
                    VocalEdit::Lyric { index, text } => {
                        if text.len() > 16384 {
                            return Err("Lyric is limited to 16384 bytes".into());
                        }
                        self.set_note_lyric(clip, index, &text).map_err(error)?;
                    }
                    VocalEdit::Phonemes { index, tokens } => {
                        if tokens.len() > 256
                            || tokens
                                .iter()
                                .any(|token| token.trim().is_empty() || token.len() > 256)
                        {
                            return Err(
                                "Use at most 256 nonempty phoneme tokens, each up to 256 bytes"
                                    .into(),
                            );
                        }
                        self.set_note_phonemes(clip, index, tokens).map_err(error)?;
                    }
                    VocalEdit::Pin {
                        index,
                        phoneme,
                        seconds,
                    } => {
                        self.set_phoneme_duration(clip, index, phoneme, Some(span(seconds, 10.0)?))
                            .map_err(error)?;
                    }
                    VocalEdit::Unpin { index, phoneme } => {
                        self.set_phoneme_duration(clip, index, phoneme, None)
                            .map_err(error)?;
                    }
                    VocalEdit::ClearTiming { index } => {
                        self.clear_phoneme_timing(clip, index).map_err(error)?;
                    }
                    VocalEdit::Scoop {
                        index,
                        depth_semitones,
                        seconds,
                    } => {
                        self.set_note_scoop(
                            clip,
                            index,
                            Some(Scoop {
                                depth: depth(depth_semitones)?,
                                seconds: span(seconds, 2.0)?,
                            }),
                        )
                        .map_err(error)?;
                    }
                    VocalEdit::Fall {
                        index,
                        depth_semitones,
                        seconds,
                    } => {
                        self.set_note_fall(
                            clip,
                            index,
                            Some(Fall {
                                depth: depth(depth_semitones)?,
                                seconds: span(seconds, 2.0)?,
                            }),
                        )
                        .map_err(error)?;
                    }
                    VocalEdit::Vibrato {
                        index,
                        depth_semitones,
                        rate_hz,
                        delay_seconds,
                        fade_in_seconds,
                    } => {
                        if !(0.1..=12.0).contains(&rate_hz)
                            || !(0.0..=10.0).contains(&delay_seconds)
                            || !(0.0..=10.0).contains(&fade_in_seconds)
                        {
                            return Err(
                                "Vibrato rate must be 0.1..12 Hz; delay/fade 0..10 seconds".into(),
                            );
                        }
                        self.set_note_vibrato(
                            clip,
                            index,
                            Some(Vibrato {
                                depth: depth(depth_semitones)?,
                                rate: rate_hz,
                                delay: delay_seconds,
                                fade_in: fade_in_seconds,
                            }),
                        )
                        .map_err(error)?;
                    }
                    VocalEdit::ClearOrnament { index, ornament } => match ornament {
                        Ornament::Scoop => self.set_note_scoop(clip, index, None),
                        Ornament::Fall => self.set_note_fall(clip, index, None),
                        Ornament::Vibrato => self.set_note_vibrato(clip, index, None),
                        Ornament::All => self.clear_note_ornaments(clip, index),
                    }
                    .map_err(error)?,
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.end_transaction();
                Ok(serde_json::json!({"clip":clip.0,"revision":self.agent_document_revision()?,"verify":"Read notes and inspect_singer; rerender a stale take with sing."}).to_string())
            }
            Err(error) => {
                self.revert_transaction();
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocal_batch_preserves_score_is_atomic_and_rejects_stale_indices() {
        let mut session = Session::new(crate::SessionOptions::headless()).unwrap();
        let track = session.add_singer_track("Vocal");
        let clip = session
            .add_midi_clip(track, "Words", Ticks::ZERO, Ticks(3840))
            .unwrap();
        session
            .replace_notes(clip, vec![Note::new(60, Ticks(13), Ticks(997))])
            .unwrap();
        let original = session.project().clone();
        let revision = session.agent_document_revision().unwrap();
        session
            .agent_edit_vocal_notes(
                clip.0,
                revision.clone(),
                vec![
                    VocalEdit::Lyric {
                        index: 0,
                        text: "か".into(),
                    },
                    VocalEdit::Scoop {
                        index: 0,
                        depth_semitones: 2.0,
                        seconds: 0.1,
                    },
                    VocalEdit::Pin {
                        index: 0,
                        phoneme: 0,
                        seconds: 0.15,
                    },
                ],
            )
            .unwrap();
        let note = &session.midi_clip(clip).unwrap().notes[0];
        assert_eq!(
            (note.pitch, note.start, note.length),
            (60, Ticks(13), Ticks(997))
        );
        assert_eq!(note.phoneme_seconds[0], 0.15);
        assert!(
            session
                .agent_edit_vocal_notes(clip.0, revision, vec![VocalEdit::ClearTiming { index: 0 }])
                .is_err()
        );
        let changed = session.project().clone();
        let revision = session.agent_document_revision().unwrap();
        assert!(
            session
                .agent_edit_vocal_notes(
                    clip.0,
                    revision,
                    vec![
                        VocalEdit::Lyric {
                            index: 0,
                            text: "さ".into()
                        },
                        VocalEdit::Pin {
                            index: 0,
                            phoneme: 999,
                            seconds: 0.2
                        },
                    ]
                )
                .is_err()
        );
        assert_eq!(session.project(), &changed);
        session.undo();
        assert_eq!(session.project(), &original);
    }
}
