//! Numeric guards for where the physical instruments place their notes.

use auris_core::param::gain_to_db;
use auris_core::{Instrument, NoteEvent};

use crate::render::{StereoImage, pan_gains};
use crate::test_support::{Rig, peak, rms};
use crate::{
    Clarinet, HammeredDulcimer, Model, Physical, TinWhistle, clarinet, hammered_dulcimer,
    tin_whistle,
};

const RATE: f64 = 48_000.0;

fn on(frame: u32, pitch: u8) -> NoteEvent {
    NoteEvent::NoteOn {
        frame,
        pitch,
        velocity: 0.8,
    }
}

/// A stereo instrument with its image and a low and a high note inside its range.
struct Case {
    name: &'static str,
    make: fn() -> Box<dyn Instrument>,
    image: StereoImage,
    low: u8,
    high: u8,
}

fn case(
    name: &'static str,
    make: fn() -> Box<dyn Instrument>,
    image: StereoImage,
    low: u8,
    high: u8,
) -> Case {
    Case {
        name,
        make,
        image,
        low,
        high,
    }
}

fn instruments() -> Vec<Case> {
    vec![
        case(
            "clarinet",
            || Box::new(Clarinet::new()),
            clarinet::IMAGE,
            55,
            86,
        ),
        case(
            "dulcimer",
            || Box::new(HammeredDulcimer::new()),
            hammered_dulcimer::IMAGE,
            55,
            81,
        ),
        case(
            "whistle",
            || Box::new(TinWhistle::new()),
            tin_whistle::IMAGE,
            76,
            93,
        ),
        case(
            "piano",
            || Box::new(Physical::new(Model::Piano)),
            Model::Piano.stereo_image(),
            36,
            96,
        ),
        case(
            "guitar",
            || Box::new(Physical::new(Model::Guitar)),
            Model::Guitar.stereo_image(),
            45,
            79,
        ),
        case(
            "electric guitar",
            || Box::new(Physical::new(Model::ElectricGuitar)),
            Model::ElectricGuitar.stereo_image(),
            45,
            79,
        ),
        case(
            "bass",
            || Box::new(Physical::new(Model::Bass)),
            Model::Bass.stereo_image(),
            31,
            60,
        ),
        case(
            "bell",
            || Box::new(Physical::new(Model::Bell)),
            Model::Bell.stereo_image(),
            60,
            84,
        ),
        case(
            "mallet",
            || Box::new(Physical::new(Model::Mallet)),
            Model::Mallet.stereo_image(),
            45,
            84,
        ),
        case(
            "strings",
            || Box::new(Physical::new(Model::Violin)),
            Model::Violin.stereo_image(),
            43,
            84,
        ),
    ]
}

fn level_difference_db(left: &[f32], right: &[f32]) -> f32 {
    gain_to_db(rms(left)) - gain_to_db(rms(right))
}

#[test]
fn a_piano_is_heard_from_the_players_seat() {
    let mut rig = Rig::new(Box::new(Physical::new(Model::Piano)), RATE, 256, 2);
    let (left, right) = rig.render_stereo(24_000, &[on(0, 36)]);
    let low = level_difference_db(&left, &right);
    rig.instrument.reset();
    let (left, right) = rig.render_stereo(24_000, &[on(0, 96)]);
    let high = level_difference_db(&left, &right);
    // C2 sits about 7.5 dB to the left and C7 about 8 dB to the right; E4 is centred.
    assert!((6.5..8.5).contains(&low), "C2 is {low} dB left of right");
    assert!(
        (-9.0..-6.5).contains(&high),
        "C7 is {high} dB left of right"
    );
    rig.instrument.reset();
    let (left, right) = rig.render_stereo(24_000, &[on(0, 64)]);
    assert!(level_difference_db(&left, &right).abs() < 0.2);
}

#[test]
fn strings_follow_orchestral_seating() {
    let mut rig = Rig::new(Box::new(Physical::new(Model::Violin)), RATE, 256, 2);
    let (left, right) = rig.render_stereo(24_000, &[on(0, 43)]);
    assert!(
        level_difference_db(&left, &right) < -3.0,
        "a low string note should sit on the right, with the cellos"
    );
    rig.instrument.reset();
    let (left, right) = rig.render_stereo(24_000, &[on(0, 84)]);
    assert!(
        level_difference_db(&left, &right) > 3.0,
        "a high string note should sit on the left, with the violins"
    );
}

