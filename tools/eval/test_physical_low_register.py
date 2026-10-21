"""Guards for the bass-note measuring tools; no model downloads or renderer needed."""

import numpy as np
import pytest
from physical_low_register import partial_frequency, periodicity, spectrum


def test_periodicity_is_gain_and_dc_invariant():
    rate, frequency = 24000, 32.7
    time = np.arange(12000) / rate
    audio = np.sin(2 * np.pi * frequency * time)
    value = periodicity(audio, rate, frequency)
    assert value > 0.999
    assert periodicity(audio * 0.03 + 0.2, rate, frequency) == pytest.approx(value)


def test_periodicity_distinguishes_noise_and_rejects_short_windows():
    noise = np.random.default_rng(123).normal(size=12000)
    assert periodicity(noise, 24000, 32.7) < 0.1
    with pytest.raises(ValueError, match="two periods"):
        periodicity(noise[:50], 24000, 32.7)


def test_partial_measurement_resolves_stretch_independently_of_level():
    rate, fundamental = 24000, 65.4
    eighth = fundamental * 8 * 2 ** (17 / 1200)
    time = np.arange(rate) / rate
    audio = np.sin(2 * np.pi * fundamental * time) + 0.03 * np.sin(
        2 * np.pi * eighth * time
    )
    bins, power = spectrum(audio, rate)
    measured = partial_frequency(bins, power, fundamental * 8)
    assert measured == pytest.approx(eighth, abs=0.125)
    assert 1200 * np.log2(measured / (fundamental * 8)) == pytest.approx(17, abs=0.5)


def test_partial_measurement_rejects_an_empty_search_band():
    with pytest.raises(ValueError, match="no partial"):
        partial_frequency(np.arange(10), np.ones(10), 100)
