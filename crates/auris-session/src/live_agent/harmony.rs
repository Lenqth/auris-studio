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
    ) -> Result<String, String> {
        if start_bar == 0 || !(1..=4096).contains(&bars) {
            return Err("start_bar must be at least 1 and bars within 1..4096".into());
        }
        if (key.is_none() && chords.is_none() && !clear_chords)
            || (clear_chords && chords.is_some())
        {
            return Err("provide key and/or chords, or clear_chords without chords".into());
        }
        let after = start_bar.checked_add(bars).ok_or("bar range overflow")?;
        let key = key
            .map(|text| MusicalKey::parse(text).ok_or_else(|| format!("invalid key: {text}")))
            .transpose()?;
        let start = self.project().signatures.bar_start(start_bar);
        let end = self.project().signatures.bar_start(after);
        let previous = self.project().harmony.key_at(end);
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
        self.end_transaction();
        Ok(serde_json::json!({"start_bar":start_bar,"bars":bars,"key":self.project().harmony.key_at(start).to_text(),"unsaved":true,"notes_preserved":true,"usage":"inspect_project reads the current harmony map."}).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        s.agent_edit_harmony(1, 4, Some("D major"), Some("| I | IV | V | I |"), false)
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
            s.agent_edit_harmony(1, 4, Some("A minor"), Some("invalid chord!"), false)
                .is_err()
        );
        assert_eq!(s.project(), &after);
        s.undo();
        assert_eq!(s.project(), &before);
    }
}
