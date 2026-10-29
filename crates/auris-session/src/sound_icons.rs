//! Semantic library icons inferred conservatively from preset metadata.

use serde::{Deserialize, Serialize};

/// A bounded musical vocabulary shared by automatic labels and manual corrections.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum SoundIcon {
    /// An unknown or mixed sound family.
    #[default]
    Sound,
    /// Piano, electric piano or another struck keyboard.
    Piano,
    /// Organ or accordion.
    Organ,
    /// Guitar.
    Guitar,
    /// Bass instrument.
    Bass,
    /// Bowed strings or string ensemble.
    Strings,
    /// Brass instrument or section.
    Brass,
    /// Woodwind or reed instrument.
    Wind,
    /// Drums and unpitched percussion.
    Drum,
    /// Bells and pitched percussion.
    Bell,
    /// Voice or choir.
    Voice,
    /// Synthesizer or electronic lead.
    Synth,
    /// Sustained synthesizer pad.
    Pad,
    /// Harp and other plucked strings.
    Pluck,
}

impl SoundIcon {
    /// Every available preset, in picker order.
    pub const ALL: &[Self] = &[
        Self::Sound,
        Self::Piano,
        Self::Organ,
        Self::Guitar,
        Self::Bass,
        Self::Strings,
        Self::Brass,
        Self::Wind,
        Self::Drum,
        Self::Bell,
        Self::Voice,
        Self::Synth,
        Self::Pad,
        Self::Pluck,
    ];

