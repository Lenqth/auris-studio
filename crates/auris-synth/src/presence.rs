//! Gentle upper-midrange voicing with protection for high-register fundamentals.

use auris_dsp::{Biquad, BiquadCoefficients};

/// Fades the correction out as the fundamental rises from 1 to 2 kHz.
pub(crate) fn correction_amount(fundamental: f32) -> f32 {
    ((2_000.0 - fundamental) / 1_000.0).clamp(0.0, 1.0)
}

/// A prepared upper-midrange cut with a smoothed, pitch-dependent wet amount.
#[derive(Clone, Debug)]
pub(crate) struct Presence {
    filter: Biquad,
    enabled: bool,
    mix: f32,
    smoothing: f32,
}

impl Presence {
    /// Designs the fixed peaking section off the audio callback.
    pub(crate) fn new(rate: f32, gain_db: f32) -> Self {
        Self {
            filter: Biquad::new(BiquadCoefficients::peaking(
                f64::from(rate),
                3_200.0_f32.min(rate * 0.4),
                0.9,
                gain_db,
            )),
            enabled: gain_db != 0.0,
            mix: 0.0,
            smoothing: 1.0 - (-1.0 / (rate * 0.003)).exp(),
        }
    }

    /// Clears the section and its smoothed correction amount.
    pub(crate) fn reset(&mut self) {
        self.filter.reset();
        self.mix = 0.0;
    }

    /// Shapes one sample, retaining high fundamentals and amount-zero patches.
    pub(crate) fn process(&mut self, input: f32, fundamental: f32, amount: f32) -> f32 {
        if !self.enabled {
            return input;
        }
        // A high note's fundamental is not excess brightness. Fade the correction
        // between 1 and 2 kHz, and smooth bends, glides and timbre changes over 3 ms.
        let protection = correction_amount(fundamental);
        let target = protection * amount.clamp(0.0, 1.0);
        self.mix += (target - self.mix) * self.smoothing;
        let filtered = self.filter.process_sample(input);
        input + self.mix * (filtered - input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{goertzel, rms};

    #[test]
    fn upper_midrange_cut_keeps_low_fundamentals_and_upper_sizzle() {
        for rate in [8_000.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            for (frequency, low, high) in [
                (440.0, 0.97, 1.01),
                (3_200.0, 0.69, 0.73),
                (8_000.0, 0.90, 1.01),
            ] {
                if frequency >= rate * 0.5 {
                    continue;
                }
                let mut presence = Presence::new(rate, -3.0);
                let audio: Vec<_> = (0..rate as usize)
                    .map(|index| {
                        let input = (std::f32::consts::TAU * frequency * index as f32 / rate).sin();
                        presence.process(input, 440.0, 1.0)
                    })
                    .collect();
                let gain = goertzel(
                    &audio[(rate * 0.1) as usize..],
                    f64::from(rate),
                    f64::from(frequency),
                );
                assert!((low..high).contains(&gain), "{rate}/{frequency}: {gain}");
            }
        }
    }

    #[test]
    fn fundamentals_in_the_target_band_and_disabled_voicing_are_exactly_unchanged() {
        for (gain, fundamental, amount) in
            [(-3.0, 2_400.0, 1.0), (-3.0, 440.0, 0.0), (0.0, 440.0, 1.0)]
        {
            let mut presence = Presence::new(48_000.0, gain);
            for index in 0..48_000 {
                let input = (std::f32::consts::TAU * fundamental * index as f32 / 48_000.0).sin();
                assert_eq!(presence.process(input, fundamental, amount), input);
            }
        }
    }

    #[test]
    fn reset_clears_filter_memory_and_processing_allocates_nothing() {
        let mut presence = Presence::new(48_000.0, -3.0);
        let mut audio = [0.0; 480];
        let allocations = crate::test_support::count_allocations(|| {
            for sample in &mut audio {
                *sample = presence.process(0.5, 440.0, 1.0);
            }
            presence.reset();
            for sample in &mut audio {
                *sample = presence.process(0.0, 440.0, 1.0);
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(rms(&audio), 0.0);
    }
}
