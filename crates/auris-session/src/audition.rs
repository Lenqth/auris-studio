//! Immutable display data captured from the same score and audio as an audition.

use auris_core::{AudioBuffer, Project, time::Ticks};

/// A named score section located on an audition's clock.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditionSection {
    /// Authored name, preserved even when repeated.
    pub label: String,
    /// Occurrence of this name, counting from one.
    pub instance: usize,
    /// Inclusive start on the rendered audio clock.
    pub start_seconds: f64,
    /// Exclusive end, before any effect tail outside the score.
    pub end_seconds: f64,
}

/// One performed note on the immutable audition's clock.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditionNote {
    /// MIDI pitch, shared by every track in the overlaid score.
    pub pitch: u8,
    /// Inclusive start in seconds.
    pub start_seconds: f64,
    /// Exclusive end in seconds.
    pub end_seconds: f64,
}

/// A track's score and identity captured alongside the rendered audio.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditionTrack {
    /// Stable project track identity.
    pub id: u64,
    /// Authored track name.
    pub name: String,
    /// Packed project color, interpreted by the frontend's theme.
    pub color: u32,
    /// Performed notes, including clip repetitions and transformations.
    pub notes: Vec<AuditionNote>,
}

/// Bounded waveform and score structure for one immutable audio buffer.
#[derive(Clone, Debug, Default)]
pub struct AuditionTimeline {
    /// All note-bearing tracks from the same score revision as this audio.
    pub tracks: Vec<AuditionTrack>,
    /// Linear absolute sample peaks, combined across channels without phase cancellation.
    pub peaks: Vec<f32>,
    /// Named score sections in timeline order; gaps and effect tails remain unnamed.
    pub sections: Vec<AuditionSection>,
    /// Duration of the exact buffer, including any rendered tail.
    pub duration_seconds: f64,
}

impl AuditionTimeline {
    /// Measure a buffer off the UI and audio callback threads, retaining captured sections.
    pub fn from_audio(audio: &AudioBuffer, sections: Vec<AuditionSection>) -> Self {
        let frames = audio.frame_count();
        let count = frames.min(256);
        let peaks = (0..count)
            .map(|bin| {
                let start = bin * frames / count;
                let end = (bin + 1) * frames / count;
                audio
                    .iter_channels()
                    .flat_map(|channel| &channel[start..end])
                    .filter(|sample| sample.is_finite())
                    .map(|sample| sample.abs())
                    .fold(0.0_f32, f32::max)
                    .min(1.0)
            })
            .collect();
        let duration_seconds = audio.duration_seconds();
        let sections = sections
            .into_iter()
            .filter_map(|mut section| {
                section.start_seconds = section.start_seconds.clamp(0.0, duration_seconds);
                section.end_seconds = section.end_seconds.clamp(0.0, duration_seconds);
                (section.end_seconds > section.start_seconds).then_some(section)
            })
            .collect();
        Self {
            tracks: Vec::new(),
            peaks,
            sections,
            duration_seconds,
        }
    }

    pub(crate) fn from_project(project: &Project, audio: &AudioBuffer) -> Self {
        Self::from_audio(audio, sections_for_project(project))
            .with_tracks(tracks_for_project(project))
    }

    /// Attach a captured score, clipping note spans to the exact audio buffer.
    pub fn with_tracks(mut self, mut tracks: Vec<AuditionTrack>) -> Self {
        for track in &mut tracks {
            track.notes.retain_mut(|note| {
                if !note.start_seconds.is_finite()
                    || !note.end_seconds.is_finite()
                    || note.pitch > 127
                {
                    return false;
                }
                note.start_seconds = note.start_seconds.clamp(0.0, self.duration_seconds);
                note.end_seconds = note.end_seconds.clamp(0.0, self.duration_seconds);
                note.end_seconds > note.start_seconds
            });
        }
        tracks.retain(|track| !track.notes.is_empty());
        self.tracks = tracks;
        self
    }
}

fn tracks_for_project(project: &Project) -> Vec<AuditionTrack> {
    project
        .tracks
        .iter()
        .filter_map(|track| {
            let clips = track.kind.note_clips()?;
            let notes = clips
                .iter()
                .flat_map(|clip| {
                    clip.sounding_notes_with_meter(
                        project.tempo_map.bpm_at(clip.start),
                        project.signatures.clone(),
                    )
                    .map(move |note| AuditionNote {
                        pitch: note.pitch,
                        start_seconds: project
                            .tempo_map
                            .ticks_to_seconds(clip.start + note.start)
                            .0,
                        end_seconds: project
                            .tempo_map
                            .ticks_to_seconds(clip.start + note.start + note.length)
                            .0,
                    })
                })
                .collect::<Vec<_>>();
            (!notes.is_empty()).then(|| AuditionTrack {
                id: track.id.0,
                name: track.name.clone(),
                color: track.color.0,
                notes,
            })
        })
        .collect()
}

pub(crate) fn sections_for_project(project: &Project) -> Vec<AuditionSection> {
    project
        .sections
        .spans_in(Ticks::ZERO, project.end_tick())
        .into_iter()
        .map(|span| AuditionSection {
            label: span.label,
            instance: span.instance,
            start_seconds: project.tempo_map.ticks_to_seconds(span.start).0,
            end_seconds: project.tempo_map.ticks_to_seconds(span.end).0,
        })
        .collect()
}

