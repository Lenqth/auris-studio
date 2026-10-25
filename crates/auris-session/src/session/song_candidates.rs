//! Independent instrumental drafts whose audition and adoption share one exact document.

use super::{Session, SessionOptions};
use crate::{Edit, SessionError};
use auris_compose::{SongSpec, compose, preset};
use auris_core::{AudioBuffer, Project};
use auris_engine::{OfflineOptions, RenderProgress};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A bounded musical direction proposed by the language model.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SongIdea {
    /// Short display title.
    pub title: String,
    /// Audible differences, in the user's language.
    pub description: String,
    /// Built-in whole-song preset name.
    pub preset: String,
    /// Quarter-note tempo, 50..180 BPM.
    pub tempo: f64,
    /// Tonal centre, such as C major or A minor.
    pub key: String,
    /// Pitched accompaniment density, 0.2..0.85.
    pub density: f32,
    /// Lead instrument: piano, guitar, flute, strings or synth.
    pub lead: String,
}

impl SongIdea {
    /// Validate all model-controlled fields before any composition or allocation.
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty()
            || self.title.chars().count() > 80
            || self.description.trim().is_empty()
            || self.description.chars().count() > 400
            || preset(&self.preset).is_none()
            || !self.tempo.is_finite()
            || !(50.0..=180.0).contains(&self.tempo)
            || !self.density.is_finite()
            || !(0.2..=0.85).contains(&self.density)
            || auris_core::theory::key::Key::parse(&self.key).is_none()
            || !matches!(
                self.lead.as_str(),
                "piano" | "guitar" | "flute" | "strings" | "synth"
            )
        {
            return Err("Invalid song direction returned by the model".into());
        }
        Ok(())
    }

    fn specification(&self, seconds: u32, seed: u64) -> Result<SongSpec, String> {
        self.validate()?;
        if !matches!(seconds, 30 | 60 | 90) {
            return Err("Choose 30, 60 or 90 seconds".into());
        }
        let mut spec = preset(&self.preset).ok_or("Unknown preset")?.spec();
        spec.title = self.title.clone();
        spec.tempo = self.tempo;
        spec.key = auris_core::theory::key::Key::parse(&self.key).ok_or("Invalid key")?;
        spec.seed = seed;
        spec.singer = None;
        spec.singer_speaker = None;
        let bars = ((f64::from(seconds) * self.tempo
            / 60.0
            / spec.meter.ticks_per_bar().as_beats()
            / spec.form.len().max(1) as f64)
            .round() as usize)
            .clamp(2, 16);
        for section in spec.sections.values_mut() {
            section.bars = bars;
            section.lyrics.clear();
        }
        for part in &mut spec.parts {
            part.source = None;
            part.program = None;
            if part.name == "melody" || part.name == "lead" {
                part.instrument = match self.lead.as_str() {
                    "piano" => "auris.physical.piano",
                    "guitar" => "auris.physical.guitar",
                    "flute" => "auris.physical.tin_whistle",
                    "strings" => "auris.physical.violin",
                    _ => "auris.synth.fm2",
                }
                .into();
                part.params.clear();
            }
            part.density = Some(self.density);
        }
        Ok(spec)
    }
}

/// Finished editable document and its full rendered audition. No external assets are needed.
pub struct SongCandidate {
    /// Original musical direction.
    pub idea: SongIdea,
    project: Project,
    /// Original render; prepare it for the current output rate before auditioning.
    pub audio: Arc<AudioBuffer>,
}

impl SongCandidate {
    /// Compose and render on a worker without opening any realtime device.
    pub fn generate(
        idea: SongIdea,
        brief: &str,
        seconds: u32,
        seed: u64,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        let spec = idea.specification(seconds, seed)?;
        if cancel.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        let composition = compose(&spec);
        if cancel.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        let mut session = Session::new(SessionOptions::headless().with_balance(false))
            .map_err(|e| e.to_string())?;
        let report = session
            .compose_without_balance(&composition)
            .map_err(|e| e.to_string())?;
        if !report.substituted.is_empty() {
            return Err(format!(
                "Unavailable built-in instruments: {:?}",
                report.substituted
            ));
        }
        let mut project = session.project().clone();
        project.creation_brief = Some(format!(
            "{}\nChosen direction: {} — {}",
            brief.chars().take(2400).collect::<String>(),
            idea.title,
            idea.description
        ));
        let audio = session
            .render_job()
            .render(
                &OfflineOptions {
                    sample_rate: Some(22_050.0),
                    ..Default::default()
                },
                &mut RenderProgress::default().cancelled_by(cancel),
            )
            .map_err(|e| e.to_string())?;
        if cancel.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        if audio.peak() <= 1e-8 {
            return Err("The candidate is silent".into());
        }
        Ok(Self {
            idea,
            project,
            audio: Arc::new(audio),
        })
    }
}

