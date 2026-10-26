//! Validate and place external parts as one atomic, reversible edit.

use super::*;
use crate::prelude::{Note, Seconds};
use crate::transcription_draft::MultitrackTranscription;
use serde_json::json;

impl Session {
    /// Preview or accept all external parts, explicitly mapping source seconds to at_tick.
    /// Unknown instruments use reported guide sounds; original labels remain in the document.
    pub fn import_multitrack_transcription(
        &mut self,
        draft: &MultitrackTranscription,
        name: &str,
        at_tick: i64,
        preview: bool,
    ) -> Result<String, String> {
        draft.validate()?;
        if self.transaction.is_some() || at_tick < 0 || name.trim().is_empty() || name.len() > 256 {
            return Err(
                "Finish the current edit; use nonnegative at_tick and a bounded name".into(),
            );
        }
        let start = Ticks(at_tick);
        let origin = self.project.tempo_map.ticks_to_seconds(start).0;
        if !origin.is_finite()
            || origin + draft.source.end_seconds - draft.source.start_seconds > 86_400.0
        {
            return Err("Draft placement exceeds the 24-hour timeline".into());
        }
        let mut groups = Vec::new();
        for part in &draft.tracks {
            let mut notes = Vec::new();
            for event in &part.notes {
                let from = self
                    .project
                    .tempo_map
                    .seconds_to_ticks(Seconds(origin + event.start - draft.source.start_seconds));
                let to = self
                    .project
                    .tempo_map
                    .seconds_to_ticks(Seconds(origin + event.end - draft.source.start_seconds));
                if to <= from || from < start {
                    return Err(
                        "A note collapses at project tick resolution; no notes were imported"
                            .into(),
                    );
                }
                let mut note = Note::new(event.pitch, from - start, to - from);
                note.velocity = event.strength;
                notes.push(note);
            }
            groups.push(notes);
        }
        let preview_parts:Vec<_> = draft.tracks.iter().zip(&groups).map(|(part,notes)|json!({"name":part.name,
            "instrument":part.instrument,"program":part.program,"drums":part.drums,"notes":notes.len(),
            "sound_id":part.sound_id,"guide_sound":part.sound_id.is_none()})).collect();
        if preview {
            return Ok(json!({"preview":true,"revision":self.agent_document_revision()?,"parts":preview_parts,"at_tick":at_tick,"provenance_authenticity":"declared_not_verified"}).to_string());
        }
        self.begin_transaction(Edit::AddInstrumentTrack);
        let result = (|| {
            let mut created = Vec::new();
            let mut provenance_clip = None;
            for (index, (part, notes)) in draft.tracks.iter().zip(groups).enumerate() {
                let track_name = format!("{name}: {}", part.name);
                let track = if part.drums {
                    self.add_default_drum_track(&track_name)
                } else {
                    self.add_default_instrument_track(&track_name)
                }
                .map_err(|e| e.to_string())?;
                if let Some(sound) = &part.sound_id {
                    self.use_library_sound(track, sound, &[])?;
                }
                let length = notes.iter().map(Note::end).max().ok_or("Empty part")?;
                let clip = self
                    .add_midi_clip(track, &part.name, start, length)
                    .map_err(|e| e.to_string())?;
                self.project
                    .midi_clip_mut(clip)
                    .ok_or("Missing new clip")?
                    .notes = notes;
                provenance_clip.get_or_insert(clip);
                created.push(json!({"track":track.0,"clip":clip.0,"source_track":index,"sound_id":part.sound_id,"guide_sound":part.sound_id.is_none()}));
            }
            if let Some(clip) = provenance_clip {
                let provenance = json!({"draft":draft,"at_tick":at_tick,"created":created,"authenticity":"declared_not_verified"});
                self.project.transcription_provenance.insert(
                    clip,
                    serde_json::to_string(&provenance).map_err(|e| e.to_string())?,
                );
            }
            Ok(created)
        })();
        match result {
            Ok(created) => {
                self.end_transaction();
                Ok(json!({"created":created,"parts":preview_parts,"notes":draft.tracks.iter().map(|t|t.notes.len()).sum::<usize>(),
                    "revision":self.agent_document_revision()?,"provenance_authenticity":"declared_not_verified","unsaved":self.is_dirty()}).to_string())
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

    fn draft() -> MultitrackTranscription {
        serde_json::from_value(json!({"schema_version":1,"source":{"audio_sha256":"a".repeat(64),"start_seconds":10.0,"end_seconds":12.0},
            "recognizer":{"name":"external-research","model":"my-model"},"tracks":[
                {"name":"Piano","instrument":"piano","program":0,"drums":false,"notes":[{"pitch":108,"start":10.0,"end":11.5,"strength":0.9,"confidence":0.8},{"pitch":0,"start":10.5,"end":11.0,"strength":0.2}]},
                {"name":"Bass","instrument":null,"role":"bass","drums":false,"notes":[{"pitch":24,"start":10.25,"end":12.0,"strength":0.6}]},
                {"name":"Drums","instrument":"drum kit","drums":true,"notes":[{"pitch":127,"start":11.0,"end":11.1,"strength":0.4}]}],
            "provenance":{"origin":"declared by external model"}})).unwrap()
    }

    #[test]
    fn piano_bass_and_drums_preview_then_import_and_undo_as_one_edit() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        session.add_bus_track("Existing");
        let before = session.project().clone();
        let draft = draft();
        let preview: serde_json::Value = serde_json::from_str(
            &session
                .import_multitrack_transcription(&draft, "Draft", 3840, true)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(session.project(), &before);
        assert_eq!(preview["parts"].as_array().unwrap().len(), 3);
        session
            .import_multitrack_transcription(&draft, "Draft", 3840, false)
            .unwrap();
        let imported = session.project().clone();
        assert_eq!(imported.tracks.len(), 4);
        assert_eq!(imported.transcription_provenance.len(), 1);
        let piano = imported.tracks[1]
            .kind
            .note_clips()
            .unwrap()
            .first()
            .unwrap();
        assert_eq!(piano.notes[0].pitch, 108);
        assert_eq!(piano.notes[1].pitch, 0);
        assert_eq!(piano.notes[0].velocity, 0.9);
        assert_eq!(piano.start, Ticks(3840));
        assert_eq!(piano.notes[1].start, Ticks::QUARTER);
        assert!(imported.tracks[3].kind.is_drum());
        let restored: Project =
            serde_json::from_slice(&serde_json::to_vec(&imported).unwrap()).unwrap();
        assert_eq!(restored, imported);
        session.undo();
        assert_eq!(session.project(), &before);
        session.redo();
        assert_eq!(session.project(), &imported);
    }

    #[test]
    fn invalid_events_unknown_curves_and_collapsed_notes_are_rejected_before_edits() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        let before = session.project().clone();
        let mut invalid = draft();
        invalid.tracks[2].notes[0].pitch = 255;
        assert!(
            session
                .import_multitrack_transcription(&invalid, "Draft", 0, false)
                .is_err()
        );
        invalid = draft();
        invalid.tracks[2].notes[0].end = invalid.tracks[2].notes[0].start + 1e-10;
        assert!(
            session
                .import_multitrack_transcription(&invalid, "Draft", 0, false)
                .is_err()
        );
        let mut unknown = serde_json::to_value(draft()).unwrap();
        unknown["tracks"][0]["pitch_bends"] = json!([]);
        assert!(serde_json::from_value::<MultitrackTranscription>(unknown).is_err());
        assert_eq!(session.project(), &before);
        assert!(!session.can_undo());
    }

    #[test]
    fn a_later_invalid_playback_mapping_rolls_back_the_whole_import() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        let before = session.project().clone();
        let mut invalid = draft();
        invalid.tracks[2].sound_id = Some("nonexistent-sound".into());
        assert!(
            session
                .import_multitrack_transcription(&invalid, "Draft", 0, false)
                .is_err()
        );
        assert_eq!(session.project(), &before);
        assert!(!session.can_undo());
    }
}