    /// Stable identifier accepted by the agent, independent of interface language.
    pub fn id(self) -> &'static str {
        match self {
            Self::Sound => "sound",
            Self::Piano => "piano",
            Self::Organ => "organ",
            Self::Guitar => "guitar",
            Self::Bass => "bass",
            Self::Strings => "strings",
            Self::Brass => "brass",
            Self::Wind => "wind",
            Self::Drum => "drum",
            Self::Bell => "bell",
            Self::Voice => "voice",
            Self::Synth => "synth",
            Self::Pad => "pad",
            Self::Pluck => "pluck",
        }
    }

    /// Infer a family from the sound's own name, ignoring a map entry's library prefix.
    /// Unknown names remain generic. Bank/program numbers are never assumed to follow GM.
    pub fn infer(name: &str) -> Self {
        let name = name.rsplit_once(" / ").map_or(name, |(_, preset)| preset);
        let name = name.to_lowercase();
        let words: Vec<_> = name
            .split(|c: char| !c.is_alphabetic())
            .filter(|s| !s.is_empty())
            .collect();
        let has = |terms: &[&str]| terms.iter().any(|term| words.contains(term));
        // Explicit pad/lead roles precede their timbral modifiers (e.g. "choir pad").
        if has(&["pad", "pads"]) || name.contains("パッド") {
            Self::Pad
        } else if has(&["lead", "leads", "fx"]) {
            Self::Synth
        } else if has(&[
            "bassoon",
            "sax",
            "saxophone",
            "oboe",
            "clarinet",
            "flute",
            "piccolo",
            "recorder",
            "ocarina",
            "shakuhachi",
            "whistle",
            "bagpipe",
            "shanai",
            "harmonica",
        ]) || (has(&["english"]) && has(&["horn"]))
            || name.contains("blown bottle")
            || name.contains("フルート")
        {
            Self::Wind
        } else if has(&["bass"]) && has(&["drum"]) {
            Self::Drum
        } else if has(&["bass"]) && has(&["trombone", "trumpet", "tuba"]) {
            Self::Brass
        } else if has(&["bass", "contrabass"]) || name.contains("ベース") {
            Self::Bass
        } else if has(&["guitar", "guitars", "gtr", "nylon", "strat", "telecaster"])
            || name.contains("ギター")
        {
            Self::Guitar
        } else if has(&[
            "piano",
            "pianoforte",
            "ep",
            "rhodes",
            "wurli",
            "wurlitzer",
            "clav",
            "clavi",
            "clavinet",
            "harpsichord",
        ]) || name.contains("ピアノ")
        {
            Self::Piano
        } else if has(&["organ", "accordion", "hammond"]) || name.contains("オルガン") {
            Self::Organ
        } else if has(&["choir", "voice", "vocal", "vocals", "aahs", "oohs"])
            || name.contains("合唱")
        {
            Self::Voice
        } else if has(&["violin", "viola", "cello", "strings", "string", "fiddle"])
            || name.contains("ストリングス")
        {
            Self::Strings
        } else if has(&[
            "trumpet",
            "trombone",
            "tuba",
            "horn",
            "brass",
            "cornet",
            "flugelhorn",
        ]) || name.contains("トランペット")
        {
            Self::Brass
        } else if has(&[
            "harp", "koto", "sitar", "shamisen", "banjo", "dulcimer", "pluck", "plucked",
        ]) || name.contains("ハープ")
        {
            Self::Pluck
        } else if has(&[
            "bell",
            "bells",
            "celesta",
            "glockenspiel",
            "vibraphone",
            "vibes",
            "marimba",
            "xylophone",
            "kalimba",
        ]) || name.contains("music box")
            || name.contains("ベル")
        {
            Self::Bell
        } else if has(&[
            "drum",
            "drums",
            "kick",
            "snare",
            "cymbal",
            "percussion",
            "timpani",
            "tom",
            "taiko",
            "agogo",
            "woodblock",
            "hihat",
        ]) || name.contains("ドラム")
        {
            Self::Drum
        } else if has(&[
            "synth",
            "synthesizer",
            "square",
            "saw",
            "sawtooth",
            "pulse",
            "fm",
        ]) || name.contains("シンセ")
        {
            Self::Synth
        } else {
            Self::Sound
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_do_not_leak_library_brands_or_match_unrelated_substrings() {
        for (name, expected) in [
            ("Guitar Collection / Warm mystery [0:24]", SoundIcon::Sound),
            ("Piano Library / Nylon Gtr [0:1]", SoundIcon::Guitar),
            ("Bassoon", SoundIcon::Wind),
            ("English Horn", SoundIcon::Wind),
            ("English-Horn", SoundIcon::Wind),
            ("Bass Drum", SoundIcon::Drum),
            ("Bass Trombone", SoundIcon::Brass),
            ("Bass Guitar", SoundIcon::Bass),
            ("Synth Bass 2", SoundIcon::Bass),
            ("Choir Pad", SoundIcon::Pad),
            ("Lead 6 (voice)", SoundIcon::Synth),
            ("Glass", SoundIcon::Sound),
            ("Bassist's dream", SoundIcon::Sound),
            ("エレキギター", SoundIcon::Guitar),
            ("Grand Piano", SoundIcon::Piano),
            ("Soft Strings", SoundIcon::Strings),
        ] {
            assert_eq!(SoundIcon::infer(name), expected, "{name}");
        }
    }

    #[test]
    fn general_midi_named_families_have_consistent_icons() {
        use auris_compose::gm::PROGRAMS;
        // Explicit family labels form a deterministic metadata benchmark, not an audio test.
        for (range, expected) in [
            (0..8, SoundIcon::Piano),
            (16..22, SoundIcon::Organ),
            (23..24, SoundIcon::Organ),
            (24..32, SoundIcon::Guitar),
            (32..40, SoundIcon::Bass),
            (40..43, SoundIcon::Strings),
            (44..46, SoundIcon::Strings),
            (48..52, SoundIcon::Strings),
            (52..55, SoundIcon::Voice),
            (56..64, SoundIcon::Brass),
            (64..80, SoundIcon::Wind),
            (80..88, SoundIcon::Synth),
            (88..96, SoundIcon::Pad),
            (96..104, SoundIcon::Synth),
        ] {
            for name in &PROGRAMS[range] {
                assert_eq!(SoundIcon::infer(name), expected, "{name}");
            }
        }
    }
}