impl Session {
    /// Adopt the exact document that produced an audition, as one undoable edit.
    pub fn adopt_song_candidate(&mut self, candidate: &SongCandidate) -> Result<(), SessionError> {
        if self.is_recording() {
            return Err(SessionError::RecordingInProgress);
        }
        candidate.project.validate_loop_expansion()?;
        self.stop_output_preview();
        self.collect_hosted_state()?;
        self.record(Edit::Compose);
        self.replace_project(candidate.project.clone());
        self.dirty = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn idea() -> SongIdea {
        SongIdea {
            title: "Draft".into(),
            description: "Bright keys".into(),
            preset: "chiptune".into(),
            tempo: 120.0,
            key: "C major".into(),
            density: 0.5,
            lead: "piano".into(),
        }
    }
    #[test]
    fn invalid_model_fields_are_rejected_before_generation() {
        let mut bad = idea();
        bad.tempo = f64::NAN;
        assert!(bad.validate().is_err());
        let mut bad = idea();
        bad.lead = "external.vst".into();
        assert!(bad.validate().is_err());
        assert!(idea().specification(600, 0).is_err());
    }
    #[test]
    fn generated_audio_and_adopted_score_share_one_candidate() {
        let candidate = SongCandidate::generate(
            idea(),
            "Bright instrumental",
            30,
            42,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(candidate.audio.peak().is_finite() && candidate.audio.peak() > 0.001);
        assert!(
            candidate
                .project
                .creation_brief
                .as_ref()
                .unwrap()
                .contains("Bright instrumental")
        );
        let mut target = Session::new(SessionOptions::headless().with_balance(false)).unwrap();
        target.adopt_song_candidate(&candidate).unwrap();
        assert_eq!(target.project(), &candidate.project);
        assert!(target.is_dirty());
    }

    #[test]
    fn every_catalog_direction_has_only_builtin_instrumental_parts() {
        for preset in auris_compose::PRESETS {
            for lead in ["piano", "guitar", "flute", "strings", "synth"] {
                let mut direction = idea();
                direction.preset = preset.name.into();
                direction.lead = lead.into();
                let spec = direction.specification(30, 42).unwrap();
                assert!(spec.singer.is_none());
                assert!(spec.sections.values().all(|s| s.lyrics.is_empty()));
                assert!(
                    spec.parts
                        .iter()
                        .all(|p| p.source.is_none() && p.program.is_none())
                );
                let mut session =
                    Session::new(SessionOptions::headless().with_balance(false)).unwrap();
                let report = session.compose_without_balance(&compose(&spec)).unwrap();
                assert!(
                    report.substituted.is_empty(),
                    "{} / {}: {:?}",
                    preset.name,
                    lead,
                    report.substituted
                );
            }
        }
        assert!(SongCandidate::generate(idea(), "", 30, 42, &AtomicBool::new(true)).is_err());
    }

    #[test]
    fn adoption_preserves_the_exact_document_and_is_undoable() {
        let mut source = Session::new(SessionOptions::headless()).unwrap();
        source.add_audio_track("Exact retained track");
        let candidate = SongCandidate {
            idea: idea(),
            project: source.project().clone(),
            audio: Arc::new(AudioBuffer::from_planar(vec![vec![0.1; 100]], 48_000.0).unwrap()),
        };
        let mut target = Session::new(SessionOptions::headless()).unwrap();
        let before = target.project().clone();
        target.adopt_song_candidate(&candidate).unwrap();
        assert_eq!(target.project(), &candidate.project);
        target.undo();
        assert_eq!(target.project(), &before);
    }
}
