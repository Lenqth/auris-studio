//! Four-times oversampled peak measurement shared by the limiter and offline analysis.
//!
//! Each fractional phase uses a 16-tap, unity-normalised Lanczos-windowed sinc. The taps
//! reconstruct samples from seven frames before the left endpoint through eight frames after
//! it. Keeping the coefficients here makes the limiter and the exported meter judge the same
//! waveform; the limiter stores its support in a fixed-size per-channel array.

/// Samples needed to interpolate one interval.
pub const TAPS: usize = 16;

/// Input frames the limiter must wait before an interval can be evaluated.
pub const DETECTION_DELAY: usize = 8;

/// Polyphase coefficients for offsets -7 through +8 at quarter-sample intervals.
///
/// Each tap is `sinc(offset - phase) * sinc((offset - phase) / 8)`, divided by the
/// phase's tap sum so a constant signal remains constant. Values were calculated in f64 and
/// rounded to f32; the same coefficients are used in realtime and offline.
#[allow(clippy::excessive_precision)] // Keep the generated f64 values visible for auditing.
const PHASES: [[f32; TAPS]; 3] = [
    [
        -3.1648431790e-3,
        9.3068483702e-3,
        -1.8336451901e-2,
        3.1574024258e-2,
        -5.1918511773e-2,
        8.7503299007e-2,
        -1.7289030564e-1,
        8.9872351811e-1,
        2.9573701261e-1,
        -1.1870980064e-1,
        6.6829547837e-2,
        -4.0555080048e-2,
        2.4305342076e-2,
        -1.3398425746e-2,
        5.9290228272e-3,
        -9.3519616869e-4,
    ],
    [
        -2.8104077629e-3,
        1.0655366516e-2,
        -2.2272919136e-2,
        3.9246872043e-2,
        -6.4877482357e-2,
        1.0780092862e-1,
        -2.0008410458e-1,
        6.3234174666e-1,
        6.3234174666e-1,
        -2.0008410458e-1,
        1.0780092862e-1,
        -6.4877482357e-2,
        3.9246872043e-2,
        -2.2272919136e-2,
        1.0655366516e-2,
        -2.8104077629e-3,
    ],
    [
        -9.3519616869e-4,
        5.9290228272e-3,
        -1.3398425746e-2,
        2.4305342076e-2,
        -4.0555080048e-2,
        6.6829547837e-2,
        -1.1870980064e-1,
        2.9573701261e-1,
        8.9872351811e-1,
        -1.7289030564e-1,
        8.7503299007e-2,
        -5.1918511773e-2,
        3.1574024258e-2,
        -1.8336451901e-2,
        9.3068483702e-3,
        -3.1648431790e-3,
    ],
];

/// Largest interpolated magnitude within the interval centred on `window[7]` and `window[8]`.
///
/// The samples at the endpoints are measured separately by callers. This function has fixed
/// stack use and performs no allocation, so it is suitable for an audio callback.
#[inline]
pub fn interval_peak(window: &[f32; TAPS]) -> f32 {
    let mut peak = 0.0f32;
    for phase in PHASES {
        let mut value = 0.0f32;
        for (sample, coefficient) in window.iter().zip(phase) {
            value += sample * coefficient;
        }
        peak = peak.max(value.abs());
    }
    peak
}

/// Largest sample or four-times oversampled peak in a channel.
///
/// The first and last samples define the finite buffer's edges by extension, keeping a DC
/// buffer constant. Boundary readings can differ from a continuous stream. Empty channels
/// measure zero. This is an estimate of the reconstructed signal, as with any finite FIR meter.
pub fn measure_channel(samples: &[f32]) -> f32 {
    let Some(&first) = samples.first() else {
        return 0.0;
    };
    let mut peak = 0.0f32;
    let mut window = [first; TAPS];
    for index in 0..samples.len() {
        for (offset, slot) in window.iter_mut().enumerate() {
            *slot = samples[(index + offset).saturating_sub(7).min(samples.len() - 1)];
        }
        peak = peak.max(samples[index].abs()).max(interval_peak(&window));
    }
    peak
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_signal_stays_constant() {
        assert!((measure_channel(&[0.5; 64]) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn quarter_rate_sine_recovers_crest() {
        let samples: Vec<f32> = (0..128)
            .map(|i| (std::f32::consts::FRAC_PI_2 * i as f32 + std::f32::consts::FRAC_PI_4).sin())
            .collect();
        let measured = measure_channel(&samples);
        assert!((0.99..=1.05).contains(&measured), "{measured}");
    }

    #[test]
    fn rendered_transient_exposes_the_old_interpolator_miss() {
        // Eight consecutive samples around the actual synthwave export's largest crest.
        // The centre exceeds 1.0 after 4x reconstruction, although each sample is below
        // 0.92 and the old four-point Catmull-Rom meter read below the -0.3 dBFS ceiling.
        let samples = [
            -0.3977586, -0.0878502, -0.3198856, -0.9146782, -0.8444753, -0.0558680, -0.3177444,
            -0.770285,
        ];
        assert!(measure_channel(&samples) > 1.0);
    }
}
