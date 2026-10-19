"""Contracts for tone-preserving upper-midrange measurements."""
import numpy as np
import pytest
from instrument_harshness import compare, measure, profiles


def test_pulse_dc_cannot_hide_its_audible_upper_midrange():
    time = np.arange(86400) / 48000
    audio = .1 * np.sin(2 * np.pi * 3200 * time)
    clean = measure(audio, 48000)
    offset = measure(audio + .75, 48000)
    assert clean["sustain"]["sharp_fraction"] == pytest.approx(1)
    assert offset["sustain"]["sharp_fraction"] == pytest.approx(1)


def test_level_matching_distinguishes_tone_changes_from_overall_attenuation():
    time = np.arange(86400) / 48000
    audio = .1 * np.sin(2 * np.pi * 440 * time) + .1 * np.sin(2 * np.pi * 3200 * time)
    gain = compare(audio, audio * .5, 48000)
    shaped = compare(audio, .1 * np.sin(2 * np.pi * 440 * time) + .05 * np.sin(2 * np.pi * 3200 * time), 48000)
    assert gain["changes"]["sustain"]["matched_sharp_db"] == pytest.approx(0)
    assert shaped["changes"]["sustain"]["matched_sharp_db"] < -3.9


def test_silent_tails_are_not_reported_as_valid_timbre_ratios():
    assert measure(np.zeros(86400), 48000)["sustain"] is None
    with pytest.raises(ValueError):
        measure(np.full(86400, np.nan), 48000)


def test_profiles_cover_every_native_family_and_high_register_bypasses():
    cases = profiles()
    assert len({case["model"] for case in cases}) == 16
    for name in ("piano", "violin", "electric_guitar", "pulse_lead", "saw_lead", "fm_lead", "vocal"):
        assert 100 in next(case for case in cases if case["name"] == name)["pitches"]
