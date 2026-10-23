//! Whole-buffer loudness measurement.
//!
//! Written for offline callers that scan every sample of a finished render. Session analysis and
//! automatic mix balancing prefer the GPU and expose the true-peak estimate through the toolbox;
//! the GUI export dialog still reports only the loudest stored sample.
//!
//! Use [`analyze_loudness`] — it tries the GPU and silently falls back to
//! [`analyze_loudness_cpu`].

use auris_core::AudioBuffer;
use auris_core::param::{db_to_gain, gain_to_db};
use auris_dsp::true_peak;

use crate::context::GpuContext;

/// Pipeline cache key and debug label for the loudness kernel.
const LABEL: &str = "auris-gpu.loudness";

/// WGSL source of the loudness kernel.
const SHADER: &str = include_str!("shaders/loudness.wgsl");

/// Samples reduced by one GPU invocation.
///
/// Large enough that the per-invocation overhead disappears, small enough that the shader's
/// f32 sum of squares stays accurate — the relative error of a naive f32 sum grows roughly
/// with the square root of the term count, so 4096 terms cost about 4e-6, and the host adds
/// the partial sums back together in f64.
const SAMPLES_PER_INVOCATION: usize = 4096;

/// Uniform block handed to the loudness kernel. Must match `Params` in `loudness.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    sample_count: u32,
    first_active: u32,
    last_active: u32,
    span: u32,
}

/// Level measurements over a whole buffer.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LoudnessAnalysis {
    /// Largest absolute sample value across all channels.
    pub peak: f32,
    /// Root-mean-square level over every sample of every channel.
    pub rms: f32,
    /// Estimated largest value of the reconstructed continuous signal.
    ///
    /// A four-times oversampled, 16-tap FIR measures peaks between stored samples. As with
    /// other finite FIR meters this is an estimate; it is never less than [`Self::peak`].
    pub true_peak_estimate: f32,
}

impl LoudnessAnalysis {
    /// The measurement of a buffer with no samples.
    pub fn silent() -> Self {
        Self {
            peak: 0.0,
            rms: 0.0,
            true_peak_estimate: 0.0,
        }
    }

    /// Builds the analysis from accumulated totals.
    fn from_totals(peak: f32, sum_squares: f64, count: usize, true_peak: f32) -> Self {
        let rms = if count == 0 {
            0.0
        } else {
            (sum_squares / count as f64).sqrt() as f32
        };
        Self {
            peak,
            rms,
            true_peak_estimate: true_peak.max(peak),
        }
    }

    /// Sample peak in dBFS, floored at -120 dB.
    pub fn peak_db(&self) -> f32 {
        gain_to_db(self.peak)
    }

    /// RMS level in dBFS, floored at -120 dB.
    pub fn rms_db(&self) -> f32 {
        gain_to_db(self.rms)
    }

    /// Estimated true peak in dBFS, floored at -120 dB.
    pub fn true_peak_db(&self) -> f32 {
        gain_to_db(self.true_peak_estimate)
    }

    /// Linear gain that would bring the estimated true peak to `target_db`.
    ///
    /// Returns 1.0 for silence, where no gain reaches the target. Normalising against the true
    /// peak rather than the sample peak is what keeps the result from clipping a converter.
    pub fn normalization_gain(&self, target_db: f32) -> f32 {
        if self.true_peak_estimate <= 1e-9 {
            1.0
        } else {
            db_to_gain(target_db) / self.true_peak_estimate
        }
    }
}

/// Measures peak, RMS and estimated true peak on the CPU.
///
/// This is the reference implementation the GPU kernel is tested against, and what runs when
/// no GPU is available. The sum of squares is accumulated in `f64` so that a long render does
/// not lose precision.
pub fn analyze_loudness_cpu(buffer: &AudioBuffer) -> LoudnessAnalysis {
    let mut peak = 0.0f32;
    let mut true_peak = 0.0f32;
    let mut sum_squares = 0.0f64;
    let mut count = 0usize;

    for samples in buffer.channels() {
        for &value in samples {
            peak = peak.max(value.abs());
            sum_squares += (value * value) as f64;
        }
        true_peak = true_peak.max(true_peak::measure_channel(samples));
        count += samples.len();
    }

    LoudnessAnalysis::from_totals(peak, sum_squares, count, true_peak)
}

