//! Exact tick-based note editing shared by file and live agents.

use crate::{Session, prelude::*};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// A newly authored note in clip-relative ticks.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthoredNote {
    /// MIDI pitch, 0..127.
    #[schemars(range(min = 0, max = 127))]
    pub pitch: u8,
    /// Nonnegative clip-relative attack tick.
    #[schemars(range(min = 0))]
    pub start_tick: i64,
    /// Positive held duration in ticks.
    #[schemars(range(min = 1))]
    pub length_ticks: i64,
    /// Attack strength, 0..1.
    #[schemars(range(min = 0, max = 1))]
    pub velocity: f32,
}

impl AuthoredNote {
    fn into_note(self) -> Note {
        Note {
            velocity: self.velocity,
            ..Note::new(self.pitch, Ticks(self.start_tick), Ticks(self.length_ticks))
        }
    }
}

/// Partial changes to an existing note; all other metadata is preserved.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoteUpdate {
    /// Zero-based storage index returned by read_notes.
    pub index: usize,
    /// Optional MIDI pitch, 0..127.
    pub pitch: Option<u8>,
    /// Optional nonnegative clip-relative tick.
    pub start_tick: Option<i64>,
    /// Optional positive duration in ticks.
    pub length_ticks: Option<i64>,
    /// Optional attack strength, 0..1.
    pub velocity: Option<f32>,
}

/// One clip's patch or complete replacement within an atomic edit.
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NoteEdit {
    /// Update and remove original indices, then append new notes.
    Patch {
        /// Stable MIDI clip ID.
        clip: u64,
        /// Original storage indices to remove; duplicate indices are refused.
        #[serde(default)]
        remove: Vec<usize>,
        /// Partial updates against original storage indices; removed notes cannot be updated.
        #[serde(default)]
        update: Vec<NoteUpdate>,
        /// New notes appended after removals and updates.
        #[serde(default)]
        add: Vec<AuthoredNote>,
    },
    /// Replace all authored notes; an empty array clears the clip.
    Replace {
        /// Stable MIDI clip ID.
        clip: u64,
        /// Complete replacement, in exact ticks.
        notes: Vec<AuthoredNote>,
    },
}

impl NoteEdit {
    /// Whether this edit may remove or replace existing note data.
    pub fn requires_confirmation(&self) -> bool {
        match self {
            Self::Replace { .. } => true,
            Self::Patch { remove, .. } => !remove.is_empty(),
        }
    }
}

impl Session {
    /// Content-based revision, also stable across saved-project calls and process restarts.
    pub fn agent_document_revision(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self.project()).map_err(|e| e.to_string())?;
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    /// Apply exact note changes atomically, preserving clip recipes, curves and performance.
    pub fn agent_edit_notes(
        &mut self,
        expected_revision: &str,
        edits: Vec<NoteEdit>,
    ) -> Result<String, String> {
        if edits.is_empty() || edits.len() > 16 {
            return Err("edits must contain 1..16 clips".into());
        }
        if self.agent_document_revision()? != expected_revision {
            return Err("Document changed; call read_notes again before editing".into());
        }
        let mut replacements = Vec::with_capacity(edits.len());
        let mut seen = HashSet::new();
        let mut total = 0usize;
        for edit in edits {
            let clip = match &edit {
                NoteEdit::Patch { clip, .. } | NoteEdit::Replace { clip, .. } => ClipId(*clip),
            };
            if !seen.insert(clip) {
                return Err("Duplicate clip in edits".into());
            }
            let (_, target) = self
                .project()
                .midi_clip(clip)
                .ok_or("Unknown MIDI clip ID")?;
            let notes = match edit {
                NoteEdit::Replace { notes, .. } => {
                    notes.into_iter().map(AuthoredNote::into_note).collect()
                }
                NoteEdit::Patch {
                    remove,
                    update,
                    add,
                    ..
                } => {
                    if remove.len() > 65_536 || update.len() > 65_536 || add.len() > 65_536 {
                        return Err("At most 65536 note changes per clip".into());
                    }
                    let removals: HashSet<_> = remove.iter().copied().collect();
                    if removals.len() != remove.len()
                        || remove.iter().any(|&i| i >= target.notes.len())
                    {
                        return Err("Remove indices must be unique existing note indices".into());
                    }
                    let mut notes = target.notes.clone();
                    let mut updated = HashSet::new();
                    for change in update {
                        if removals.contains(&change.index) || !updated.insert(change.index) {
                            return Err(
                                "Cannot update a removed note or the same note twice".into()
                            );
                        }
                        let note = notes.get_mut(change.index).ok_or("Unknown note index")?;
                        if let Some(value) = change.pitch {
                            note.pitch = value;
                        }
                        if let Some(value) = change.start_tick {
                            note.start = Ticks(value);
                        }
                        if let Some(value) = change.length_ticks {
                            note.length = Ticks(value);
                        }
                        if let Some(value) = change.velocity {
                            note.velocity = value;
                        }
                    }
                    let mut index = 0;
                    notes.retain(|_| {
                        let keep = !removals.contains(&index);
                        index += 1;
                        keep
                    });
                    notes.extend(add.into_iter().map(AuthoredNote::into_note));
                    notes
                }
            };
            total += notes.len();
            if total > 65_536 {
                return Err("At most 65536 resulting notes per batch".into());
            }
            replacements.push((clip, notes));
        }
        let changed = self.replace_notes_batch(replacements)?;
        Ok(
            serde_json::json!({"changed_clips":changed,"revision":self.agent_document_revision()?})
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_edits_preserve_metadata_rollback_stale_reads_and_undo_as_one() {
        let mut session = Session::new(crate::SessionOptions::headless()).unwrap();
        let track = session.add_default_instrument_track("Lead").unwrap();
        let clip = session
            .add_midi_clip(track, "Notes", Ticks::ZERO, Ticks(3840))
            .unwrap();
        let mut note = Note::new(60, Ticks(13), Ticks(97));
        note.lyric = "keep".into();
        note.phonemes = vec!["a".into()];
        session.replace_notes(clip, vec![note]).unwrap();
        let before = session.project().clone();
        let revision = session.agent_document_revision().unwrap();
        let edit = NoteEdit::Patch {
            clip: clip.0,
            remove: vec![],
            add: vec![],
            update: vec![NoteUpdate {
                index: 0,
                pitch: Some(62),
                start_tick: None,
                length_ticks: None,
                velocity: None,
            }],
        };
        session
            .agent_edit_notes(&revision, vec![edit.clone()])
            .unwrap();
        let result = &session.project().midi_clip(clip).unwrap().1.notes[0];
        assert_eq!(
            (
                result.start.raw(),
                result.length.raw(),
                result.lyric.as_str(),
                result.phonemes.as_slice()
            ),
            (13, 97, "keep", ["a".to_owned()].as_slice())
        );
        let after = session.project().clone();
        assert!(session.agent_edit_notes(&revision, vec![edit]).is_err());
        assert_eq!(session.project(), &after);
        session.undo();
        assert_eq!(session.project(), &before);
        let valid = NoteEdit::Replace {
            clip: clip.0,
            notes: vec![],
        };
        let invalid = NoteEdit::Replace {
            clip: u64::MAX,
            notes: vec![],
        };
        assert!(
            session
                .agent_edit_notes(&revision, vec![valid, invalid])
                .is_err()
        );
        assert_eq!(session.project(), &before);
    }
}
