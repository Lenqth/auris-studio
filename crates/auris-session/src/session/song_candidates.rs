//! Independent song drafts whose audition and adoption share one exact document.

use super::{Session, SessionOptions};
use crate::{Edit, SessionError};
use auris_compose::{SongSpec, compose, preset};
use auris_core::{AssetPath, AudioBuffer, Project};
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
        if !(30..=300).contains(&seconds) {
            return Err("Choose a duration between 30 and 300 seconds".into());
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
            .clamp(2, 64);
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

/// A selected voice and speaker, independent of any live project.
#[derive(Clone, Debug, PartialEq)]
pub struct SongSinger {
    /// Display name for the singer's editable track.
    pub name: String,
    /// Installed voice configuration file.
    pub path: std::path::PathBuf,
    /// Exact speaker name, or the voice's default.
    pub speaker: Option<String>,
}

/// How multiple voices share the score.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SongSinging {
    /// Alternate lyrical sections, with all voices joining the last section.
    #[default]
    Auto,
    /// Alternate lyrical sections.
    Alternate,
    /// Sing together, with additional voices on diatonic harmony parts.
    Harmony,
}

/// An exact instrument choice for an arrangement role.
#[derive(Clone, Debug)]
pub struct SongSourceChoice {
    /// Role name, or `drums` for the entire kit.
    pub role: String,
    /// Built-in instrument identifier when no external source is selected.
    pub instrument: String,
    /// Exact library source, if selected.
    pub source: Option<auris_compose::spec::PartSource>,
}

/// Immutable controls captured before planning or generating a candidate batch.
#[derive(Clone, Debug, Default)]
pub struct SongRequest {
    /// Approximate seconds; absence lets the preset and lyric capacity decide.
    pub seconds: Option<u32>,
    /// Authored lyrics; line breaks separate phrases and blank lines separate sections.
    pub lyrics: String,
    /// Selected singers. Empty requests an instrumental song.
    pub singers: Vec<SongSinger>,
    /// Vocal arrangement policy.
    pub singing: SongSinging,
    /// Explicit part choices; unspecified roles retain their arrangement's instruments.
    pub sources: Vec<SongSourceChoice>,
    /// User-configured trusted plugin search paths.
    pub plugin_paths: Vec<std::path::PathBuf>,
    /// Japanese pronunciation dictionary used by the originating session, if available.
    pub japanese_dictionary: Option<std::path::PathBuf>,
}

impl SongRequest {
    /// Reject inconsistent controls before starting any model or render work.
    pub fn validate(&self) -> Result<(), String> {
        if self.seconds.is_some_and(|s| !(30..=300).contains(&s)) {
            return Err("Choose a duration between 30 and 300 seconds".into());
        }
        if !self.singers.is_empty() && self.lyrics.trim().is_empty() {
            return Err("Enter or generate lyrics before creating a vocal song".into());
        }
        if self.lyrics.chars().count() > 8000 {
            return Err("Lyrics must contain at most 8000 characters".into());
        }
        for singer in &self.singers {
            if !singer.path.is_file() {
                return Err(format!("Voice is unavailable: {}", singer.name));
            }
        }
        Ok(())
    }