#[test]
fn each_note_is_placed_by_the_pan_law_at_every_width() {
    for Case {
        name,
        make,
        image,
        low,
        high,
    } in instruments()
    {
        for width in [0.5, 1.0, 2.0] {
            for pitch in [low, high] {
                let mut instrument = make();
                instrument.set_param_by_key("width", width);
                let mut rig = Rig::new(instrument, RATE, 256, 2);
                let (left, right) = rig.render_stereo(12_000, &[on(0, pitch)]);
                let (to_left, to_right) = pan_gains(image.position(f32::from(pitch)) * width);
                let expected = gain_to_db(to_left / to_right);
                let measured = level_difference_db(&left[4_800..], &right[4_800..]);
                // The piano's soundboard knock comes from the whole case and so sits in the
                // centre, which narrows a high, quickly fading note a little.
                let tolerance = if name == "piano" {
                    0.05 + 0.06 * expected.abs()
                } else {
                    0.05
                };
                assert!(
                    (measured - expected).abs() < tolerance,
                    "{name} pitch {pitch} width {width}: {measured} dB, expected {expected} dB"
                );
            }
        }
    }
}

#[test]
fn the_downmix_is_the_centred_instrument() {
    let chord_of = |low: u8, high: u8| {
        let middle = ((u16::from(low) + u16::from(high)) / 2) as u8;
        [on(0, low), on(1_000, middle), on(2_000, high)]
    };
    for Case {
        name,
        make,
        low,
        high,
        ..
    } in instruments()
    {
        let chord = chord_of(low, high);
        let mut centred = make();
        centred.set_param_by_key("width", 0.0);
        let mut centred = Rig::new(centred, RATE, 256, 2);
        let (left, right) = centred.render_stereo(12_000, &chord);
        assert_eq!(left, right, "{name} at zero width is not mono");

        let mut wide = Rig::new(make(), RATE, 256, 2);
        let (wide_left, wide_right) = wide.render_stereo(12_000, &chord);
        assert_ne!(wide_left, wide_right, "{name} has no stereo image");
        let downmix: Vec<f32> = wide_left
            .iter()
            .zip(&wide_right)
            .map(|(l, r)| (l + r) * 0.5)
            .collect();
        let error: Vec<f32> = downmix.iter().zip(&left).map(|(a, b)| a - b).collect();
        assert!(
            peak(&error) <= peak(&left) * 1.0e-4,
            "{name}'s downmix differs from its centred render by {}",
            peak(&error)
        );

        let mut mono = Rig::new(make(), RATE, 256, 1);
        let mono = mono.render(12_000, &chord);
        let error: Vec<f32> = mono.iter().zip(&left).map(|(a, b)| a - b).collect();
        assert!(
            peak(&error) <= peak(&left) * 1.0e-4,
            "{name} on a mono bus differs from its centred render"
        );
    }
}

#[test]
fn surround_buses_carry_the_downmix_beyond_the_first_pair() {
    for Case {
        name, make, low, ..
    } in instruments()
    {
        // The rig checks every channel past the second against the downmix itself.
        let mut rig = Rig::new(make(), RATE, 256, 4);
        let rendered = rig.render(4_096, &[on(0, low)]);
        assert!(
            peak(&rendered) > 0.0,
            "{name} was silent on a four-channel bus"
        );
    }
}

#[test]
fn turning_the_width_moves_a_held_note_without_a_step() {
    let mut rig = Rig::new(Box::new(Physical::new(Model::Piano)), RATE, 256, 2);
    let (left, right) = rig.render_stereo(4_096, &[on(0, 36)]);
    let before = level_difference_db(&left[2_048..], &right[2_048..]);
    rig.set_param("width", 0.0);
    let (left, right) = rig.render_stereo(4_096, &[]);
    // Ten milliseconds in, the pan is still on its way; by the end it has arrived.
    let early = level_difference_db(&left[..240], &right[..240]);
    let late = level_difference_db(&left[2_048..], &right[2_048..]);
    assert!(before > 6.0, "C2 started {before} dB left");
    assert!(early > 0.5 && early < before, "the pan jumped: {early} dB");
    assert!(late.abs() < 0.05, "the pan never arrived: {late} dB");
}
