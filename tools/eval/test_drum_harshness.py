"""Contracts for absolute band-power and drum-voicing comparisons."""
import numpy as np
import pytest
from drum_harshness import band_power, compare, delta_db, measure


def test_band_power_recovers_mean_square_of_an_in_band_sine():
    time = np.arange(2400) / 48000
    audio = .2 * np.sin(2 * np.pi * 3000 * time)
    assert band_power(audio, 48000, 2000, 5000) == pytest.approx(.2**2 / 2)
    assert band_power(audio, 48000, 500, 1500) < 1e-20


def test_comparison_reports_gain_changes_without_normalizing_them_away():
    time = np.arange(48000) / 48000
    audio = np.random.default_rng(19).normal(0, .1, len(time)) * np.exp(-time * 10)
    row = compare(audio, audio * .5, 48000)
    assert row["rms_delta_db"] == pytest.approx(-6.0206, abs=1e-4)
    assert row["attack_band_delta_db"]["sharp"] == pytest.approx(-6.0206, abs=1e-4)
    assert row["before"]["t90_seconds"] == row["after"]["t90_seconds"]


@pytest.mark.parametrize("audio", [np.zeros(48000), np.full(48000, np.nan), np.ones(100)])
def test_invalid_measurements_are_rejected(audio):
    with pytest.raises(ValueError):
        measure(audio, 48000)


def test_undefined_power_ratios_are_rejected():
    with pytest.raises(ValueError):
        delta_db(0, 1)
