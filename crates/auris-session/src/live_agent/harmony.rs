//! Bounded live harmony edits, validated before recording a single undo transaction.
use super::*;

impl Session {
    pub(super) fn agent_edit_harmony(
        &mut self,
        start_bar: u32,
        bars: u32,
        key: Option<&str>,
        chords: Option<&str>,
        clear_chords: bool,
        section: Option<&str>,
    ) -> Result<String, String> {
        if start_bar == 0 || !(1..=4096).contains(&bars) {
            return Err("start_bar must be at least 1 and bars within 1..4096".into());
        }
        if (key.is_none() && chords.is_none() && !clear_chords && section.is_none())
            || (clear_chords && chords.is_some())
        {
            return Err("provide key, chords, section, or clear_chords without chords".into());
        }
        if section.is_some_and(|label| {
            label.trim().is_empty() || label.len() > 256 || label.chars().any(char::is_control)
        }) {
            return Err("section must be a nonempty label within 256 UTF-8 bytes, without control characters".into());
        }
        let after = start_bar.checked_add(bars).ok_or("bar range overflow")?;
        let key = key
            .map(|text| MusicalKey::parse(text).ok_or_else(|| format!("invalid key: {text}")))
            .transpose()?;
        let start = self.project().signatures.bar_start(start_bar);
        let end = self.project().signatures.bar_start(after);
        let previous = self.project().harmony.key_at(end);
        let previous_section = self.project().sections.label_at(end).map(str::to_owned);
        let internal_sections = self
            .project()
            .sections
            .points()
            .iter()
            .filter(|point| point.tick > start && point.tick < end)
            .map(|point| point.tick)
            .collect::<Vec<_>>();
        let chart = chords
            .map(|text| {
                Chart::parse(text)
                    .filter(|chart| !chart.bars.is_empty())
                    .map(|chart| {
                        chart
                            .spelled_in(key.unwrap_or_else(|| self.project().harmony.key_at(start)))
                    })
                    .ok_or_else(|| format!("invalid Roman-numeral chord chart: {text}"))
            })
            .transpose()?;
        self.begin_transaction(crate::Edit::StampProgression);
        if let Some(key) = key {
            self.set_key(start, key);
            self.set_key(end, previous);
        }
        if clear_chords || chart.is_some() {
            self.clear_harmony(start, end);
        }
        if let Some(mut chart) = chart {
            let progression = std::mem::take(&mut chart.bars);
            for offset in 0..bars {
                let at = self.project().signatures.bar_start(start_bar + offset);
                chart.bars.clear();
                chart
                    .bars
                    .push(progression[offset as usize % progression.len()].clone());
                self.stamp_progression(&chart, at, 1);
            }
        }
        if let Some(label) = section {
            for tick in internal_sections {
                self.remove_section(tick);
            }
            self.set_section(start, Some(label.trim().to_owned()));
            self.set_section(end, previous_section);
        }
        self.end_transaction();
        Ok(serde_json::json!({"start_bar":start_bar,"bars":bars,"key":self.project().harmony.key_at(start).to_text(),"section":self.project().sections.label_at(start),"unsaved":true,"notes_preserved":true,"usage":"inspect_project reads the current harmony and section maps."}).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_only_edits_bound_the_form_preserve_music_and_undo_together() {
        let mut session =
            Session::new(crate::SessionOptions::headless().with_balance(false)).unwrap();
        session.add_default_instrument_track("Existing").unwrap();
        let track = session.project().tracks[0].id;
        let clip = session
            .add_midi_clip(track, "Existing music", Ticks::ZERO, Ticks::QUARTER * 24)
            .unwrap();
        session
            .add_note(clip, Note::new(60, Ticks::QUARTER * 8, Ticks::QUARTER))
            .unwrap();
        session.set_section(Ticks::ZERO, Some("Old verse".into()));
        session.set_section(Ticks::QUARTER * 8, Some("Old bridge".into()));
        let before = session.project().clone();
        let command: Command = serde_json::from_value(serde_json::json!({
            "action":"edit_harmony","start_bar":2,"bars":3,"section":"Chorus"
        }))
        .expect("section metadata must be callable without changing harmony");
        session.agent_command(command).unwrap();
        assert_eq!(
            session.project().sections.label_at(Ticks::QUARTER * 4),
            Some("Chorus")
        );
        assert_eq!(
            session.project().sections.label_at(Ticks::QUARTER * 8),
            Some("Chorus")
        );
        assert_eq!(
            session.project().sections.label_at(Ticks::QUARTER * 16),
            Some("Old bridge")
        );
        assert_eq!(
            session.project().sections.label_at(Ticks::ZERO),
            Some("Old verse")
        );
        assert_eq!(session.project().tracks, before.tracks);
        assert_eq!(session.project().harmony, before.harmony);
        assert!(session.is_dirty());
        let after = session.project().clone();
        session.undo();
        assert_eq!(session.project(), &before);
        session.redo();
        assert_eq!(session.project(), &after);
    }

    #[test]
    fn invalid_section_labels_or_charts_leave_metadata_and_history_untouched() {
        let mut session =
            Session::new(crate::SessionOptions::headless().with_balance(false)).unwrap();
        let before = session.project().clone();
        for label in [" ", "bad\nlabel", &"長".repeat(100)] {
            assert!(
                session
                    .agent_edit_harmony(1, 4, None, None, false, Some(label))
                    .is_err()
            );
            assert_eq!(session.project(), &before);
        }
        assert!(
            session
                .agent_edit_harmony(1, 4, None, Some("invalid!"), false, Some("Chorus"))
                .is_err()
        );
        assert_eq!(session.project(), &before);
    }

    #[test]
    fn harmony_writes_metadata_preserves_notes_and_is_one_undo_step() {
        let mut s = Session::new(crate::SessionOptions::headless().with_balance(false)).unwrap();
        let track = s.add_default_instrument_track("Existing").unwrap();
        let clip = s
            .add_midi_clip(track, "Phrase", Ticks::ZERO, Ticks::QUARTER * 32)
            .unwrap();
        s.add_note(clip, Note::new(60, Ticks::ZERO, Ticks::QUARTER))
            .unwrap();
        let before = s.project().clone();
        s.agent_edit_harmony(
            1,
            4,
            Some("D major"),
            Some("| I | IV | V | I |"),
            false,
            None,
        )
        .unwrap();
        assert_eq!(s.project().harmony.key_at(Ticks::ZERO).to_text(), "D major");
        for (bar, chord) in ["D", "G", "A", "D"].iter().enumerate() {
            assert_eq!(
                s.project()
                    .harmony
                    .chord_at(Ticks::QUARTER * (bar * 4) as i64)
                    .unwrap()
                    .to_string(),
                *chord
            );
        }
        assert_eq!(
            s.project().harmony.key_at(Ticks::QUARTER * 16),
            before.harmony.key_at(Ticks::QUARTER * 16)
        );
        assert_eq!(s.project().tracks, before.tracks);
        let after = s.project().clone();
        assert!(
            s.agent_edit_harmony(1, 4, Some("A minor"), Some("invalid chord!"), false, None)
                .is_err()
        );
        assert_eq!(s.project(), &after);
        s.undo();
        assert_eq!(s.project(), &before);
    }
}