    fn specification(
        &self,
        idea: &SongIdea,
        seed: u64,
        session: &Session,
    ) -> Result<SongSpec, String> {
        self.validate()?;
        let mut spec = idea.specification(self.seconds.unwrap_or(120), seed)?;
        if self.seconds.is_none() {
            let original = preset(&idea.preset).ok_or("Unknown preset")?.spec();
            for (name, section) in &mut spec.sections {
                section.bars = original.sections[name].bars;
            }
        }
        for choice in &self.sources {
            let mut matched = false;
            for part in &mut spec.parts {
                let role = part.role.name();
                if role == choice.role
                    || (choice.role == "drums"
                        && matches!(role, "kick" | "snare" | "hat" | "crash"))
                {
                    part.source = choice.source.clone();
                    part.instrument.clone_from(&choice.instrument);
                    part.params.clear();
                    part.program = None;
                    matched = true;
                }
            }
            if !matched {
                let role = auris_compose::spec::Role::parse(&choice.role)
                    .ok_or_else(|| format!("The arrangement has no {} part", choice.role))?;
                let mut part = auris_compose::spec::PartSpec::of_role(choice.role.clone(), role);
                part.instrument.clone_from(&choice.instrument);
                part.source = choice.source.clone();
                spec.parts.push(part);
                for section in spec.sections.values_mut() {
                    if !section.parts.is_empty() {
                        section.parts.push(choice.role.clone());
                    }
                }
            }
        }
        if self.singers.is_empty() {
            return Ok(spec);
        }
        // Give every occurrence its own lyric identity; repeated choruses must not
        // accidentally repeat or discard an authored verse.
        let old = spec.sections.clone();
        spec.sections.clear();
        for (index, name) in spec.form.iter_mut().enumerate() {
            let mut section = old.get(name).ok_or("Unknown song section")?.clone();
            *name = format!("section{}", index + 1);
            section.name.clone_from(name);
            section.melody_from = None;
            spec.sections.insert(name.clone(), section);
        }
        let normalized = self.lyrics.replace("\r\n", "\n");
        let stanzas: Vec<_> = normalized
            .split("\n\n")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let phrases: Vec<_> = normalized
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let sections: Vec<String> = if stanzas.len() > 1 {
            let count = stanzas.len().min(spec.form.len());
            (0..count)
                .map(|index| {
                    stanzas[index * stanzas.len() / count..(index + 1) * stanzas.len() / count]
                        .join("\n\n")
                })
                .collect()
        } else {
            let count = spec.form.len().min(phrases.len());
            (0..count)
                .map(|index| {
                    phrases[index * phrases.len() / count..(index + 1) * phrases.len() / count]
                        .join("\n")
                })
                .collect()
        };
        let slots = sections.len();
        if slots < self.singers.len() && self.singing != SongSinging::Harmony {
            return Err("Add more lyric phrases for alternating singers, or choose harmony".into());
        }
        for (index, lyrics) in sections.into_iter().enumerate() {
            let section = spec
                .sections
                .get_mut(&spec.form[index])
                .ok_or("Unknown section")?;
            section.lyrics = lyrics;
            if self.seconds.is_none() {
                let measured = session.measure_lyrics(&section.lyrics, spec.meter);
                while measured.fits_in_bars(spec.meter, section.bars) != Some(true)
                    && section.bars < 64
                {
                    section.bars += 1;
                }
            }
        }
        if spec.total_bars() as f64 * spec.meter.ticks_per_bar().as_beats() * 60.0 / spec.tempo
            > 330.0
        {
            return Err("The lyrics need more than five minutes; shorten them or choose a shorter arrangement".into());
        }
        session
            .validate_song_lyrics(&spec)
            .map_err(|e| e.to_string())?;
        Ok(spec)
    }
}

/// Finished editable document and its full rendered audition, including retained vocal assets.
pub struct SongCandidate {
    /// Original musical direction.
    pub idea: SongIdea,
    project: Project,
    /// Original render; prepare it for the current output rate before auditioning.
    pub audio: Arc<AudioBuffer>,
    _assets: Option<tempfile::TempDir>,
}

impl SongCandidate {
    /// Track names in the retained arrangement, including each selected singer.
    pub fn part_names(&self) -> Vec<String> {
        self.project
            .tracks
            .iter()
            .filter(|track| track.kind.note_clips().is_some())
            .map(|track| track.name.clone())
            .collect()
    }
    /// Compose and render on a worker without opening any realtime device.
    pub fn generate(
        idea: SongIdea,
        brief: &str,
        seconds: u32,
        seed: u64,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        Self::generate_with(
            idea,
            brief,
            &SongRequest {
                seconds: Some(seconds),
                ..Default::default()
            },
            seed,
            cancel,
            &mut |_| {},
        )
    }

