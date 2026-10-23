//! Bounded, deterministic harmony proposals grounded in an authored melody.
//!
//! These are explainable heuristics, not aesthetic scores. Colour needs melodic evidence;
//! every extra tone or chord change has a cost. A plain alternative is retained even when
//! colour fits better. No existing composer or stored score is changed by this module.

use crate::Mood;
use crate::theory::{
    chart::{Chart, ChartOrigin},
    chord::{Chord, Quality},
    key::Key,
    numeral::Numeral,
    scale::ScaleId,
};
use auris_core::Note;
use auris_core::time::{Ticks, TimeSignature};

/// The phrase's intended destination, independent of its harmonic colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// Prefer a tonic arrival when the melody supports one.
    Settled,
    /// Leave room for the following phrase, without forcing a tonic cadence.
    Open,
}

/// Conditions for a short, constant-key, constant-meter phrase.
#[derive(Debug, Clone, Copy)]
pub struct Context {
    /// Declared tonal centre; mood never changes it.
    pub key: Key,
    /// Meter of the authored notes.
    pub signature: TimeSignature,
    /// Phrase length, 1..16 bars and at most 128 quarter notes.
    pub bars: usize,
    /// Finite controls within 0..1. Tension adjusts the cost of supported colour, never a quota.
    pub mood: Mood,
    /// Desired phrase ending.
    pub ending: Ending,
}

/// Evidence for one proposed bar, with no claim of perceptual certainty.
#[derive(Debug, Clone)]
pub struct Evidence {
    /// One-based bar within the requested phrase.
    pub bar: usize,
    /// Chord-tone share of duration/metric-weighted melody in this bar, 0..1.
    pub melody_fit: f64,
    /// Share of sustained/accented melody outside the proposed chords, 0..1.
    pub salient_non_chord: f64,
    /// Concrete reasons for this choice.
    pub reasons: Vec<&'static str>,
}

/// A complete phrase alternative. The first returned suggestion is the recommendation.
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// Literal chart; its explicit qualities will not be recoloured when applied.
    pub chart: Chart,
    /// Comparable heuristic utility per bar, not a probability or musical quality rating.
    pub score: f64,
    /// Whether this is the strongest plain major/minor-triad alternative.
    pub plain: bool,
    /// Evidence and unresolved melodic tensions, one entry per bar.
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Default)]
struct Profile {
    weights: [f64; 12],
    salient: [f64; 12],
}

impl Profile {
    fn of(notes: &[Note], start: Ticks, end: Ticks, beat: Ticks) -> Self {
        let mut profile = Self::default();
        for note in notes {
            let overlap = (note.end().min(end) - note.start.max(start)).raw();
            if overlap <= 0 {
                continue;
            }
            let pc = (note.pitch % 12) as usize;
            let accented = note.start >= start && (note.start - start).raw() % beat.raw() == 0;
            let duration = overlap as f64 / beat.raw() as f64;
            let weight = duration + if accented { duration.min(0.5) } else { 0.0 };
            profile.weights[pc] += weight;
            // Short passing notes, even on a beat, must not purchase an extension on their own.
            if overlap >= beat.raw() || (accented && overlap >= beat.raw() / 2) {
                profile.salient[pc] += weight;
            }
        }
        profile
    }

    fn total(&self) -> f64 {
        self.weights.iter().sum()
    }

    fn share(&self, chord: Chord, salient: bool) -> f64 {
        let weights = if salient {
            &self.salient
        } else {
            &self.weights
        };
        let total: f64 = weights.iter().sum();
        if total == 0.0 {
            return 1.0;
        }
        weights
            .iter()
            .enumerate()
            .filter(|(pc, _)| chord.contains_midi(*pc as i32))
            .map(|(_, w)| w)
            .sum::<f64>()
            / total
    }

    fn supports(&self, pc: i32) -> bool {
        self.salient[pc.rem_euclid(12) as usize] / self.total().max(f64::EPSILON) >= 0.12
    }