impl GpuContext {
    /// Measures sample peak and RMS on the GPU, then true peak on the CPU.
    ///
    /// Each invocation reduces a fixed span of samples to a partial
    /// (min, max, sum of squares); the host folds the partials together, summing in `f64`.
    /// The FIR true-peak measurement runs on the CPU over each complete channel so GPU chunk
    /// boundaries cannot change the result.
    ///
    /// Returns `None` on any wgpu failure; use [`analyze_loudness`] to fall back automatically.
    pub fn analyze_loudness(&self, buffer: &AudioBuffer) -> Option<LoudnessAnalysis> {
        // `frame_count()` only describes the first channel, and `AudioBuffer::channels_mut`
        // lets the rest differ. The CPU reference scans every channel, so take the shortcut
        // only when there is genuinely nothing to scan.
        if buffer.channels().iter().all(|samples| samples.is_empty()) {
            return Some(LoudnessAnalysis::silent());
        }

        let per_chunk = (self.max_input_samples() / SAMPLES_PER_INVOCATION)
            .min(self.max_output_slots())
            * SAMPLES_PER_INVOCATION;
        if per_chunk == 0 {
            log::debug!("auris-gpu: device limits leave no room for a loudness dispatch");
            return None;
        }

        let mut peak = 0.0f32;
        let mut true_peak = 0.0f32;
        let mut sum_squares = 0.0f64;
        let mut count = 0usize;

        for samples in buffer.channels() {
            let mut start = 0;
            while start < samples.len() {
                let active = per_chunk.min(samples.len() - start);
                let chunk = samples.get(start..start + active)?;

                let params = Params {
                    sample_count: chunk.len() as u32,
                    first_active: 0,
                    last_active: active as u32,
                    span: SAMPLES_PER_INVOCATION as u32,
                };
                let slots = active.div_ceil(SAMPLES_PER_INVOCATION);
                for slot in self.run_reduction(LABEL, SHADER, chunk, params, slots)? {
                    peak = peak.max(slot[0].abs()).max(slot[1].abs());
                    sum_squares += slot[2] as f64;
                }
                start += active;
            }
            true_peak = true_peak.max(true_peak::measure_channel(samples));
            count += samples.len();
        }

        Some(LoudnessAnalysis::from_totals(
            peak,
            sum_squares,
            count,
            true_peak,
        ))
    }
}

