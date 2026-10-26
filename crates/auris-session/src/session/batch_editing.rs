//! Bounded edits committed as one undo step, with unchanged retries left alone.

use super::Session;
use crate::history::Edit;
use crate::param::ParamTarget;
use crate::prelude::{ClipId, Note, TrackId};
use std::collections::HashSet;

/// One static instrument control in its native units.
#[derive(Clone, Debug)]
pub struct InstrumentParamEdit {
    /// Stable instrument or drum track ID.
    pub track: TrackId,
    /// Exact descriptor key, such as `clap.18`.
    pub param: String,
    /// Requested value in descriptor units.
    pub value: f32,
}

/// The observed before/after values of one instrument control.
#[derive(Debug, serde::Serialize)]
pub struct InstrumentParamChange {
    /// Stable instrument track ID.
    pub track: TrackId,
    /// Exact descriptor key.
    pub param: String,
    /// Human-readable descriptor name.
    pub name: String,
    /// Value before this operation.
    pub previous: f32,
    /// Value after this operation.
    pub value: f32,
    /// Descriptor unit label.
    pub unit: String,
    /// Arithmetic domain of the returned values: normalized for VST3, otherwise native.
    pub value_domain: &'static str,
    /// The plugin's display unit, which does not change the normalized VST3 range.
    pub display_unit: String,
    /// Whether playback automation overrides the static value.
    pub automated: bool,
}

impl Session {
    /// Sets 1..256 instrument controls as one undo step, validating every value first.
    /// Duplicate targets, invalid ranges and fractional discrete choices are refused.
    /// Identical retries do not dirty the document or create history.
    pub fn set_instrument_params(
        &mut self,
        edits: &[InstrumentParamEdit],
    ) -> Result<Vec<InstrumentParamChange>, String> {
        if self.transaction.is_some() {
            return Err("Finish the current edit before setting a parameter batch".into());
        }
        if !(1..=256).contains(&edits.len()) {
            return Err("edits must contain 1..256 instrument parameters".into());
        }
        let mut prepared = Vec::with_capacity(edits.len());
        let mut seen = HashSet::new();
        for edit in edits {
            if self
                .project
                .track(edit.track)
                .and_then(|track| track.kind.as_instrument())
                .is_none()
            {
                return Err(format!(
                    "track {} is not an instrument or drum track",
                    edit.track.0
                ));
            }
            let descriptors = self.instrument_descriptors(edit.track);
            let descriptor = descriptors.iter().find(|descriptor| descriptor.key == edit.param)
                .ok_or_else(|| format!("track {}: unknown instrument parameter; use automation read to discover exact keys", edit.track.0))?;
            if descriptor.read_only {
                return Err(format!(
                    "track {}: {} is read-only",
                    edit.track.0, edit.param
                ));
            }
            if !seen.insert((edit.track, descriptor.id)) {
                return Err(format!(
                    "duplicate target: track {}, {}",
                    edit.track.0, edit.param
                ));
            }
            if !(descriptor.min..=descriptor.max).contains(&edit.value) {
                return Err(format!(
                    "track {}: {} must be between {} and {} ({:?})",
                    edit.track.0, edit.param, descriptor.min, descriptor.max, descriptor.unit
                ));
            }
            let clamped = descriptor.clamp(edit.value);
            let tolerance = f32::EPSILON * edit.value.abs().max(clamped.abs()).max(1.0) * 4.0;
            if (clamped - edit.value).abs() > tolerance {
                return Err(format!(
                    "track {}: {} requires a discrete step; nearest valid value is {clamped}",
                    edit.track.0, edit.param
                ));
            }
            let target = ParamTarget::Instrument {
                track: edit.track,
                param: descriptor.id,
            };
            prepared.push((
                edit.track,
                target,
                descriptor.clone(),
                edit.value,
                self.param_value(target, descriptor),
            ));
        }
        self.begin_transaction(Edit::ExternalChanges);
        for (_, target, _, value, previous) in &prepared {
            if value != previous {
                self.set_param(*target, *value);
            }
        }
        self.end_transaction();
        Ok(prepared
            .into_iter()
            .map(
                |(track, target, descriptor, _, previous)| InstrumentParamChange {
                    track,
                    param: descriptor.key.to_string(),
                    name: descriptor.name.to_string(),
                    previous,
                    value: self.param_value(target, &descriptor),
                    unit: format!(
                        "{:?}",
                        if descriptor.key.starts_with("vst3.") {
                            auris_core::param::ParamUnit::Plain
                        } else {
                            descriptor.unit
                        }
                    ),
                    value_domain: if descriptor.key.starts_with("vst3.") {
                        "normalized"
                    } else {
                        "native"
                    },
                    display_unit: format!("{:?}", descriptor.unit),
                    automated: self.is_automated(target),
                },
            )
            .collect())
    }

