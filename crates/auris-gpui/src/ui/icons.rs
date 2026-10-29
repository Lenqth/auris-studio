//! Shared icon vocabulary backed by Atelier's monochrome SVG assets.
//!
//! Atelier keeps familiar controls in one 24px SVG family; Auris's
//! two domain-specific marks (punch and notes) live beside the provider assets, use the same
//! stroke geometry, and reach Atelier's `Icon` as an `IconSource`.

use atelier::{Icon as AtelierIcon, IconName as AtelierIconName, IconSource};
use gpui::{Hsla, IntoElement, Pixels};

/// An icon the UI can draw.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Return the playhead to the start.
    ToStart,
    /// Start playback.
    Play,
    /// Pause playback.
    Pause,
    /// Audible preview output.
    Volume,
    /// Muted preview output.
    VolumeMuted,
    /// Stop playback and rewind.
    Stop,
    /// Start recording a take.
    Record,
    /// Toggle the cycle region.
    Loop,
    /// Toggle the punch region: a record disc between range brackets.
    Punch,
    /// Toggle the click: the wedge and its pendulum.
    Metronome,
    /// Add something.
    Plus,
    /// Move up in a list.
    ChevronUp,
    /// Move down in a list.
    ChevronDown,
    /// A branch of a tree that is shut. Points at what opening it would reveal.
    ChevronRight,
    /// Remove something.
    Cross,
    /// Additional commands: three horizontal dots.
    More,
    /// A menu item that is currently on.
    Check,
    /// The sound library: a folder with a raised tab.
    Library,
    /// The piano roll: notes lying at different places along a grid.
    Notes,
    /// The mixer: a bank of vertical faders.
    Faders,
    /// The inspector: an information badge for the selected item's properties.
    Inspector,
    /// The log: a folded page with lines of text.
    Log,
    /// An instrument: an enclosed piano keyboard.
    Keyboard,
    /// A singing voice: a microphone in its stand.
    Microphone,
    /// An effect: a knob with its pointer.
    Knob,
    /// A sound — a font's preset, or the shelf a set of them sits on.
    Wave,
    /// The agent: Atelier's sparkle, the mark every AI entry point shares.
    Agent,
    /// Step back one edit: an arrow turning back on itself.
    Undo,
    /// Speed: a lightning bolt, for the agent's fast mode.
    Bolt,
    /// Step forward one edit: Undo's arrow, mirrored.
    Redo,
}

/// An element that draws `icon` at `size`, centred in whatever box it is given.
pub fn icon(icon: Icon, size: Pixels, color: Hsla) -> impl IntoElement + use<> {
    AtelierIcon::new(icon.source()).size(size).color(color)
}

impl Icon {
    /// Maps Auris's domain vocabulary onto Atelier's shared 24px icon family.
    ///
    /// Atelier deliberately keeps the set small so icon weight, optical centring and state
    /// treatment stay consistent across the editor and agent surfaces. Domain-specific names
    /// therefore resolve to the nearest established symbol; the two marks the set has no
    /// counterpart for are Auris's own SVGs, drawn on the same grid and handed to Atelier as an
    /// [`IconSource`].
    pub fn source(self) -> IconSource {
        let name = match self {
            Self::Punch => return IconSource::svg("icons/auris-punch.svg"),
            Self::Notes => return IconSource::svg("icons/auris-notes.svg"),
            Self::ToStart => AtelierIconName::SkipBack,
            Self::Play => AtelierIconName::Play,
            Self::Pause => AtelierIconName::Pause,
            Self::Volume => AtelierIconName::Volume,
            Self::VolumeMuted => AtelierIconName::VolumeOff,
            Self::Stop => AtelierIconName::Stop,
            Self::Record => AtelierIconName::Record,
            Self::Loop => AtelierIconName::Loop,
            Self::Metronome => AtelierIconName::Metronome,
            Self::Plus => AtelierIconName::Plus,
            Self::ChevronUp => AtelierIconName::ChevronUp,
            Self::ChevronDown => AtelierIconName::ChevronDown,
            Self::ChevronRight => AtelierIconName::ChevronRight,
            Self::Cross => AtelierIconName::Close,
            Self::More => AtelierIconName::More,
            Self::Check => AtelierIconName::Check,
            Self::Library => AtelierIconName::Folder,
            Self::Faders => AtelierIconName::Sliders,
            Self::Knob => AtelierIconName::Knob,
            Self::Inspector => AtelierIconName::Info,
            Self::Log => AtelierIconName::File,
            Self::Keyboard => AtelierIconName::Keyboard,
            Self::Microphone => AtelierIconName::Mic,
            Self::Wave => AtelierIconName::Waveform,
            Self::Agent => AtelierIconName::Sparkles,
            Self::Undo => AtelierIconName::Undo,
            Self::Redo => AtelierIconName::Redo,
            Self::Bolt => AtelierIconName::Bolt,
        };
        name.into()
    }
}