/// Measures a buffer's loudness, preferring the GPU.
///
/// This is what callers use. A `None` context, or any GPU failure, transparently runs
/// [`analyze_loudness_cpu`] instead.
pub fn analyze_loudness(gpu: Option<&GpuContext>, buffer: &AudioBuffer) -> LoudnessAnalysis {
    if let Some(gpu) = gpu {
        if let Some(analysis) = gpu.analyze_loudness(buffer) {
            return analysis;
        }
        log::debug!("auris-gpu: loudness analysis fell back to the CPU");
    }
    analyze_loudness_cpu(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `frames` samples of a sine at `period` samples per cycle, scaled to `amplitude`.
    fn sine(frames: usize, period: f32, amplitude: f32, phase: f32) -> AudioBuffer {
        let samples = (0..frames)
            .map(|i| amplitude * (std::f32::consts::TAU * i as f32 / period + phase).sin())
            .collect();
        AudioBuffer::from_planar(vec![samples], 48_000.0).unwrap()
    }

    #[test]
    fn full_scale_sine_measures_minus_three_db_rms() {
        // 48 samples per period lands exactly on the crest and the trough.
        let analysis = analyze_loudness_cpu(&sine(480, 48.0, 1.0, 0.0));
        assert!((analysis.peak - 1.0).abs() < 1e-6, "{analysis:?}");
        assert!(
            (analysis.rms - 1.0 / 2.0f32.sqrt()).abs() < 1e-5,
            "{analysis:?}"
        );
        // A sine's RMS is 3.0103 dB below its peak.
        assert!((analysis.rms_db() + 3.0103).abs() < 1e-2, "{analysis:?}");
        assert!((analysis.peak_db()).abs() < 1e-3, "{analysis:?}");
    }

    #[test]
    fn silence_measures_as_the_floor() {
        let analysis = analyze_loudness_cpu(&AudioBuffer::stereo(256, 48_000.0));
        assert_eq!(analysis, LoudnessAnalysis::silent());
        assert_eq!(analysis.rms_db(), -120.0);
        assert_eq!(analysis.normalization_gain(-1.0), 1.0);
    }

    #[test]
    fn empty_buffer_measures_as_silence() {
        let analysis = analyze_loudness_cpu(&AudioBuffer::stereo(0, 48_000.0));
        assert_eq!(analysis, LoudnessAnalysis::silent());
    }

    #[test]
    fn constant_signal_has_no_inter_sample_overshoot() {
        // The phase taps are unity-normalised, so a DC buffer stays at its sample level.
        let buffer = AudioBuffer::from_planar(vec![vec![0.5; 64]], 48_000.0).unwrap();
        let analysis = analyze_loudness_cpu(&buffer);
        assert_eq!(analysis.peak, 0.5);
        assert_eq!(analysis.rms, 0.5);
        assert!((analysis.true_peak_estimate - 0.5).abs() < 1e-6);
        // 0.5 -> 1.0 is exactly +6.0206 dB, so normalising to 0 dBFS doubles the signal.
        assert!((analysis.normalization_gain(0.0) - 2.0).abs() < 1e-4);
        assert!((analysis.normalization_gain(-6.0206) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn inter_sample_peak_exceeds_the_sample_peak() {
        // A quarter-rate sine offset by 45 degrees samples at +-0.70711 while the underlying
        // waveform reaches 1.0. FIR endpoint ringing may read a little higher on this finite
        // block, but the meter must recover the missing crest.
        let buffer = sine(64, 4.0, 1.0, std::f32::consts::FRAC_PI_4);
        let analysis = analyze_loudness_cpu(&buffer);
        assert!(
            (analysis.peak - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5,
            "{analysis:?}"
        );
        assert!(
            (0.99..1.05).contains(&analysis.true_peak_estimate),
            "{analysis:?}"
        );
        assert!(analysis.true_peak_estimate > analysis.peak);
        // The overshoot is about 3 dB above the sample peak.
        assert!(
            (2.9..3.4).contains(&(analysis.true_peak_db() - analysis.peak_db())),
            "{analysis:?}"
        );
    }

    #[test]
    fn rms_covers_every_channel() {
        // One full-scale channel and one silent channel average to -3 dB of power, i.e. an
        // overall RMS of 1/sqrt(2) of the loud channel's.
        let buffer =
            AudioBuffer::from_planar(vec![vec![1.0; 128], vec![0.0; 128]], 48_000.0).unwrap();
        let analysis = analyze_loudness_cpu(&buffer);
        assert_eq!(analysis.peak, 1.0);
        assert!(
            (analysis.rms - 1.0 / 2.0f32.sqrt()).abs() < 1e-6,
            "{analysis:?}"
        );
    }

    #[test]
    fn without_a_context_the_wrapper_is_the_cpu_path() {
        let buffer = sine(1000, 37.0, 0.6, 0.3);
        assert_eq!(
            analyze_loudness(None, &buffer),
            analyze_loudness_cpu(&buffer)
        );
    }

    #[test]
    fn gpu_matches_the_cpu_reference() {
        let Some(gpu) = GpuContext::new() else {
            return;
        };
        // Long enough to need several invocations, and not a multiple of the span.
        let frames = 12_345;
        let left: Vec<f32> = (0..frames)
            .map(|i| (i as f32 * 0.017).sin() * 0.8)
            .collect();
        let right: Vec<f32> = (0..frames)
            .map(|i| (i as f32 * 1.5).sin() * 0.55 + 0.05)
            .collect();
        let buffer = AudioBuffer::from_planar(vec![left, right], 48_000.0).unwrap();

        let expected = analyze_loudness_cpu(&buffer);
        let actual = gpu
            .analyze_loudness(&buffer)
            .expect("a working adapter should complete the reduction");

        assert!(
            (expected.peak - actual.peak).abs() < 1e-4,
            "cpu {expected:?} gpu {actual:?}"
        );
        assert!(
            (expected.rms - actual.rms).abs() < 1e-4,
            "cpu {expected:?} gpu {actual:?}"
        );
        assert!(
            (expected.true_peak_estimate - actual.true_peak_estimate).abs() < 1e-4,
            "cpu {expected:?} gpu {actual:?}"
        );
    }

    #[test]
    fn gpu_matches_the_cpu_across_chunk_boundaries() {
        let Some(mut gpu) = GpuContext::new() else {
            return;
        };
        // One output slot per dispatch forces a chunk every SAMPLES_PER_INVOCATION samples.
        gpu.constrain_for_test(usize::MAX, 1);

        // Deliberately full of inter-sample peaks: a quarter-rate sine at 45 degrees overshoots
        // by about 3 dB. The length is not a multiple of the chunk size. The second channel stays
        // quieter than the first channel's *sample* peak of 1/sqrt(2), so the overshoot is what
        // sets `true_peak_estimate` for the whole buffer.
        let frames = 3 * SAMPLES_PER_INVOCATION + 1_234;
        let left: Vec<f32> = (0..frames)
            .map(|i| (std::f32::consts::TAU * i as f32 / 4.0 + std::f32::consts::FRAC_PI_4).sin())
            .collect();
        let right: Vec<f32> = (0..frames)
            .map(|i| (i as f32 * 0.017).sin() * 0.5)
            .collect();
        let buffer = AudioBuffer::from_planar(vec![left, right], 48_000.0).unwrap();

        let expected = analyze_loudness_cpu(&buffer);
        let actual = gpu
            .analyze_loudness(&buffer)
            .expect("the chunked path should still complete");

        // The overshoot must actually be present, or the test proves nothing about the overlap.
        assert!(
            expected.true_peak_estimate > expected.peak * 1.2,
            "{expected:?}"
        );
        assert!(
            (expected.peak - actual.peak).abs() < 1e-4,
            "cpu {expected:?} gpu {actual:?}"
        );
        assert!(
            (expected.rms - actual.rms).abs() < 1e-4,
            "cpu {expected:?} gpu {actual:?}"
        );
        assert!(
            (expected.true_peak_estimate - actual.true_peak_estimate).abs() < 1e-4,
            "cpu {expected:?} gpu {actual:?}"
        );
    }

    #[test]
    fn gpu_chunking_preserves_an_overshoot_that_straddles_the_boundary() {
        let Some(mut gpu) = GpuContext::new() else {
            return;
        };
        // One output slot per dispatch puts a chunk boundary every SAMPLES_PER_INVOCATION
        // samples: chunk 0 reduces [0, 4096), chunk 1 reduces [4096, 8192).
        gpu.constrain_for_test(usize::MAX, 1);
        let edge = SAMPLES_PER_INVOCATION;

        // A periodic signal hides a broken overlap, because the same overshoot recurs
        // everywhere and the buffer maximum survives losing one sample's worth of it. So plant
        // exactly one overshoot in an otherwise silent buffer and put it on the seam.
        //
        // `-A, A, A, -A` has a large reconstructed overshoot. Place it on each side of the
        // dispatch boundary to confirm that the host's complete-channel FIR measurement agrees
        // with the CPU reference regardless of GPU chunking.
        const A: f32 = 0.8;
        for (name, first) in [("look-ahead", edge - 2), ("look-behind", edge - 1)] {
            let mut samples = vec![0.0f32; 2 * SAMPLES_PER_INVOCATION + 500];
            samples[first] = -A;
            samples[first + 1] = A;
            samples[first + 2] = A;
            samples[first + 3] = -A;
            let buffer = AudioBuffer::from_planar(vec![samples], 48_000.0).unwrap();

            let expected = analyze_loudness_cpu(&buffer);
            // The FIR peak is substantially above the highest stored sample.
            assert!(
                (expected.peak - A).abs() < 1e-6,
                "{name}: {expected:?} should peak at the sample value"
            );
            assert!(
                expected.true_peak_estimate > 1.5 * A,
                "{name}: {expected:?} should overshoot"
            );

            let actual = gpu
                .analyze_loudness(&buffer)
                .expect("the chunked path should still complete");
            assert!(
                (expected.true_peak_estimate - actual.true_peak_estimate).abs() < 1e-4,
                "{name}: cpu {expected:?} gpu {actual:?}"
            );
            assert!(
                (expected.peak - actual.peak).abs() < 1e-4
                    && (expected.rms - actual.rms).abs() < 1e-4,
                "{name}: cpu {expected:?} gpu {actual:?}"
            );
        }
    }

    #[test]
    fn mono_and_ragged_buffers_measure_the_same_on_both_paths() {
        let mut buffer =
            AudioBuffer::from_planar(vec![vec![0.25; 500], vec![-0.5; 500]], 48_000.0).unwrap();
        buffer.channels_mut()[1].truncate(100);

        let cpu = analyze_loudness_cpu(&buffer);
        // 500 samples at 0.25 and 100 at -0.5: peak is 0.5 and the mean square is
        // (500 * 0.0625 + 100 * 0.25) / 600.
        assert_eq!(cpu.peak, 0.5);
        let expected_rms = ((500.0 * 0.0625 + 100.0 * 0.25) / 600.0f32).sqrt();
        assert!((cpu.rms - expected_rms).abs() < 1e-6, "{cpu:?}");

        if let Some(gpu) = GpuContext::new() {
            let actual = gpu
                .analyze_loudness(&buffer)
                .expect("a working adapter should complete the reduction");
            assert!(
                (cpu.peak - actual.peak).abs() < 1e-4,
                "cpu {cpu:?} gpu {actual:?}"
            );
            assert!(
                (cpu.rms - actual.rms).abs() < 1e-4,
                "cpu {cpu:?} gpu {actual:?}"
            );
            assert!(
                (cpu.true_peak_estimate - actual.true_peak_estimate).abs() < 1e-4,
                "cpu {cpu:?} gpu {actual:?}"
            );
        }
    }
}
