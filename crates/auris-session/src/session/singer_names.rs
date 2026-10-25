//! Singer clip titles are the complete written score, never its pronunciation.

use super::Session;
use auris_core::{ClipId, MidiClip};

fn lyric_name(clip: &MidiClip) -> String {
    let mut notes: Vec<_> = clip.notes.iter().enumerate().collect();
    notes.sort_by_key(|(index, note)| (note.start, *index));
    notes
        .into_iter()
        .filter_map(|(_, note)| {
            (note.lyric != super::singer::LYRIC_CONTINUATION).then_some(note.lyric.as_str())
        })
        .collect()
}

impl Session {
    pub(super) fn singer_clip_name(&self, clip: ClipId) -> Option<String> {
        let (track, clip) = self.project.midi_clip(clip)?;
        self.project
            .track(track)?
            .kind
            .is_singer()
            .then(|| lyric_name(clip))
    }

    pub(super) fn sync_singer_clip_names(&mut self) {
        for track in &mut self.project.tracks {
            if track.kind.is_singer()
                && let Some(clips) = track.kind.note_clips_mut()
            {
                for clip in clips {
                    clip.name = lyric_name(clip);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::{Note, Ticks};

    #[test]
    fn written_kanji_survive_note_edits_split_and_history_without_using_phonemes() {
        let mut session = super::super::fixtures::session();
        let track = session.add_singer_track("Singer");
        let clip = session
            .add_midi_clip(track, "Verse", Ticks::ZERO, Ticks(1920))
            .unwrap();
        let notes = vec![
            Note {
                lyric: "桜、咲いた！".into(),
                phonemes: vec!["s".into(), "a".into()],
                ..Note::new(60, Ticks::ZERO, Ticks(480))
            },
            Note {
                lyric: "+".into(),
                phonemes: vec!["k".into(), "u".into()],
                ..Note::new(62, Ticks(480), Ticks(480))
            },
        ];
        session.replace_notes(clip, notes.clone()).unwrap();
        assert_eq!(session.midi_clip(clip).unwrap().name, "桜、咲いた！");
        assert_eq!(session.midi_clip(clip).unwrap().notes, notes);
        let before = session.project().clone();
        let right = session.split_clip(clip, Ticks(480)).unwrap();
        assert_eq!(session.midi_clip(right).unwrap().name, "桜、咲いた！");
        assert_eq!(
            session.midi_clip(right).unwrap().notes[0].phonemes,
            notes[1].phonemes
        );
        session.undo();
        assert_eq!(session.project(), &before);
        session.redo();
        assert_eq!(session.midi_clip(right).unwrap().name, "桜、咲いた！");
        session.remove_notes(right, &[0]).unwrap();
        assert_eq!(session.midi_clip(right).unwrap().name, "");
    }
}
