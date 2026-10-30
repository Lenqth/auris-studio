//! Shared interpretation and ranking for desktop and agent sound discovery.

pub use auris_dsp::timbre::TimbreCharacter;

/// Optional fixed-scale targets; omitted dimensions do not affect ranking.
/// Values describe reference-note acoustics, not probabilities or measured musical quality.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimbreTargets {
    /// Dark (0) to bright (1).
    #[schemars(range(min = 0, max = 1))]
    pub brightness: Option<f64>,
    /// Slow (0) to immediate (1) attack.
    #[schemars(range(min = 0, max = 1))]
    pub attack: Option<f64>,
    /// Decaying (0) to sustained (1) held note.
    #[schemars(range(min = 0, max = 1))]
    pub sustain: Option<f64>,
    /// Short (0) to at least the captured 400 ms (1) release.
    #[schemars(range(min = 0, max = 1))]
    pub release: Option<f64>,
    /// Tonal (0) to noise-like (1).
    #[schemars(range(min = 0, max = 1))]
    pub noise: Option<f64>,
}

impl TimbreTargets {
    /// Brightness, attack speed, sustain, captured release, noise, in descriptor order.
    pub fn values(&self) -> [Option<f64>; 5] {
        [
            self.brightness,
            self.attack,
            self.sustain,
            self.release,
            self.noise,
        ]
    }

    /// Interpret supported Japanese/English impressions; explicit targets take precedence.
    /// Returns recognized terms so callers can distinguish them from uninterpreted prose.
    pub fn interpret(query: &str, explicit: &Self) -> Result<(Self, Vec<String>), String> {
        if query.len() > 1024 {
            return Err("Timbre query must be at most 1024 UTF-8 bytes".into());
        }
        let query = query.to_lowercase();
        if ["ない", "なく", "ではなく", "not ", "without ", "less "]
            .iter()
            .any(|word| query.contains(word))
        {
            return Err("Use explicit targets for negation/comparisons; the local impression dictionary does not interpret them".into());
        }
        let mut sums = [0.; 5];
        let mut counts = [0.; 5];
        let mut recognized = Vec::new();
        // Compound impressions are intentionally acoustic hypotheses that can be overridden.
        for (term_index, (words, values)) in [
            (
                "ふんわり|ふわふわ|柔らか|やわらか|soft|fluffy",
                [Some(0.25), Some(0.3), Some(0.8), None, Some(0.1)],
            ),
            (
                "ポコポコ|ぽこぽこ|ぽこん|plucky|plucked",
                [Some(0.3), Some(1.), Some(0.1), Some(0.1), Some(0.)],
            ),
            (
                "キラキラ|きらきら|煌びやか|sparkling|shimmering",
                [Some(0.85), None, None, Some(0.8), Some(0.)],
            ),
            (
                "ザラザラ|ざらざら|ざらつ|rough|gritty|noisy",
                [None, None, None, None, Some(0.8)],
            ),
            ("明る|bright", [Some(0.9), None, None, None, None]),
            (
                "暗い|暗め|dark|mellow|丸い|丸め",
                [Some(0.15), None, None, None, None],
            ),
            (
                "鋭い|鋭く|硬い|硬め|sharp|hard|パンチ|punchy",
                [Some(0.75), Some(1.), None, None, None],
            ),
            (
                "ゆっくり|緩やか|slow attack|slow-attack",
                [None, Some(0.15), None, None, None],
            ),
            (
                "持続|伸びる|sustained|sustaining",
                [None, None, Some(0.95), None, None],
            ),
            (
                "短い|短め|歯切れ|short|dry",
                [None, None, Some(0.15), Some(0.1), None],
            ),
            (
                "長い余韻|余韻が長|余韻は長|長め|long release|ringing",
                [None, None, None, Some(0.95), None],
            ),
            (
                "澄んだ|クリア|clear|pure|ピュア",
                [None, None, None, None, Some(0.)],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(word) = words.split('|').find(|word| {
                if word.is_ascii() {
                    query.match_indices(word).any(|(i, _)| {
                        !query[..i]
                            .chars()
                            .next_back()
                            .is_some_and(|c| c.is_ascii_alphabetic())
                            && !query[i + word.len()..]
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_ascii_alphabetic())
                    })
                } else {
                    query.contains(word)
                }
            }) {
                recognized.push(word.to_owned());
                for i in 0..5 {
                    if let Some(value) = values[i] {
                        if term_index >= 4 {
                            sums[i] = value;
                            counts[i] = 1.;
                        } else {
                            sums[i] += value;
                            counts[i] += 1.;
                        }
                    }
                }
            }
        }
        let explicit_values = explicit.values();
        let values: [Option<f64>; 5] = std::array::from_fn(|i| {
            explicit_values[i].or_else(|| (counts[i] > 0.).then(|| sums[i] / counts[i]))
        });
        if values
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err("Timbre targets must be finite values between 0 and 1".into());
        }
        if values.iter().all(Option::is_none) {
            return Err("No supported impression found; use text search for names or supply explicit timbre targets".into());
        }
        Ok((
            Self {
                brightness: values[0],
                attack: values[1],
                sustain: values[2],
                release: values[3],
                noise: values[4],
            },
            recognized,
        ))
    }

    /// Mean squared target error; smaller is closer. Never a probability.
    pub fn distance(&self, character: TimbreCharacter) -> f64 {
        let mut sum = 0.;
        let mut count = 0;
        for (target, actual) in self.values().into_iter().zip(character.values()) {
            if let Some(target) = target {
                sum += (target - actual).powi(2);
                count += 1;
            }
        }
        sum / count.max(1) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impressions_rank_acoustics_and_explicit_targets_override_dictionary() {
        let (soft, words) =
            TimbreTargets::interpret("ふんわりした音", &TimbreTargets::default()).unwrap();
        assert_eq!(words, ["ふんわり"]);
        let pad = TimbreCharacter {
            brightness: 0.25,
            attack: 0.3,
            sustain: 0.8,
            noise: 0.1,
            ..Default::default()
        };
        let pluck = TimbreCharacter {
            brightness: 0.8,
            attack: 1.,
            sustain: 0.,
            ..Default::default()
        };
        assert!(soft.distance(pad) < soft.distance(pluck));
        let (short, _) = TimbreTargets::interpret(
            "ふんわり、余韻は短め",
            &TimbreTargets {
                release: Some(0.05),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(short.release, Some(0.05));
    }

    #[test]
    fn unknown_negation_and_invalid_targets_are_explicit_errors() {
        for query in ["microsoft", "明るくない", "not bright", "宇宙"] {
            assert!(TimbreTargets::interpret(query, &TimbreTargets::default()).is_err());
        }
        assert!(
            TimbreTargets::interpret(
                "",
                &TimbreTargets {
                    noise: Some(f64::NAN),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