    fn resembles(&self, other: &Self) -> bool {
        let a = self.total();
        let b = other.total();
        a > 0.0
            && b > 0.0
            && self
                .weights
                .iter()
                .zip(other.weights)
                .map(|(x, y)| (x / a - y / b).abs())
                .sum::<f64>()
                < 0.12
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Choice(Vec<Numeral>);

impl Choice {
    fn first(&self, key: Key) -> Chord {
        self.0[0].chord_in(key)
    }
    fn last(&self, key: Key) -> Chord {
        self.0[self.0.len() - 1].chord_in(key)
    }
    fn plain(&self, key: Key) -> bool {
        self.0.len() == 1 && matches!(self.first(key).quality, Quality::Major | Quality::Minor)
    }
}

#[derive(Clone)]
struct Path {
    choices: Vec<Choice>,
    score: f64,
}

fn diminished(quality: Quality) -> bool {
    matches!(
        quality,
        Quality::Diminished | Quality::Diminished7 | Quality::HalfDiminished7
    )
}

fn unresolved(chord: Chord) -> bool {
    diminished(chord.quality) || chord.quality == Quality::Sus4
}

fn transition(from: Chord, to: Chord) -> Option<f64> {
    let motion = (to.root.semitones() - from.root.semitones()).rem_euclid(12);
    if from.quality == Quality::Sus4
        && (motion != 0
            || !matches!(
                to.quality,
                Quality::Major
                    | Quality::Minor
                    | Quality::Major7
                    | Quality::Minor7
                    | Quality::Dominant7
            ))
    {
        return None;
    }
    if diminished(from.quality)
        && (diminished(to.quality) || to.quality == Quality::Sus4 || !matches!(motion, 1 | 2 | 5))
    {
        return None;
    }
    let distance = |a: Chord, b: Chord| {
        a.quality
            .intervals()
            .iter()
            .map(|interval| {
                let pitch = a.root.semitones() + interval;
                b.quality
                    .intervals()
                    .iter()
                    .map(|other| {
                        let d = (pitch - b.root.semitones() - other).rem_euclid(12);
                        d.min(12 - d) as f64
                    })
                    .fold(12.0, f64::min)
            })
            .sum::<f64>()
            / a.quality.intervals().len() as f64
    };
    let leading = (distance(from, to) + distance(to, from)) * 0.5;
    if diminished(from.quality) && leading > 2.0 {
        return None;
    }
    Some(
        -0.10 * leading
            + if motion == 5 {
                0.12
            } else if from == to {
                0.08
            } else {
                0.0
            },
    )
}

fn roots(key: Key) -> Vec<Numeral> {
    let mut roots: Vec<_> = (1..=7)
        .map(|degree| {
            let numeral = Numeral::new(degree, false).as_diatonic(key);
            numeral.with_quality(numeral.chord_in(key).quality)
        })
        .collect();
    if matches!(
        key.scale,
        ScaleId::Minor | ScaleId::HarmonicMinor | ScaleId::MelodicMinor
    ) {
        roots.push(Numeral::new(5, false).with_quality(Quality::Major));
        let mut leading = Numeral::new(7, true).with_quality(Quality::Diminished);
        leading.accidental = (11 - key.semitone(6)).rem_euclid(12);
        roots.push(leading);
    }
    roots
}

fn choices(context: Context, whole: &Profile, halves: &[Profile; 2], plain: bool) -> Vec<Choice> {
    let mut choices = Vec::new();
    for root in roots(context.key) {
        let chord = root.chord_in(context.key);
        if matches!(chord.quality, Quality::Major | Quality::Minor) {
            choices.push(Choice(vec![root]));
        } else if !plain && diminished(chord.quality) {
            let supported = chord
                .quality
                .intervals()
                .iter()
                .filter(|interval| whole.supports(chord.root.semitones() + *interval))
                .count();
            if supported >= 2 {
                choices.push(Choice(vec![root]));
            }
        }
        if plain || whole.total() == 0.0 {
            continue;
        }
        let interval = (context
            .key
            .class(i32::from(root.degree) - 1 + 6)
            .semitones()
            - chord.root.semitones())
        .rem_euclid(12);
        let seventh = match (chord.quality, interval) {
            (Quality::Major, 11) => Some(Quality::Major7),
            (Quality::Major, 10) => Some(Quality::Dominant7),
            (Quality::Minor, 10) => Some(Quality::Minor7),
            (Quality::Diminished, 10) => Some(Quality::HalfDiminished7),
            (Quality::Diminished, 9) => Some(Quality::Diminished7),
            _ => None,
        };
        if let Some(seventh) = seventh {
            let coloured = root.with_quality(seventh);
            let extended = coloured.chord_in(context.key);
            // Diatonic colour and the tonal minor dominant are enough for this vocabulary.
            let in_key = context
                .key
                .scale
                .contains(context.key.tonic, chord.root.transposed(interval));
            if in_key
                && whole.supports(chord.root.semitones() + interval)
                && (!diminished(chord.quality)
                    || chord
                        .quality
                        .intervals()
                        .iter()
                        .filter(|interval| whole.supports(chord.root.semitones() + *interval))
                        .count()
                        >= 2)
                && whole.share(extended, false) > whole.share(chord, false) + 0.08
            {
                choices.push(Choice(vec![coloured]));
            }
        }
        if !matches!(chord.quality, Quality::Major | Quality::Minor) {
            continue;
        }
        let suspended = root.with_quality(Quality::Sus4);
        let third = chord.root.semitones() + chord.quality.intervals()[1];
        let fourth = chord.root.semitones() + 5;
        if !context
            .key
            .scale
            .contains(context.key.tonic, chord.root.transposed(5))
        {
            continue;
        }
        if whole.supports(fourth) && !whole.supports(third) {
            choices.push(Choice(vec![suspended]));
        }
        if context.signature.numerator.is_multiple_of(2)
            && halves[0].supports(fourth)
            && !halves[0].supports(third)
            && halves[1].supports(third)
            && !halves[1].supports(fourth)
        {
            choices.push(Choice(vec![suspended, root]));
        }
    }
    let mut seen = std::collections::HashSet::new();
    choices.retain(|choice| seen.insert(choice.clone()));
    choices
}

fn local(
    choice: &Choice,
    whole: &Profile,
    halves: &[Profile; 2],
    context: Context,
) -> (f64, f64, f64) {
    let mut score = 0.0;
    let mut fit = 0.0;
    let mut salient_matched = 0.0;
    let mut salient_total = 0.0;
    for (index, numeral) in choice.0.iter().enumerate() {
        let profile = if choice.0.len() == 1 {
            whole
        } else {
            &halves[index]
        };
        let chord = numeral.chord_in(context.key);
        let weight = profile.total() / whole.total().max(f64::EPSILON);
        fit += profile.share(chord, false) * weight;
        let salient: f64 = profile.salient.iter().sum();
        salient_matched += profile.share(chord, true) * salient;
        salient_total += salient;
        score += (3.0 * profile.share(chord, false)
            + profile.share(chord, true)
            + 0.15 * profile.weights[chord.root.semitones() as usize]
                / profile.total().max(f64::EPSILON))
            * weight;
        let colour_cost = if diminished(chord.quality) {
            0.42
        } else if chord.quality == Quality::Sus4 {
            0.20
        } else if chord.quality.has_seventh() {
            0.18
        } else {
            0.0
        };
        // Always a cost, even for the most tense mood. Better melodic fit must pay for colour.
        score -= colour_cost * (1.5 - f64::from(context.mood.tension));
    }
    if choice.0.len() > 1 {
        score -= 0.16 * (1.5 - f64::from(context.mood.energy));
    }
    let non_chord = if salient_total == 0.0 {
        0.0
    } else {
        1.0 - salient_matched / salient_total
    };
    if whole.total() == 0.0 {
        (0.0, 1.0, 0.0)
    } else {
        (score, fit, non_chord)
    }
}

fn search(context: Context, profiles: &[(Profile, [Profile; 2])], plain: bool) -> Vec<Path> {
    let mut paths = vec![Path {
        choices: Vec::new(),
        score: 0.0,
    }];
    for (bar, (whole, halves)) in profiles.iter().enumerate() {
        let options = choices(context, whole, halves, plain);
        let mut next = Vec::new();
        for path in &paths {
            let held;
            let options = if whole.total() == 0.0
                && let Some(previous) = path.choices.last()
            {
                held = vec![Choice(vec![*previous.0.last().expect("nonempty choice")])];
                &held
            } else {
                &options
            };
            for choice in options {
                let first = choice.first(context.key);
                let last = choice.last(context.key);
                if bar + 1 == context.bars && unresolved(last) {
                    continue;
                }
                if whole.total() == 0.0 {
                    if let Some(previous) = path.choices.last() {
                        if choice.0.len() != 1 || first != previous.last(context.key) {
                            continue;
                        }
                    } else if first.root != context.key.tonic || !choice.plain(context.key) {
                        continue;
                    }
                }
                let mut score = path.score + local(choice, whole, halves, context).0;
                if let Some(previous) = path.choices.last() {
                    let Some(cost) = transition(previous.last(context.key), first) else {
                        continue;
                    };
                    score += cost;
                }
                if context.ending == Ending::Settled && bar + 1 == context.bars {
                    score += if last.root == context.key.tonic {
                        0.45
                    } else {
                        -0.15
                    };
                }
                // Keep a recognizable repeated motif harmonically related; a phrase ending can vary.
                if bar + 1 < context.bars
                    && let Some(previous) =
                        (0..bar).rev().find(|i| whole.resembles(&profiles[*i].0))
                    && path.choices[previous] != *choice
                {
                    score -= 0.22;
                }
                let mut candidate = path.clone();
                candidate.choices.push(choice.clone());
                candidate.score = score;
                next.push(candidate);
            }
        }
        next.sort_by(|a, b| b.score.total_cmp(&a.score));
        next.truncate(24);
        paths = next;
    }
    paths
}

/// Propose up to three distinct charts from clip-relative authored notes.
///
/// Sustained/accented notes support colour; suspensions and diminished chords require
/// explicit resolution. Short passing notes cannot add extensions by themselves. Every
/// extra tone/change has a cost; no target percentage of complex chords exists.
/// The best plain alternative is always included. Unsupported scale families, empty input,
/// nonfinite controls or inputs beyond the bounded phrase budget return an error.
pub fn suggest(notes: &[Note], context: Context) -> Result<Vec<Suggestion>, &'static str> {
    if !TimeSignature::NUMERATORS.contains(&context.signature.numerator)
        || !TimeSignature::DENOMINATORS.contains(&context.signature.denominator)
        || !(1..=16).contains(&context.bars)
        || context.signature.ticks_per_bar() <= Ticks::ZERO
        || context.signature.ticks_per_bar() * context.bars as i64 > Ticks::QUARTER * 128
        || notes.len() > 4096
    {
        return Err("Use 1..16 bars, at most 128 quarter notes and 4096 melody notes");
    }
    if notes.iter().any(|n| {
        n.pitch > 127
            || n.start < Ticks::ZERO
            || n.length <= Ticks::ZERO
            || n.start.raw().checked_add(n.length.raw()).is_none()
    }) {
        return Err(
            "Melody notes require valid MIDI pitches and nonnegative, nonoverflowing timing",
        );
    }
    if !matches!(
        context.key.scale,
        ScaleId::Major
            | ScaleId::Minor
            | ScaleId::HarmonicMinor
            | ScaleId::MelodicMinor
            | ScaleId::Dorian
            | ScaleId::Lydian
            | ScaleId::Mixolydian
            | ScaleId::Phrygian
    ) {
        return Err(
            "Harmony suggestions require a seven-note major, minor or supported modal scale",
        );
    }
    if [
        context.mood.brightness,
        context.mood.energy,
        context.mood.tension,
        context.mood.syncopation,
    ]
    .iter()
    .any(|value| !(0.0..=1.0).contains(value))
    {
        return Err("Mood controls must be finite within 0..1");
    }
    let bar_ticks = context.signature.ticks_per_bar();
    let profiles: Vec<_> = (0..context.bars)
        .map(|bar| {
            let start = bar_ticks * bar as i64;
            let mid = start + Ticks(bar_ticks.raw() / 2);
            let end = start + bar_ticks;
            (
                Profile::of(notes, start, end, context.signature.beat_ticks()),
                [
                    Profile::of(notes, start, mid, context.signature.beat_ticks()),
                    Profile::of(notes, mid, end, context.signature.beat_ticks()),
                ],
            )
        })
        .collect();
    if profiles.iter().all(|(p, _)| p.total() == 0.0) {
        return Err("The requested phrase contains no melody notes");
    }
    let plain = search(context, &profiles, true)
        .into_iter()
        .next()
        .ok_or("No plain harmony fits this phrase")?;
    let mut paths = search(context, &profiles, false);
    // A small theoretical gain is not enough to recommend a more elaborate progression.
    paths.retain(|p| {
        p.choices != plain.choices && p.score > plain.score + 0.08 * context.bars as f64
    });
    // Preserve a simple comparison without padding the report with duplicate charts.
    paths.truncate(2);
    paths.push(plain.clone());
    paths.sort_by(|a, b| b.score.total_cmp(&a.score));
    Ok(paths.into_iter().map(|path| {
        let evidence = path.choices.iter().enumerate().map(|(bar, choice)| {
            let (_, melody_fit, salient_non_chord) = local(choice, &profiles[bar].0, &profiles[bar].1, context);
            let mut reasons = Vec::new();
            if profiles[bar].0.total() == 0.0 { reasons.push("Hold the previous harmony through the rest"); }
            else if choice.plain(context.key) { reasons.push("A plain triad fits without adding unsupported colour"); }
            for numeral in choice.0.iter().filter(|_| profiles[bar].0.total() > 0.0) {
                let chord = numeral.chord_in(context.key);
                if chord.quality.has_seventh() { reasons.push("The melody sustains or accents this chord's seventh"); }
                if chord.quality == Quality::Sus4 { reasons.push("The melody supports the fourth; the proposed progression resolves it to a third"); }
                if diminished(chord.quality) { reasons.push("The melody supports diminished harmony leading into the next chord with small pitch-class motion"); }
            }
            if salient_non_chord > 0.15 { reasons.push("Some sustained or accented melody remains outside the chord; audition this tension"); }
            Evidence { bar: bar+1, melody_fit, salient_non_chord, reasons }
        }).collect();
        Suggestion { chart: Chart::new(path.choices.iter().map(|choice| choice.0.clone()).collect(), ChartOrigin::Given), score: path.score/context.bars as f64, plain: path.choices == plain.choices, evidence }
    }).collect())
}

#[cfg(test)]
mod tests;