    /// Generate a complete candidate on its own session thread, including selected voices.
    pub fn generate_with(
        idea: SongIdea,
        brief: &str,
        request: &SongRequest,
        seed: u64,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(&str),
    ) -> Result<Self, String> {
        let mut session = Session::new(
            SessionOptions::headless()
                .with_balance(false)
                .with_plugin_paths(request.plugin_paths.clone()),
        )
        .map_err(|e| e.to_string())?;
        if !request.singers.is_empty()
            && let Some(dictionary) = &request.japanese_dictionary
        {
            session
                .set_japanese_dictionary(Some(dictionary))
                .map_err(|e| e.to_string())?;
        }
        let spec = request.specification(&idea, seed, &session)?;
        progress("compose");
        if cancel.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        let composition = compose(&spec);
        if cancel.load(Ordering::Acquire) {
            return Err("Cancelled".into());
        }
        let report = session
            .compose_without_balance(&composition)
            .map_err(|e| e.to_string())?;
        if !report.substituted.is_empty() {
            return Err(format!(
                "Unavailable built-in instruments: {:?}",
                report.substituted
            ));
        }
        if !report.unsung.is_empty() {
            return Err(format!("Could not fit lyrics: {:?}", report.unsung));
        }
        if !request.singers.is_empty() {
            session.arrange_candidate_singers(request, &spec)?;
            let tracks: Vec<_> = session
                .project
                .tracks
                .iter()
                .filter(|t| t.kind.as_singer().is_some())
                .map(|t| t.id)
                .collect();
            for track in tracks {
                if cancel.load(Ordering::Acquire) {
                    return Err("Cancelled".into());
                }
                progress("sing");
                let plan = session
                    .sing_plan(track, Some(seed))
                    .map_err(|e| e.to_string())?;
                let voice = session
                    .voice_model_at(&plan.voice)
                    .map_err(|e| e.to_string())?;
                let render = voice
                    .lock()
                    .map_err(|_| "Voice worker stopped")?
                    .sing_render_with(
                        &plan.frames,
                        &plan.score,
                        plan.speaker,
                        plan.seed,
                        |_, _| !cancel.load(Ordering::Acquire),
                    )
                    .map_err(|e| e.to_string())?;
                let landed = session
                    .begin_singer_landing(&plan)
                    .map_err(|e| e.to_string())?
                    .run(render, cancel)
                    .map_err(|e| e.to_string())?
                    .ok_or("Cancelled")?;
                session
                    .continue_singer_landing(landed)
                    .map_err(|e| e.to_string())?
                    .ok_or("Stale vocal render")?;
            }
        }
        session.collect_hosted_state().map_err(|e| e.to_string())?;
        let mut project = session.project().clone();
        project.creation_brief = Some(format!(
            "{}\nChosen direction: {} — {}",
            brief.chars().take(2400).collect::<String>(),
            idea.title,
            idea.description
        ));
        progress("render");
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
        let assets = if project.audio_sources.is_empty() {
            None
        } else {
            let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
            for source in project.audio_sources.values_mut() {
                let path = source
                    .path
                    .resolve(session.project_folder())
                    .ok_or("Missing vocal asset")?;
                let name = auris_io::copy_into(&path, dir.path()).map_err(|e| e.to_string())?;
                source.path = AssetPath::external(dir.path().join(name));
            }
            Some(dir)
        };
        Ok(Self {
            idea,
            project,
            audio: Arc::new(audio),
            _assets: assets,
        })
    }
}

