# /// script
# requires-python = ">=3.10"
# dependencies = ["numpy>=1.26", "scipy>=1.11", "soundfile>=0.12"]
# ///
"""Voice the native piano across its whole keyboard against Steinway notes from C1 to C8.

prepare   download the hash-locked Iowa notes into REFERENCE
fit       search register curves stage by stage; the worker needs `--features piano-calibration`
evaluate  compare two workers' factory pianos note by note, with no override

The `mel` stage refits hammer and soundboard on the project's temporal mel distance over these
notes and the grand-piano cohort's training notes (`--grand`, prepared by grand_piano.py).
Voicing keys starting with an underscore are instrument parameters, such as `_position`.

Every measurement is relative within one note except `level`, which compares a note with the
same-dynamic C4 through offsets frozen at the start, and `knock`, which goes through the same
offsets. See docs/grand-piano-register.md.
"""

import argparse
import hashlib
import json
import struct
import subprocess
import sys
import time
from math import gcd
from pathlib import Path
from urllib.request import urlopen

import numpy as np
import soundfile as sf
from physical_mel import distances, features
from scipy.signal import resample_poly

REPO = Path(__file__).resolve().parents[2]
LOCK = REPO / "tools/eval/references/piano-register-captures.json"
RATE = 24_000
HOP = 600  # 25 ms
SECONDS = 8.0
T0 = 0.1
KNOCK_EDGES = ((40, 80), (80, 160), (160, 320), (320, 640), (640, 1280), (1280, 2560))
KNOCK_TIMES = (0.02, 0.06, 0.12)
STAGES = ("decay", "spectrum", "knock", "mel")
MEL_SECONDS = 3


def hz(pitch):
    return 440.0 * 2 ** ((pitch - 69) / 12)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class Worker:
    """A running `physical_fit_render`: JSON lines in, length-prefixed float32 PCM out."""

    def __init__(self, executable):
        self.process = subprocess.Popen(
            [str(executable)], stdin=subprocess.PIPE, stdout=subprocess.PIPE
        )

    def render(self, request):
        self.process.stdin.write((json.dumps(request) + "\n").encode())
        self.process.stdin.flush()
        header = self.process.stdout.read(4)
        if len(header) != 4:
            raise RuntimeError(
                "worker closed: was it built with --features piano-calibration?"
            )
        (count,) = struct.unpack("<I", header)
        data = self.process.stdout.read(count * 4)
        return np.frombuffer(data, dtype="<f4").astype(np.float64)

    def close(self):
        self.process.stdin.close()
        self.process.wait()


def onset(x):
    """Start of the rise into the loudest 2 ms frame, 25 dB below it."""
    hop = int(0.002 * RATE)
    energy = np.sqrt(np.convolve(x**2, np.ones(hop) / hop, mode="valid")[::hop])
    db = 20 * np.log10(energy + 1e-12)
    peak = int(np.argmax(db))
    index = peak
    while index > 0 and db[index] > db[peak] - 25:
        index -= 1
    return index * hop


def early_level(x):
    """Mean power from 50 to 500 ms after onset, in dB."""
    segment = x[int(0.05 * RATE) : int(0.5 * RATE)]
    return float(10 * np.log10(np.mean(segment**2) + 1e-20))


def stft(x, size):
    count = (len(x) - size) // HOP + 1
    frames = np.lib.stride_tricks.sliding_window_view(x, size)[::HOP][:count]
    window = np.hanning(size)
    return np.abs(np.fft.rfft(frames * window, axis=1)) * 2 / window.sum()


def find_partials(x, f0, count):
    """Measured partial frequencies, each search centred on the previous stretch."""
    segment = x[int(0.1 * RATE) : int(1.1 * RATE)]
    spectrum = np.abs(np.fft.rfft(segment * np.hanning(len(segment)), 1 << 19))
    freqs = np.fft.rfftfreq(1 << 19, 1 / RATE)
    found, ratio = [], 1.0
    for n in range(1, count + 1):
        target = n * f0 * ratio
        if target > 9000:
            break
        mask = (freqs > target * 0.985) & (freqs < target * 1.015)
        frequency = freqs[mask][np.argmax(spectrum[mask])]
        found.append(float(frequency))
        ratio = frequency / (n * f0)
    return found


