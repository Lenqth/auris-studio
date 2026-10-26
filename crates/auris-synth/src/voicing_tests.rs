//! Numeric guards for upper-midrange voicing and retained fundamentals.

use auris_core::{Instrument, NoteEvent, Parameterized};
use auris_dsp::{Biquad, BiquadCoefficients};

use crate::test_support::{Rig, goertzel, rms};
use crate::{Chiptune, Clarinet, Fm2, Model, Physical, Vocal};

#[test]
fn bright_instruments_reduce_upper_midrange_without_losing_the_note() {
    // Each ceiling is below the measured pre-change band/fundamental amplitude ratio.
    type Probe = (Box<dyn Instrument>, u8, bool, f64, f64);
    let cases: [Probe; 9] = [
        // The piano's C6 sits about 9 dB under C4, as measured on a grand.
        (Box::new(Physical::new(Model::Piano)), 84, true, 0.65, 0.012),
        (
            Box::new(Physical::new(Model::Guitar)),
            85,
            true,
            0.345,
            0.0083,
        ),
        (
            Box::new(Physical::new(Model::ElectricGuitar)),
            85,
            true,
            0.80,
            0.044,
        ),
        (
            Box::new(Physical::new(Model::Violin)),
            85,
            false,
            0.85,
            0.012,
        ),
        (Box::new(Clarinet::new()), 77, false, 0.23, 0.41),
        (
            {
                let mut synth = Chiptune::new();
                synth.set_param_by_key("pulse_width", 0.125);
                Box::new(synth)
            },
            84,
            false,
            0.76,
            0.14,
        ),
        (
            {
                let mut synth = Chiptune::new();
                synth.set_param_by_key("waveform", 2.0);
                Box::new(synth)
            },
            84,
            false,
            0.39,
            0.18,
        ),
        (
            {
                let mut synth = Fm2::new();
                synth.set_param_by_key("ratio", 1.0);
                Box::new(synth)
            },
            84,
            true,
            0.84,
            0.078,
        ),
        (Box::new(Vocal::new()), 81, false, 0.48, 0.028),
    ];
    for (instrument, pitch, attack, ceiling, minimum_fundamental) in cases {
        let name = instrument.descriptor().id.to_string();
        let mut rig = Rig::new(instrument, 48_000.0, 256, 2);
        let audio = rig.render(
            86_400,
            &[NoteEvent::NoteOn {
                frame: 0,
                pitch,
                velocity: 1.0,
            }],
        );
        let mut highpass = Biquad::new(BiquadCoefficients::highpass(
            48_000.0,
            2_000.0,
            std::f32::consts::FRAC_1_SQRT_2,
        ));
        let mut lowpass = Biquad::new(BiquadCoefficients::lowpass(
            48_000.0,
            5_000.0,
            std::f32::consts::FRAC_1_SQRT_2,
        ));
        let band: Vec<_> = audio
            .iter()
            .map(|x| lowpass.process_sample(highpass.process_sample(*x)))
            .collect();
        let window = if attack { 0..7_200 } else { 7_200..21_600 };
        let frequency = 440.0 * ((f64::from(pitch) - 69.0) / 12.0).exp2();
        let fundamental = goertzel(&audio[window.clone()], 48_000.0, frequency);
        let ratio = f64::from(rms(&band[window])) / fundamental;
        assert!(
            fundamental > minimum_fundamental,
            "{name}: fundamental {fundamental}"
        );
        assert!(
            ratio < ceiling,
            "{name}: upper-midrange/fundamental {ratio}, ceiling {ceiling}"
        );
    }
}
