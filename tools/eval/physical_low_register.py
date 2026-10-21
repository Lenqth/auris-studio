# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=1.26", "scipy>=1.11", "soundfile>=0.12"]
# ///
"""Compare physical bass notes with bounded-memory, sequential Rust renders.

    uv run tools/eval/physical_low_register.py before.exe after.exe target/low-register/check

Reports diagnostic measurements, not a perceptual preference score. Optional Iowa mf
AIFFs in --reference-dir are inspected without downloading or modifying recordings.
"""

import argparse
import hashlib
import json
from pathlib import Path

import numpy as np
import soundfile as sf
from fit_physical_mel import Renderer
from scipy.optimize import least_squares
from scipy.signal import find_peaks

PATCHES = {
    "piano": ("piano", {}),
    "bright": (
        "piano",
        {"hardness": 0.9, "damping": 0.03, "position": 0.12, "body": 0.5},
    ),
    "felt": ("piano", {"hardness": 0.2, "damping": 0.25, "decay": 5, "body": 0.8}),
    "strings": ("violin", {}),
    "viola": (
        "violin",
        {
            "body": 0.72,
            "damping": 0.42,
            "bow_pressure": 0.38,
            "bow_speed": 0.24,
            "release": 0.18,
        },
    ),
    "cello": (
        "violin",
        {
            "body": 0.85,
            "damping": 0.5,
            "bow_pressure": 0.42,
            "bow_speed": 0.28,
            "release": 0.3,
        },
    ),
    "contrabass": (
        "violin",
        {
            "body": 0.95,
            "damping": 0.62,
            "bow_pressure": 0.48,
            "bow_speed": 0.32,
            "release": 0.4,
        },
    ),
}


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def audition(audio, rate):
    """Fade listening boundaries only; measurements and raw evaluation PCM stay intact."""
    audio = audio.copy()
    frames = round(rate * 0.005)
    ramp = np.linspace(0, 1, frames)
    audio[:frames] *= ramp
    audio[-frames:] *= ramp[::-1]
    return audio


def spectrum(audio, rate):
    power = abs(np.fft.rfft(audio * np.hanning(len(audio)), len(audio) * 8)) ** 2
    return np.fft.rfftfreq(len(audio) * 8, 1 / rate), power


def periodicity(audio, rate, frequency):
    """Maximum normalized one-period correlation within 0.5% of the target period."""
    period = rate / frequency
    lags = range(max(1, int(period * 0.995)), int(np.ceil(period * 1.005)) + 1)
    if len(audio) <= max(lags) * 2:
        raise ValueError("need more than two periods")
    values = []
    for lag in lags:
        left, right = audio[:-lag], audio[lag:]
        left, right = left - left.mean(), right - right.mean()
        norm = np.sqrt(np.dot(left, left) * np.dot(right, right))
        values.append(float(np.dot(left, right) / norm) if norm > 1e-20 else 0.0)
    return max(values)


def partial_frequency(frequencies, power, target, low=0.99, high=1.07):
    """Locate a local spectral maximum around a named harmonic, without gain matching."""
    peaks, _ = find_peaks(power)
    peaks = peaks[
        (frequencies[peaks] > target * low) & (frequencies[peaks] < target * high)
    ]
    if not len(peaks):
        raise ValueError("no partial in the search band")
    return float(frequencies[peaks[np.argmax(power[peaks])]])


def measure(audio, rate, pitch):
    if not np.isfinite(audio).all():
        raise ValueError("nonfinite render")
    frequency = 440 * 2 ** ((pitch - 69) / 12)
    attack = audio[round(rate * 0.3) : round(rate * 0.8)]
    sustain = audio[round(rate * 1.5) : round(rate * 2.5)]
    bins, power = spectrum(attack, rate)
    fundamental = power[(bins > frequency * 0.8) & (bins < frequency * 1.2)].sum()
    upper = power[(bins > frequency * 5) & (bins < min(frequency * 40, 5000))].sum()
    full_bins, full_power = spectrum(audio[:rate], rate)
    eighth = partial_frequency(full_bins, full_power, frequency * 8)
    return {
        "peak": float(max(abs(audio))),
        "attack_rms": float(np.sqrt(np.mean(attack**2))),
        "sustain_rms": float(np.sqrt(np.mean(sustain**2))),
        "attack_periodicity": periodicity(attack, rate, frequency),
        "sustain_periodicity": periodicity(sustain, rate, frequency),
        "attack_upper_to_fundamental_db": float(
            10 * np.log10(max(upper, 1e-30) / max(fundamental, 1e-30))
        ),
        "eighth_partial_cents": float(1200 * np.log2(eighth / (frequency * 8))),
    }