def window_size(f0):
    return int(min(8192, 1 << int(np.ceil(np.log2(max(1024, 5 * RATE / f0))))))


def trajectories(x, freqs, size):
    """Partial levels in dB every 25 ms, the strongest bin within 0.6% of each frequency."""
    spectrum = stft(x, size)
    levels = []
    for frequency in freqs:
        low = max(1, int(frequency * 0.994 * size / RATE))
        high = max(low + 1, int(np.ceil(frequency * 1.006 * size / RATE)) + 1)
        levels.append(20 * np.log10(spectrum[:, low:high].max(axis=1) + 1e-12))
    times = (np.arange(spectrum.shape[0]) * HOP + size / 2) / RATE
    return times, np.array(levels)


def broadband(x):
    frames = np.lib.stride_tricks.sliding_window_view(x, 1200)[::HOP]
    return 10 * np.log10(np.mean(frames**2, axis=1) + 1e-20)


def band_power(x, start):
    size = 1024
    segment = x[start : start + size]
    if len(segment) < size:
        return np.full(len(KNOCK_EDGES), 1e-20)
    spectrum = np.abs(np.fft.rfft(segment * np.hanning(size))) ** 2
    freqs = np.fft.rfftfreq(size, 1 / RATE)
    return np.array(
        [spectrum[(freqs >= a) & (freqs < b)].sum() for a, b in KNOCK_EDGES]
    )


def knock_bands(x, f0, floor=None):
    """Absolute band powers in dB below 0.6 f0 at 20, 60 and 120 ms.

    With a floor, power less than 3 dB above it is unmeasurable and reported as None.
    """
    bands = []
    for seconds in KNOCK_TIMES:
        power = band_power(x, max(0, int(seconds * RATE) - 512))
        for index, (_, high) in enumerate(KNOCK_EDGES):
            if high > 0.6 * f0:
                bands.append(None)
            elif floor is None:
                bands.append(float(10 * np.log10(power[index] + 1e-20)))
            elif power[index] - floor[index] < floor[index]:
                bands.append(None)
            else:
                bands.append(float(10 * np.log10(power[index] - floor[index])))
    return bands


def with_floor(levels, floor):
    """A level heard through a noise floor at the given relative level."""
    return 10 * np.log10(10 ** (levels / 10) + 10 ** (floor / 10))