    /// Replaces 1..16 clips, up to 65536 total notes, in one undo step.
    /// Any failure restores the document and its dirty state. Repeated identical edits are no-ops.
    pub fn replace_notes_batch(
        &mut self,
        replacements: Vec<(ClipId, Vec<Note>)>,
    ) -> Result<usize, String> {
        if self.transaction.is_some() {
            return Err("Finish the current edit before replacing a note batch".into());
        }
        if !(1..=16).contains(&replacements.len())
            || replacements
                .iter()
                .map(|(_, notes)| notes.len())
                .sum::<usize>()
                > 65_536
        {
            return Err(
                "replacements must contain 1..16 clips and at most 65536 total notes".into(),
            );
        }
        let mut seen = HashSet::new();
        for (clip, _) in &replacements {
            if !seen.insert(*clip) {
                return Err(format!("duplicate clip {}", clip.0));
            }
        }
        self.begin_transaction(Edit::ExternalChanges);
        let mut changed = 0;
        for (clip, notes) in replacements {
            if self
                .project
                .midi_clip(clip)
                .is_some_and(|(_, target)| target.notes != notes)
            {
                changed += 1;
            }
            if let Err(error) = self.replace_notes(clip, notes) {
                self.revert_transaction();
                return Err(format!("clip {}: {error}", clip.0));
            }
        }
        self.end_transaction();
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionOptions;
    use crate::prelude::Ticks;

    #[test]
    fn note_batches_rollback_then_undo_together_and_retries_are_unchanged() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        let track = session.add_default_instrument_track("Lead").unwrap();
        let clips = (0..12)
            .map(|index| {
                session
                    .add_midi_clip(track, "Phrase", Ticks::QUARTER * index, Ticks::QUARTER)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        session.forget_history();
        let before = session.project().clone();
        let edits = || {
            clips
                .iter()
                .map(|clip| (*clip, vec![Note::new(60, Ticks::ZERO, Ticks::QUARTER)]))
                .collect::<Vec<_>>()
        };
        let mut invalid = edits();
        invalid[11].1[0].pitch = 128;
        assert!(session.replace_notes_batch(invalid).is_err());
        assert_eq!(session.project(), &before);
        assert!(session.undo().is_none());
        assert_eq!(session.replace_notes_batch(edits()).unwrap(), 12);
        assert_eq!(session.replace_notes_batch(edits()).unwrap(), 0);
        assert!(session.undo().is_some());
        assert_eq!(session.project(), &before);
        assert!(session.undo().is_none());
    }

    #[test]
    fn parameter_batches_validate_every_target_before_editing_and_undo_together() {
        let mut session = Session::new(SessionOptions::headless()).unwrap();
        let tracks = (0..18)
            .map(|index| {
                session
                    .add_default_instrument_track(format!("Lead {index}"))
                    .unwrap()
            })
            .collect::<Vec<_>>();
        let mut edits = Vec::new();
        for track in tracks {
            let descriptors = session.instrument_descriptors(track);
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.min < descriptor.max)
                .unwrap();
            let target = ParamTarget::Instrument {
                track,
                param: descriptor.id,
            };
            let previous = session.param_value(target, descriptor);
            let value = if previous != descriptor.min {
                descriptor.min
            } else {
                descriptor.max
            };
            edits.push(InstrumentParamEdit {
                track,
                param: descriptor.key.to_string(),
                value,
            });
        }
        session.forget_history();
        let before = session.project().clone();
        let mut invalid = edits.clone();
        invalid[17].param = "unknown".into();
        assert!(session.set_instrument_params(&invalid).is_err());
        assert_eq!(session.project(), &before);
        assert!(session.undo().is_none());
        let report = session.set_instrument_params(&edits).unwrap();
        assert_eq!(report.len(), 18);
        assert!(report.iter().all(|change| change.previous != change.value));
        session.set_instrument_params(&edits).unwrap();
        assert!(session.undo().is_some());
        assert_eq!(session.project(), &before);
        assert!(session.undo().is_none());
    }
}