impl Session {
    fn arrange_candidate_singers(
        &mut self,
        request: &SongRequest,
        spec: &SongSpec,
    ) -> Result<(), String> {
        let original = self
            .project
            .tracks
            .iter()
            .find(|t| t.kind.as_singer().is_some())
            .cloned()
            .ok_or("No vocal notes were generated")?;
        let clips = original.kind.note_clips().ok_or("No vocal clips")?.to_vec();
        self.project.remove_track(original.id);
        for (index, voice) in request.singers.iter().enumerate() {
            let name = voice
                .speaker
                .as_ref()
                .filter(|name| *name != &voice.name)
                .map_or_else(
                    || voice.name.clone(),
                    |speaker| format!("{} — {speaker}", voice.name),
                );
            let track = self.project.add_singer_track(&name, auris_synth::Vocal::ID);
            for (position, clip) in clips.iter().enumerate() {
                let together = request.singing == SongSinging::Harmony
                    || (request.singing == SongSinging::Auto && position + 1 == clips.len());
                if !together && position % request.singers.len() != index {
                    continue;
                }
                let id = self
                    .project
                    .add_midi_clip(track, &clip.name, clip.start, clip.length)
                    .ok_or("Could not create vocal clip")?;
                let target = self.project.midi_clip_mut(id).ok_or("Missing vocal clip")?;
                target.notes = clip.notes.clone();
                target.length_is_explicit = true;
                if together && index > 0 {
                    for note in &mut target.notes {
                        let degree = (-35..70)
                            .min_by_key(|&degree| {
                                (spec.key.midi(degree, 0) - i32::from(note.pitch)).abs()
                            })
                            .unwrap_or(0);
                        note.pitch =
                            spec.key.midi(degree + (index * 2) as i32, 0).clamp(0, 127) as u8;
                    }
                }
            }
            self.set_singer_voice(track, Some(&voice.path))
                .map_err(|e| e.to_string())?;
            if let Some(speaker) = &voice.speaker {
                self.set_singer_speaker(track, Some(speaker))
                    .map_err(|e| e.to_string())?;
            }
        }
        self.invalidate_graph();
        Ok(())
    }