class Note:
    """One recorded note's measurements, at the analysis rate."""

    def __init__(self, path, capture):
        self.pitch = capture["pitch"]
        self.velocity = capture["velocity"]
        self.dynamic = capture["dynamic"]
        self.label = f"{self.dynamic}.{Path(path).name.split('.')[2]}"
        self.tuning = 0.0
        audio, rate = sf.read(path, always_2d=True)
        divisor = gcd(rate, RATE)
        x = resample_poly(audio.mean(axis=1), RATE // divisor, rate // divisor)
        start = onset(x)
        tail = x[-int(1.5 * RATE) :]
        self.level = early_level(x[start:])
        x = x[start : start + int(SECONDS * RATE)]
        self.mel = features(x[None, : MEL_SECONDS * RATE])
        self.f0 = hz(self.pitch)
        self.freqs = find_partials(x, self.f0, 12)
        self.size = window_size(self.freqs[0])
        self.times, self.levels = trajectories(x, self.freqs, self.size)
        self.floor = np.median(trajectories(tail, self.freqs, self.size)[1], axis=1)
        self.i0 = int(np.searchsorted(self.times, T0))
        self.broadband = broadband(x)
        self.broadband_floor = float(np.median(broadband(tail)))
        knock_floor = np.mean(
            [band_power(tail, i) for i in range(0, len(tail) - 1024, 1024)], axis=0
        )
        self.knock = knock_bands(x, self.freqs[0], knock_floor)
        start_levels = self.levels[:, self.i0]
        top = start_levels.max()
        # Partials worth fitting stand well clear of their floor and of the strongest one.
        self.use = [
            n
            for n in range(len(self.freqs))
            if start_levels[n] > self.floor[n] + 15 and start_levels[n] > top - 45
        ]


def note_losses(note, y, offset):
    """Mean absolute dB distances of one rendered note from its recording."""
    level = early_level(y[onset(y) :])
    y = y[onset(y) :][: int(SECONDS * RATE)]
    found = find_partials(y, note.f0, len(note.freqs))
    freqs = (found + note.freqs[len(found) :])[: len(note.freqs)]
    levels = trajectories(y, freqs, note.size)[1]
    frames = min(levels.shape[1], note.levels.shape[1])
    i0 = note.i0
    losses = {}
    long_enough = frames > i0 + 80
    if long_enough:
        # Decay shape: each partial relative to its own level at T0, through the same floor.
        error = weight = 0.0
        top = note.levels[note.use, i0].max()
        for n in note.use:
            recorded = note.levels[n, i0:frames] - note.levels[n, i0]
            floor = note.floor[n] - note.levels[n, i0]
            rendered = with_floor(levels[n, i0:frames] - levels[n, i0], floor)
            share = 10 ** ((note.levels[n, i0] - top) / 40)
            error += share * np.mean(np.abs(np.maximum(recorded, floor) - rendered))
            weight += share
        losses["decay"] = error / max(weight, 1e-9)
        envelope = broadband(y)
        count = min(len(envelope), len(note.broadband))
        recorded = note.broadband[i0:count] - note.broadband[i0]
        floor = note.broadband_floor - note.broadband[i0]
        rendered = with_floor(envelope[i0:count] - envelope[i0], floor)
        losses["envelope"] = float(
            np.mean(np.abs(np.maximum(recorded, floor) - rendered))
        )
    # Spectral balance: partial levels over 50-400 ms relative to their total.
    window = slice(max(1, i0 - 2), i0 + 12)
    recorded = note.levels[:, window].mean(axis=1)
    rendered = levels[:, window].mean(axis=1)
    recorded_total = 10 * np.log10(np.sum(10 ** (recorded[note.use] / 10)))
    rendered_total = 10 * np.log10(np.sum(10 ** (rendered[note.use] / 10)))
    differences = []
    for n in note.use:
        target = recorded[n] - recorded_total
        value = max(rendered[n] - rendered_total, target - 30)
        differences.append(abs(target - value) * (1.0 if target > -40 else 0.5))
    losses["spectrum"] = float(np.mean(differences))
    if offset is not None:
        losses["level"] = abs(note.level - (level + offset))
        knock = knock_bands(y, note.f0)
        pairs = [
            abs(a - (b + offset))
            for a, b in zip(note.knock, knock)
            if a is not None and b is not None
        ]
        losses["knock"] = float(np.mean(pairs)) if pairs else 0.0
    return losses


class CohortNote:
    """A grand-piano cohort note: a mel target and its recorded tuning, nothing more."""

    def __init__(self, note, audio):
        self.pitch, self.velocity = note["pitch"], note["velocity"]
        self.tuning = note.get("tuning_cents", 0.0)
        self.label = f"grand.{self.pitch}.{self.velocity}"
        self.dynamic = None
        self.mel = features(audio[None, :])


def split_parameters(voicing):
    """Separate instrument parameters (underscored keys) from the voicing proper."""
    if voicing is None:
        return None, {}
    voicing = dict(voicing)
    params = {key[1:]: voicing.pop(key) for key in [k for k in voicing if k[0] == "_"]}
    return voicing, params


class Evaluator:
    def __init__(self, notes, worker):
        self.notes = notes
        self.worker = worker
        self.seconds = SECONDS
        self.offsets = {}
        self.renders = 0

    def render(self, voicing, notes, seconds=None):
        voicing, params = split_parameters(voicing)
        length = seconds or self.seconds + 0.2
        request = {
            "model": "piano",
            "notes": [
                {"pitch": n.pitch, "velocity": n.velocity, "tuning_cents": n.tuning}
                for n in notes
            ],
            "seconds": length,
            "hold": length,
            "params": params,
            "sample_rate": RATE,
        }
        if voicing is not None:
            request["piano_voicing"] = voicing
        pcm = self.worker.render(request)
        self.renders += 1
        length = len(pcm) // len(notes)
        return [pcm[i * length : (i + 1) * length] for i in range(len(notes))]

    def freeze_levels(self, voicing):
        """Per-dynamic offsets from rendered to recorded level, taken at C4."""
        references = [n for n in self.notes if n.pitch == 60]
        for note, y in zip(references, self.render(voicing, references)):
            self.offsets[note.dynamic] = note.level - early_level(y[onset(y) :])

    def losses(self, voicing, notes):
        return {
            note.label: note_losses(note, y, self.offsets.get(note.dynamic))
            for note, y in zip(notes, self.render(voicing, notes))
        }

    def mel_cost(self, voicing, notes):
        """Mean mel distance (x100), with the level guard on register notes."""
        total = 0.0
        for note, y in zip(notes, self.render(voicing, notes, MEL_SECONDS)):
            total += 100 * float(distances(note.mel, features(y[None, :]))[0])
            if note.dynamic in self.offsets:
                level = early_level(y[onset(y) :]) + self.offsets[note.dynamic]
                total += 0.5 * abs(note.level - level)
        return total / len(notes) + penalty(voicing)

    def cost(self, voicing, notes, weights):
        if weights is None:
            return self.mel_cost(voicing, notes)
        detail = self.losses(voicing, notes)
        total = sum(
            weights.get(key, 0.0) * value
            for parts in detail.values()
            for key, value in parts.items()
        )
        return total / len(notes) + penalty(voicing)


def penalty(voicing):
    """Smoothness of register curves and of the soundboard."""
    total = 0.0
    for key, weight in (
        ("contact", 1.0),
        ("strike", 1.0),
        ("aftersound", 0.1),
        ("bridge", 0.1),
    ):
        curve = np.log(np.maximum(np.array(voicing[key], float), 1e-6))
        total += weight * float(np.sum(np.diff(curve, 2) ** 2))
    gains = np.array(voicing["soundboard"], float)
    return (
        total
        + 0.002 * float(np.sum(np.diff(gains) ** 2))
        + 0.001 * float(np.sum(gains**2))
    )


def updated(voicing, key, index, value):
    voicing = json.loads(json.dumps(voicing))
    if index is None:
        voicing[key] = value
    else:
        voicing[key][index] = value
    return voicing


def handles(stage, notes):
    """(key, index, kind, step, low, high, notes) for every searched coefficient."""
    if stage != "mel":
        notes = [note for note in notes if isinstance(note, Note)]
    at = {k: [n for n in notes if n.pitch == 24 + 12 * k] for k in range(8)}
    if stage == "decay":
        found = []
        for k in range(8):
            found.append(("aftersound", k, "log", 0.4, 0.2, 80.0, at[k]))
            found.append(("bridge", k, "log", 0.4, 0.0, 200.0, at[k]))
        found += [
            ("friction", None, "log", 0.4, 0.01, 20.0, None),
            ("bridge_slope", None, "lin", 0.2, -1.0, 1.5, None),
            ("detune", 0, "log", 0.4, 0.01, 4.0, None),
            ("detune", 1, "log", 0.4, 0.01, 2.0, None),
            ("unevenness", None, "log", 0.4, 0.001, 0.8, None),
            ("polarization", 0, "log", 0.4, 0.01, 0.7, at[0]),
            ("polarization", 1, "log", 0.4, 0.01, 1.0, at[0]),
            ("polarization", 2, "lin", 0.2, -2.0, 2.0, at[0]),
        ]
        return found, {"decay": 1.0, "envelope": 0.5}
    if stage == "spectrum":
        found = []
        for k in range(8):
            found.append(("contact", k, "log", 0.3, 0.3, 12.0, at[k]))
            found.append(("strike", k, "log", 0.2, 0.5, 2.0, at[k]))
        found += [
            ("shape", 0, "log", 0.15, 0.35, 1.6, None),
            ("shape", 1, "log", 0.15, 0.6, 3.0, None),
            ("contact_velocity", None, "lin", 0.1, 0.0, 1.5, None),
            ("core", 0, "log", 0.3, 0.02, 1.0, None),
            ("core", 1, "log", 0.4, 0.001, 1.0, None),
            ("core", 2, "lin", 0.3, 0.0, 4.0, None),
            ("highpass", None, "log", 0.3, 20.0, 400.0, None),
        ]
        found += [("soundboard", i, "lin", 2.0, -15.0, 15.0, None) for i in range(12)]
        return found, {"spectrum": 1.0, "level": 0.5}
    if stage == "knock":
        found = [("knock", k, "lin", 4.0, -80.0, 20.0, at[k]) for k in range(3, 8)]
        found += [
            ("knock_shape", 0, "log", 0.3, 60.0, 600.0, None),
            ("knock_shape", 1, "log", 0.3, 40.0, 1000.0, None),
            ("knock_shape", 2, "log", 0.3, 800.0, 10000.0, None),
        ]
        return found, {"knock": 1.0, "level": 0.5}
    if stage == "mel":
        near = {
            k: [n for n in notes if abs(n.pitch - (24 + 12 * k)) < 12] for k in range(8)
        }
        found = []
        for k in range(1, 7):
            found.append(("contact", k, "log", 0.25, 0.3, 12.0, near[k]))
            found.append(("strike", k, "log", 0.15, 0.5, 2.0, near[k]))
        found += [
            ("shape", 0, "log", 0.15, 0.35, 1.6, None),
            ("shape", 1, "log", 0.15, 0.6, 3.0, None),
            ("contact_velocity", None, "lin", 0.1, 0.0, 1.5, None),
            ("core", 0, "log", 0.3, 0.02, 1.0, None),
            ("core", 1, "log", 0.3, 0.001, 1.0, None),
            ("core", 2, "lin", 0.3, 0.0, 4.0, None),
            ("highpass", None, "log", 0.25, 20.0, 400.0, None),
            ("_position", None, "log", 0.15, 0.08, 0.4, None),
        ]
        found += [("soundboard", i, "lin", 2.0, -15.0, 15.0, None) for i in range(12)]
        return found, None
    raise ValueError(f"unknown stage {stage}")


def coordinate(evaluator, voicing, stage, passes, log):
    """Bounded coordinate search; a register anchor renders only the notes it voices."""
    searched, weights = handles(stage, evaluator.notes)
    pool = [
        note for note in evaluator.notes if stage == "mel" or isinstance(note, Note)
    ]
    cache = {}

    def cost(candidate, notes):
        key = (json.dumps(candidate, sort_keys=True), tuple(n.label for n in notes))
        if key not in cache:
            cache[key] = evaluator.cost(candidate, notes, weights)
        return cache[key]

    steps = {(key, index): step for key, index, _, step, *_ in searched}
    for sweep in range(passes):
        improved = False
        for key, index, kind, _, low, high, notes in searched:
            notes = notes or pool
            if not notes:
                continue
            step = steps[(key, index)]
            base = cost(voicing, notes)
            current = voicing[key] if index is None else voicing[key][index]
            best = (base, current)
            for direction in (1, -1):
                trial = (
                    current * np.exp(direction * step)
                    if kind == "log"
                    else current + direction * step
                )
                trial = float(np.clip(trial, low, high))
                if trial != current:
                    value = cost(updated(voicing, key, index, trial), notes)
                    if value < best[0] - 1e-4:
                        best = (value, trial)
            if best[1] != current:
                voicing = updated(voicing, key, index, best[1])
                improved = True
                log(
                    f"  {sweep} {key}[{index}] {current:.5g} -> {best[1]:.5g}: {base:.4f} -> {best[0]:.4f}"
                )
            else:
                steps[(key, index)] = step * 0.6
        if not improved and all(step < 0.02 for step in steps.values()):
            break
    return voicing


def read_lock():
    return json.loads(LOCK.read_text(encoding="utf-8"))["captures"]


def load_notes(root, fitted_only=True):
    notes = []
    for capture in read_lock():
        path = root / capture["path"]
        if digest(path) != capture["sha256"]:
            raise ValueError(f"reference hash mismatch: {path}")
        if capture["fit"] or not fitted_only:
            notes.append(Note(path, capture))
    return notes


def prepare(root):
    root.mkdir(parents=True, exist_ok=True)
    for capture in read_lock():
        path = root / capture["path"]
        if not path.exists():
            with urlopen(capture["url"], timeout=120) as response:
                raw = response.read(40_000_001)
            if len(raw) > 40_000_000 or raw[:4] != b"FORM":
                raise ValueError(f"invalid AIFF: {path.name}")
            path.write_bytes(raw)
        if digest(path) != capture["sha256"]:
            raise ValueError(f"reference hash mismatch: {path}")
        print(path.name, flush=True)


def cohort_notes(grand):
    """The grand-piano calibration's training notes, three seconds each."""
    from grand_piano import load_cohort

    notes, audio = load_cohort(grand, MEL_SECONDS, "train")
    return [CohortNote(note, clip) for note, clip in zip(notes, audio)]


def fit(root, executable, start, output, stages, passes, grand=None):
    notes = load_notes(root)
    if "mel" in stages:
        if grand is None:
            sys.exit("the mel stage needs --grand")
        notes += cohort_notes(grand)
    evaluator = Evaluator(notes, Worker(executable))
    voicing = json.loads(start.read_text(encoding="utf-8"))
    if "mel" in stages:
        # The factory Excitation Position, which the mel stage refines with the voicing.
        voicing.setdefault("_position", 0.2444)
    lines = []

    def log(text):
        print(text, flush=True)
        lines.append(text)

    started = time.time()
    evaluator.freeze_levels(voicing)
    register_notes = [n for n in notes if isinstance(n, Note)]
    report = {"renderer_sha256": digest(executable), "start": voicing, "stages": []}
    for stage in stages:
        log(f"== {stage}")
        evaluator.seconds = SECONDS if stage == "decay" else 0.8
        voicing = coordinate(evaluator, voicing, stage, passes, log)
        evaluator.seconds = SECONDS
        report["stages"].append({"stage": stage, "voicing": voicing})
    report["losses"] = evaluator.losses(voicing, register_notes)
    report["voicing"] = voicing
    report["log"] = lines
    report["renders"] = evaluator.renders
    report["seconds"] = round(time.time() - started)
    output.write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")
    evaluator.worker.close()


def evaluate(root, before, after, output):
    notes = load_notes(root)
    report = {
        "before_sha256": digest(before),
        "after_sha256": digest(after),
        "references": {c["path"]: c["sha256"] for c in read_lock()},
        "notes": {},
    }
    means = {}
    for label, executable in (("before", before), ("after", after)):
        evaluator = Evaluator(notes, Worker(executable))
        evaluator.freeze_levels(None)
        losses = evaluator.losses(None, notes)
        evaluator.worker.close()
        for note, parts in losses.items():
            report["notes"].setdefault(note, {})[label] = parts
        keys = sorted({k for parts in losses.values() for k in parts})
        means[label] = {
            k: float(np.mean([p[k] for p in losses.values() if k in p])) for k in keys
        }
    report["means"] = means
    for key in means["before"]:
        print(
            f"{key:9s} {means['before'][key]:7.2f} dB -> {means['after'][key]:7.2f} dB"
        )
    output.write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("prepare")
    fitter = commands.add_parser("fit")
    fitter.add_argument("renderer", type=Path)
    fitter.add_argument("start", type=Path)
    fitter.add_argument("output", type=Path)
    fitter.add_argument("--stages", default="decay,spectrum,knock")
    fitter.add_argument("--passes", type=int, default=8)
    fitter.add_argument("--grand", type=Path, help="grand_piano.py reference directory")
    evaluator = commands.add_parser("evaluate")
    evaluator.add_argument("before", type=Path)
    evaluator.add_argument("after", type=Path)
    evaluator.add_argument("output", type=Path)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args.reference)
    elif args.command == "fit":
        stages = args.stages.split(",")
        if not set(stages) <= set(STAGES):
            sys.exit(f"stages are {', '.join(STAGES)}")
        fit(
            args.reference,
            args.renderer,
            args.start,
            args.output,
            stages,
            args.passes,
            args.grand,
        )
    else:
        evaluate(args.reference, args.before, args.after, args.output)


if __name__ == "__main__":
    main()
