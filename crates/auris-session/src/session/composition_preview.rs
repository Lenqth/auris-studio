//! Read-only layout reports from the same draft used by composition.

use super::Session;
use crate::SessionError;
use crate::prelude::{Project, SignatureMap};
use auris_compose::{Composition, SongSpec, spec::PartSource};

/// One musical track and the authored parts that feed it.
#[derive(Debug, serde::Serialize)]
pub struct CompositionTrackPreview {
    /// Planned output track name.
    pub name: String,
    /// Authored part names; several drum writers may share one track.
    pub parts: Vec<String>,
    /// Native instrument choice, or fallback when a library selection takes precedence.
    pub instrument: String,
    /// Exact external selection, when requested; availability is checked at composition.
    pub source: Option<PartSource>,
    /// Number of planned clips.
    pub clips: usize,
}

/// The resulting timeline and part mapping, without loading instruments or writing files.
#[derive(Debug, serde::Serialize)]
pub struct CompositionPreview {
    /// Bars in the explicitly requested form.
    pub requested_bars: usize,
    /// Additional generated ending bars.
    pub ending_bars: usize,
    /// Full output length in bars.
    pub total_bars: usize,
    /// Musical tracks in output order, including any readable vocal lyrics.
    pub tracks: Vec<CompositionTrackPreview>,
    /// Auxiliary buses, separate from the musical track count.
    pub buses: Vec<String>,
    /// Declared parts that produced no track or notes.
    pub omitted_parts: Vec<String>,
    /// Lyrical sections that could not be read with this session's dictionary.
    pub unsung_sections: Vec<String>,
    /// False: source files and hosted instruments are checked when composing.
    pub source_availability_checked: bool,
}

impl Session {
    /// Reports a draft's actual length and merged track mapping without changing the session.
    /// Uses the same vocal preparation as composition but never loads external instruments.
    pub fn composition_preview(
        &self,
        composition: &Composition,
    ) -> Result<CompositionPreview, SessionError> {
        let spec = SongSpec::parse(&composition.spec)
            .map_err(|errors| SessionError::SongLyrics(format!("{errors:?}")))?;
        self.validate_song_lyrics(&spec)?;
        let requested_bars = spec.total_bars();
        let total_bars =
            (composition.length.raw() / composition.meter.ticks_per_bar().raw().max(1)) as usize;
        let mut tracks = composition
            .tracks
            .iter()
            .map(|track| CompositionTrackPreview {
                name: track.name.clone(),
                parts: if track.drum_parts.is_empty() {
                    vec![track.name.clone()]
                } else {
                    track
                        .drum_parts
                        .iter()
                        .map(|part| part.name.clone())
                        .collect()
                },
                instrument: track.instrument.clone(),
                source: track.source.clone(),
                clips: track.clips.len(),
            })
            .collect::<Vec<_>>();
        let omitted_parts = spec
            .parts
            .iter()
            .filter(|part| !tracks.iter().any(|track| track.parts.contains(&part.name)))
            .map(|part| part.name.clone())
            .collect();
        let mut vocal_project = Project::new(&composition.title, self.sample_rate());
        vocal_project.tempo_map = composition.tempo_map.clone();
        vocal_project.signatures = SignatureMap::constant(composition.meter);
        vocal_project.harmony = composition.harmony.clone();
        vocal_project.sections = composition.sections.clone();
        let (_, clips, unsung_sections) = self.write_spec_vocal(&mut vocal_project, composition);
        if clips > 0
            && let Some(vocal) = vocal_project.tracks.first()
        {
            tracks.push(CompositionTrackPreview {
                name: vocal.name.clone(),
                parts: Vec::new(),
                instrument: auris_synth::Vocal::ID.into(),
                source: None,
                clips,
            });
        }
        Ok(CompositionPreview {
            requested_bars,
            ending_bars: total_bars.saturating_sub(requested_bars),
            total_bars,
            tracks,
            buses: composition
                .buses
                .iter()
                .map(|bus| bus.name.clone())
                .collect(),
            omitted_parts,
            unsung_sections,
            source_availability_checked: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionOptions;

    #[test]
    fn preview_explains_ending_bars_and_merged_parts_without_loading_their_plugin() {
        let session = Session::new(SessionOptions::headless()).unwrap();
        let before = session.project().clone();
        for (ending, total) in [("held", 3), ("none", 2), ("loop", 2), ("fade", 2)] {
            let spec = SongSpec::parse(&format!("ending = '{ending}'\nform = 'verse'\n[section.verse]\nbars = 2\n[[part]]\nname = 'kick'\nsource = {{type='clap',path='Missing.clap',plugin_id='drums'}}\n[[part]]\nname = 'snare'\nsource = {{type='clap',path='Missing.clap',plugin_id='drums'}}")).unwrap();
            let report = session
                .composition_preview(&auris_compose::compose(&spec))
                .unwrap();
            assert_eq!(report.requested_bars, 2);
            assert_eq!(report.total_bars, total);
            assert_eq!(report.ending_bars, total - 2);
            assert_eq!(report.tracks.len(), 1);
            assert_eq!(report.tracks[0].parts, ["kick", "snare"]);
            assert!(!report.source_availability_checked);
        }
        assert_eq!(session.project(), &before);
        assert!(!session.is_dirty());
    }

    #[test]
    fn preview_and_installed_composition_agree_including_vocal_and_separate_drum_chains() {
        let mut session = Session::new(SessionOptions::headless().with_balance(false)).unwrap();
        let spec = SongSpec::parse("ending='held'\nform='verse'\n[section.verse]\nbars=2\nlyrics='あ'\n[[part]]\nname='kick'\neffects=[]\n[[part]]\nname='snare'\neffects=[{id='auris.fx.gain',params={gain_db=-3.0}}]").unwrap();
        let composition = auris_compose::compose(&spec);
        let preview = session.composition_preview(&composition).unwrap();
        assert_eq!(preview.tracks.len(), 3);
        assert_eq!(preview.tracks.last().unwrap().name, "Vocal");
        let installed = session.compose(&composition).unwrap();
        assert_eq!(installed.tracks, preview.tracks.len());
        assert_eq!(
            installed.length,
            composition.meter.ticks_per_bar() * preview.total_bars as i64
        );
    }
}