    /// Adopt the exact document that produced an audition, as one undoable edit.
    pub fn adopt_song_candidate(&mut self, candidate: &SongCandidate) -> Result<(), SessionError> {
        if self.is_recording() {
            return Err(SessionError::RecordingInProgress);
        }
        candidate.project.validate_loop_expansion()?;
        // Prepare every asset before the undoable document swap. The candidate's
        // temporary directory may disappear immediately after this method returns.
        let mut project = candidate.project.clone();
        // Source banks also serve Undo. Independent candidates allocate IDs from one,
        // so collision-free IDs are essential before installing their decoded audio.
        let colliding: Vec<_> = project
            .audio_sources
            .keys()
            .copied()
            .filter(|id| self.waveforms.contains_key(id))
            .collect();
        for old_id in colliding {
            let source = project.audio_sources[&old_id].clone();
            let new_id = loop {
                let id = project.add_audio_source(
                    &source.name,
                    source.path.clone(),
                    source.frame_count,
                    source.sample_rate,
                    source.channel_count,
                );
                if !self.waveforms.contains_key(&id) {
                    break id;
                }
                project.audio_sources.remove(&id);
            };
            let mut replacement = source;
            replacement.id = new_id;
            project.audio_sources.insert(new_id, replacement);
            project.audio_sources.remove(&old_id);
            for track in &mut project.tracks {
                if let Some(singer) = track.kind.as_singer_mut()
                    && let Some(take) = &mut singer.take
                    && take.source == old_id
                {
                    take.source = new_id;
                }
            }
        }
        let folder = self
            .project_folder()
            .ok_or(SessionError::NoPath)?
            .join(auris_io::AUDIO_DIR);
        let mut audio = Vec::new();
        for source in project.audio_sources.values_mut() {
            let from = source.path.resolve(None).ok_or(SessionError::NoPath)?;
            let name = auris_io::copy_into(&from, &folder)?;
            let buffer = auris_io::import_audio_file(&folder.join(&name), project.sample_rate)?;
            source.path = AssetPath::inside(std::path::Path::new(auris_io::AUDIO_DIR).join(name));
            audio.push((source.id, Arc::new(buffer)));
        }
        let mut fonts = Vec::new();
        for font in project.soundfonts.values() {
            let path = font.path.resolve(None).ok_or(SessionError::NoPath)?;
            fonts.push((font.id, path.clone(), auris_io::load_soundfont(&path)?));
        }
        for track in &project.tracks {
            if let Some(instrument) = track.kind.as_instrument()
                && let Some(file) = &instrument.file
            {
                let path = file.resolve(None).ok_or(SessionError::NoPath)?;
                let source = if let Some(id) =
                    instrument.instrument_id.strip_prefix(auris_clap::ID_PREFIX)
                {
                    auris_compose::spec::PartSource::Clap {
                        path,
                        plugin_id: id.into(),
                    }
                } else if let Some(id) =
                    instrument.instrument_id.strip_prefix(auris_vst3::ID_PREFIX)
                {
                    auris_compose::spec::PartSource::Vst3 {
                        path,
                        class_id: id.into(),
                    }
                } else {
                    return Err(SessionError::SongSource(format!(
                        "Unknown hosted instrument: {}",
                        instrument.instrument_id
                    )));
                };
                self.validate_candidate_source(&source)?;
            }
        }
        self.stop_output_preview();
        self.collect_hosted_state()?;
        self.record(Edit::Compose);
        for (id, path, font) in fonts {
            self.cache_font(&path, Arc::clone(&font), false);
            self.fonts.insert(id, font);
        }
        for (id, buffer) in audio {
            self.install_source(id, buffer);
        }
        self.hosted.clear();
        self.vst3.clear();
        self.replace_project(project);
        self.dirty = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocal_request(folder: &std::path::Path, singing: SongSinging) -> SongRequest {
        let path = folder.join("test.voicevox.json");
        std::fs::write(&path, r#"{"format_version":1,"name":"Test voice","url":"http://127.0.0.1:1","styles":[{"name":"First","query_style_id":6000,"decode_style_id":3001},{"name":"Second","query_style_id":6000,"decode_style_id":3002}]}"#).unwrap();
        SongRequest {
            seconds: Some(90),
            lyrics: "さくらさいた\nはるがきた\n\nひかりとどく\nそらをみあげた".into(),
            singing,
            singers: ["First", "Second"]
                .into_iter()
                .map(|speaker| SongSinger {
                    name: speaker.into(),
                    path: path.clone(),
                    speaker: Some(speaker.into()),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn vocal_arrangements_preserve_lyrics_and_independent_singers() {
        let folder = tempfile::tempdir().unwrap();
        for mode in [
            SongSinging::Alternate,
            SongSinging::Harmony,
            SongSinging::Auto,
        ] {
            let mut session = Session::new(SessionOptions::headless()).unwrap();
            let request = vocal_request(folder.path(), mode);
            let spec = request.specification(&idea(), 42, &session).unwrap();
            let lyrics = spec
                .form
                .iter()
                .filter_map(|name| {
                    let text = &spec.sections[name].lyrics;
                    (!text.is_empty()).then_some(text.as_str())
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            assert_eq!(lyrics, request.lyrics);
            session.compose_without_balance(&compose(&spec)).unwrap();
            session.arrange_candidate_singers(&request, &spec).unwrap();
            let voices: Vec<_> = session
                .project
                .tracks
                .iter()
                .filter(|t| t.kind.as_singer().is_some())
                .collect();
            assert_eq!(voices.len(), 2);
            assert_eq!(voices[0].name, "First");
            assert_eq!(voices[1].name, "Second");
            let first = voices[0].kind.note_clips().unwrap();
            let second = voices[1].kind.note_clips().unwrap();
            assert!(!first.is_empty() && !second.is_empty());
            if mode == SongSinging::Alternate {
                assert!(
                    first
                        .iter()
                        .all(|a| second.iter().all(|b| a.start != b.start))
                );
            }
            if mode == SongSinging::Harmony {
                assert_eq!(first.len(), second.len());
                assert_eq!(first[0].notes[0].lyric, second[0].notes[0].lyric);
                assert_ne!(first[0].notes[0].pitch, second[0].notes[0].pitch);
            }
        }
    }

    #[test]
    fn duration_and_explicit_sources_are_preserved_without_erasing_other_parts() {
        let session = Session::new(SessionOptions::headless()).unwrap();
        for seconds in [30, 40, 110, 240, 300] {
            assert!(idea().specification(seconds, 1).is_ok());
        }
        let request = SongRequest {
            sources: vec![SongSourceChoice {
                role: "melody".into(),
                instrument: "auris.physical.violin".into(),
                source: None,
            }],
            ..Default::default()
        };
        let spec = request.specification(&idea(), 42, &session).unwrap();
        assert_eq!(
            spec.parts
                .iter()
                .find(|p| p.role == auris_compose::spec::Role::Melody)
                .unwrap()
                .instrument,
            "auris.physical.violin"
        );
        assert_eq!(
            spec.total_bars(),
            preset("chiptune").unwrap().spec().total_bars()
        );
    }

    #[test]
    fn vocal_assets_survive_candidate_drop_save_reload_and_undo_source_collisions() {
        let assets = tempfile::tempdir().unwrap();
        let file = assets.path().join("voice.wav");
        let buffer = Arc::new(AudioBuffer::from_planar(vec![vec![0.25; 4800]], 48_000.0).unwrap());
        auris_io::write_wav(&file, &buffer, &Default::default()).unwrap();
        let mut project = Project::new("Vocal", 48_000.0);
        let track = project.add_singer_track("Singer", auris_synth::Vocal::ID);
        let source =
            project.add_audio_source("Voice", AssetPath::external(&file), 4800, 48_000.0, 1);
        project
            .track_mut(track)
            .unwrap()
            .kind
            .as_singer_mut()
            .unwrap()
            .take = Some(auris_core::SingerTake {
            source,
            fingerprint: 0,
            seed: 0,
            backend_pitch: None,
        });
        let candidate = SongCandidate {
            idea: idea(),
            project,
            audio: buffer,
            _assets: Some(assets),
        };
        let mut target = Session::new(SessionOptions::headless()).unwrap();
        let old = Arc::new(AudioBuffer::from_planar(vec![vec![0.75; 4800]], 48_000.0).unwrap());
        target.install_source(source, old.clone());
        let before = target.project.clone();
        target.adopt_song_candidate(&candidate).unwrap();
        let new_source = target
            .project
            .track(track)
            .unwrap()
            .kind
            .as_singer()
            .unwrap()
            .take
            .as_ref()
            .unwrap()
            .source;
        assert_ne!(new_source, source);
        assert!(Arc::ptr_eq(target.bank.get(source).unwrap(), &old));
        drop(candidate);
        let stored = target.project.audio_sources[&new_source]
            .path
            .resolve(target.project_folder())
            .unwrap();
        assert!(stored.is_file());
        let adopted = target.project.clone();
        target.undo();
        assert_eq!(target.project, before);
        target.redo();
        assert_eq!(target.project, adopted);
        let saved = tempfile::tempdir().unwrap();
        target.save_as(&saved.path().join("Song.auris")).unwrap();
        let mut reopened = Session::new(SessionOptions::headless()).unwrap();
        assert!(
            reopened
                .open(&saved.path().join("Song/Song.auris"))
                .unwrap()
                .is_empty()
        );
        assert!((reopened.bank.get(new_source).unwrap().peak() - 0.25).abs() < 0.001);
    }

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
            _assets: None,
        };
        let mut target = Session::new(SessionOptions::headless()).unwrap();
        let before = target.project().clone();
        target.adopt_song_candidate(&candidate).unwrap();
        assert_eq!(target.project(), &candidate.project);
        target.undo();
        assert_eq!(target.project(), &before);
    }
}