def reference(path, pitch):
    audio, rate = sf.read(path, always_2d=True)
    audio = audio.mean(axis=1)
    onset = int(np.flatnonzero(abs(audio) > max(abs(audio)) * 0.02)[0])
    bins, power = spectrum(audio[onset : onset + rate], rate)
    nominal = 440 * 2 ** ((pitch - 69) / 12)
    partials = [
        partial_frequency(bins, power, nominal * n, 0.97, 1.06) for n in range(1, 13)
    ]
    indices = np.arange(1, 13)
    fit = least_squares(
        lambda values: (
            partials
            - indices
            * values[0]
            * np.sqrt((1 + values[1] * indices**2) / (1 + values[1]))
        ),
        [nominal, 0.0003],
        bounds=([nominal * 0.95, 0], [nominal * 1.05, 0.01]),
    )
    return {
        "file": path.name,
        "sha256": digest(path),
        "pitch": pitch,
        "url": "https://theremin.music.uiowa.edu/sound%20files/MIS/"
        f"Piano_Other/piano/{path.name}",
        "onset_seconds": onset / rate,
        "fundamental_hz": float(fit.x[0]),
        "inharmonicity": float(fit.x[1]),
        "fit_rmse_hz": float(np.sqrt(np.mean(fit.fun**2))),
        "partials_hz": partials,
    }


def compare(before, after, output, reference_dir=None):
    output.mkdir(parents=True, exist_ok=True)
    report = {
        "schema_version": 1,
        "seconds": 3.0,
        "hold": 2.5,
        "before_sha256": digest(before),
        "after_sha256": digest(after),
        "cases": [],
        "controls": [],
        "references": [],
    }
    workers = [Renderer(path.resolve()) for path in (before, after)]
    try:
        for rate in (24000, 48000, 96000):
            for patch, (model, params) in PATCHES.items():
                for pitch in (24, 30, 36, 48, 69, 85, 88):
                    for velocity in (0.35, 0.8):
                        notes = [{"pitch": pitch, "velocity": velocity}]
                        pair = [
                            worker.render(model, notes, params, 3.0, 2.5, rate)[0]
                            for worker in workers
                        ]
                        report["cases"].append(
                            {
                                "patch": patch,
                                "model": model,
                                "params": params,
                                "rate": rate,
                                "pitch": pitch,
                                "velocity": velocity,
                                "before": measure(pair[0], rate, pitch),
                                "after": measure(pair[1], rate, pitch),
                            }
                        )
                        if (
                            rate == 48000
                            and patch in ("piano", "strings")
                            and pitch in (24, 36)
                            and velocity == 0.8
                        ):
                            # Common gain exposes changes in level; no limiting or per-take EQ.
                            gain = 0.8 / max(max(abs(audio)) for audio in pair)
                            for label, audio in zip(
                                ("before", "after"), pair, strict=True
                            ):
                                raw_dir = output / label
                                raw_dir.mkdir(exist_ok=True)
                                sf.write(
                                    raw_dir / f"{patch}-{pitch}.wav",
                                    audio,
                                    rate,
                                    subtype="FLOAT",
                                )
                                sf.write(
                                    output / f"{patch}-{pitch}-{label}.wav",
                                    audition(audio, rate) * gain,
                                    rate,
                                    subtype="FLOAT",
                                )
                            after_gain = np.sqrt(
                                np.mean(pair[0] ** 2) / np.mean(pair[1] ** 2)
                            )
                            matched = (pair[0], pair[1] * after_gain)
                            headroom = 0.8 / max(max(abs(audio)) for audio in matched)
                            sf.write(
                                output / f"{patch}-{pitch}-ab-rms-matched.wav",
                                np.concatenate(
                                    (
                                        audition(matched[0], rate),
                                        np.zeros(rate // 3),
                                        audition(matched[1], rate),
                                    )
                                )
                                * headroom,
                                rate,
                                subtype="FLOAT",
                            )
        for model in ("piano", "guitar", "bass", "bell", "mallet", "electric_guitar"):
            for pitch in (24, 48, 69):
                pair = [
                    worker.render(
                        model, [{"pitch": pitch, "velocity": 0.8}], {}, 1.0, 0.8, 48000
                    )[0]
                    for worker in workers
                ]
                identical = bool(np.array_equal(*pair))
                report["controls"].append(
                    {"model": model, "pitch": pitch, "identical": identical}
                )
                if (model != "piano" or pitch >= 48) and not identical:
                    raise ValueError(f"unrelated control changed: {model} MIDI {pitch}")
    finally:
        for worker in workers:
            worker.close()
    if reference_dir:
        for name, pitch in (("C1", 24), ("C2", 36), ("C3", 48)):
            report["references"].append(
                reference(reference_dir / f"Piano.mf.{name}.aiff", pitch)
            )
    (output / "report.json").write_text(
        json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8"
    )
    print(
        f"{len(report['cases'])} paired cases; controls verified; {output / 'report.json'}"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--reference-dir", type=Path)
    args = parser.parse_args()
    compare(args.before, args.after, args.output, args.reference_dir)