impl crate::Session {
    /// Capture all piano-roll tracks before starting an asynchronous audition render.
    pub fn audition_tracks(&self) -> Vec<AuditionTrack> {
        tracks_for_project(self.project())
    }
    /// Capture score section times before starting an asynchronous audition render.
    pub fn audition_sections(&self) -> Vec<AuditionSection> {
        sections_for_project(self.project())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_track_notes_share_the_render_clock_across_tempo_changes_and_clip_loops() {
        use auris_core::Note;
        let mut session = crate::Session::new(crate::SessionOptions::headless()).unwrap();
        let lead = session.add_default_instrument_track("Lead").unwrap();
        let bass = session.add_default_instrument_track("Bass").unwrap();
        let beat = Ticks::QUARTER;
        session.set_tempo_at(Ticks::ZERO, 120.0);
        session.set_tempo_point(beat * 4, 60.0);
        let clip = session
            .add_midi_clip(lead, "Loop", beat * 2, beat * 2)
            .unwrap();
        session
            .replace_notes(clip, vec![Note::new(72, Ticks::ZERO, beat * 2)])
            .unwrap();
        session.set_clip_loop(clip, beat * 4).unwrap();
        let bass_clip = session
            .add_midi_clip(bass, "Bass", beat * 4, beat * 2)
            .unwrap();
        session
            .replace_notes(bass_clip, vec![Note::new(36, Ticks::ZERO, beat)])
            .unwrap();
        let audio = AudioBuffer::stereo(500, 100.0);
        let timeline = AuditionTimeline::from_project(session.project(), &audio);
        assert_eq!(
            timeline
                .tracks
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["Lead", "Bass"]
        );
        let notes = &timeline.tracks[0].notes;
        assert_eq!((notes[0].start_seconds, notes[0].end_seconds), (1.0, 2.0));
        assert_eq!((notes[1].start_seconds, notes[1].end_seconds), (2.0, 4.0));
        assert_eq!(
            (
                timeline.tracks[1].notes[0].start_seconds,
                timeline.tracks[1].notes[0].end_seconds
            ),
            (2.0, 3.0)
        );
        assert_eq!(
            timeline.tracks[0].color,
            session.project().tracks[0].color.0
        );
        assert_eq!(
            timeline.tracks[1].color,
            session.project().tracks[1].color.0
        );
        let shortened =
            AuditionTimeline::from_project(session.project(), &AudioBuffer::stereo(300, 100.0));
        assert_eq!(shortened.tracks[0].notes[1].end_seconds, 3.0);
    }

    #[test]
    fn waveform_keeps_transients_and_stereo_peaks_at_their_time() {
        let mut audio = AudioBuffer::stereo(1025, 100.0);
        audio.channel_mut(0)[3] = -0.25;
        audio.channel_mut(0)[1024] = -0.9;
        audio.channel_mut(1)[1024] = 0.9;
        let timeline = AuditionTimeline::from_audio(&audio, vec![]);
        assert_eq!(timeline.peaks.len(), 256);
        assert_eq!(timeline.peaks[0], 0.25);
        assert!(timeline.peaks[1..255].iter().all(|peak| *peak == 0.0));
        assert_eq!(timeline.peaks[255], 0.9);
        assert_eq!(timeline.duration_seconds, 10.25);
    }

    #[test]
    fn empty_short_and_nonfinite_audio_produce_bounded_finite_peaks() {
        for samples in [vec![], vec![0.0], vec![f32::NAN, f32::INFINITY, -2.0]] {
            let audio = AudioBuffer::from_planar(vec![samples], 48_000.0).unwrap();
            let timeline = AuditionTimeline::from_audio(&audio, vec![]);
            assert_eq!(timeline.peaks.len(), audio.frame_count());
            assert!(
                timeline
                    .peaks
                    .iter()
                    .all(|p| p.is_finite() && (0.0..=1.0).contains(p))
            );
        }
    }

    #[test]
    fn sections_follow_tempo_changes_repeats_gaps_and_leave_the_tail_unnamed() {
        let mut session = crate::Session::new(crate::SessionOptions::headless()).unwrap();
        let track = session.add_default_instrument_track("Lead").unwrap();
        let beat = Ticks::QUARTER;
        session
            .add_midi_clip(track, "Score", Ticks::ZERO, beat * 16)
            .unwrap();
        session.set_tempo_at(Ticks::ZERO, 120.0);
        session.set_tempo_point(beat * 4, 60.0);
        session.set_section(Ticks::ZERO, Some("Intro".into()));
        session.set_section(beat * 4, Some("Chorus".into()));
        session.set_section(beat * 8, None);
        session.set_section(beat * 12, Some("Chorus".into()));
        let audio = AudioBuffer::stereo(1600, 100.0);
        let timeline = AuditionTimeline::from_audio(&audio, session.audition_sections());
        let sections: Vec<_> = timeline
            .sections
            .iter()
            .map(|s| (s.label.as_str(), s.instance, s.start_seconds, s.end_seconds))
            .collect();
        assert_eq!(
            sections,
            [
                ("Intro", 1, 0.0, 2.0),
                ("Chorus", 1, 2.0, 6.0),
                ("Chorus", 2, 10.0, 14.0)
            ]
        );
        assert_eq!(timeline.duration_seconds, 16.0);
    }
}
