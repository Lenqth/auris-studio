# /// script
# requires-python = ">=3.11"
# dependencies = ["faster-whisper>=1.1", "pykakasi>=2.3", "soundfile>=0.12", "scipy>=1.11"]
# ///
"""Transcribe sung VOICEVOX probes with Whisper and score them against the written kana.

`cargo run -p auris-singer --example voicevox_probe -- <dir>` writes the probes; this
reads `<dir>/phrases.json` and compares `<id>-<variant>.wav` for every variant present.
"""

from __future__ import annotations

import argparse
import json
import re
import unicodedata
from pathlib import Path

_kakasi = None


def to_hiragana(text: str) -> str:
    """Kana reading of a transcript, with punctuation, spaces and long marks removed."""
    global _kakasi
    if _kakasi is None:
        import pykakasi

        _kakasi = pykakasi.kakasi()
    text = unicodedata.normalize("NFKC", text)
    reading = "".join(item["hira"] for item in _kakasi.convert(text))
    return re.sub(r"[^ぁ-ゖ]", "", reading)


def edit_distance(a: str, b: str) -> int:
    """Levenshtein distance between two strings."""
    row = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        previous, row[0] = row[0], i
        for j, cb in enumerate(b, 1):
            previous, row[j] = (
                row[j],
                min(row[j] + 1, row[j - 1] + 1, previous + (ca != cb)),
            )
    return row[-1]


def cer(reference: str, hypothesis: str) -> float:
    """Character error rate of a hiragana hypothesis against a hiragana reference."""
    return edit_distance(reference, hypothesis) / max(len(reference), 1)


def load_16k(path: Path):
    """Mono float32 audio at Whisper's 16 kHz; decoded here rather than through PyAV."""
    from math import gcd

    import numpy as np
    import soundfile
    from scipy.signal import resample_poly

    samples, rate = soundfile.read(path, dtype="float32", always_2d=True)
    samples = samples.mean(axis=1)
    divisor = gcd(rate, 16_000)
    return resample_poly(samples, 16_000 // divisor, rate // divisor).astype(np.float32)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--model", default="large-v3-turbo")
    args = parser.parse_args()

    from faster_whisper import WhisperModel

    model = WhisperModel(args.model, device="cpu", compute_type="int8")
    phrases = json.loads((args.directory / "phrases.json").read_text(encoding="utf-8"))
    variants = sorted(
        {path.stem.split("-", 1)[1] for path in args.directory.glob("*-*.wav")}
    )
    totals = {variant: [0, 0] for variant in variants}
    results = []
    for phrase in phrases:
        reference = to_hiragana(phrase["kana"])
        row = {"id": phrase["id"], "kana": phrase["kana"]}
        for variant in variants:
            segments, _ = model.transcribe(
                load_16k(args.directory / f"{phrase['id']}-{variant}.wav"),
                language="ja",
                beam_size=5,
                temperature=0.0,
                condition_on_previous_text=False,
            )
            text = "".join(segment.text for segment in segments).strip()
            heard = to_hiragana(text)
            row[variant] = {"text": text, "kana": heard, "cer": cer(reference, heard)}
            totals[variant][0] += edit_distance(reference, heard)
            totals[variant][1] += len(reference)
        results.append(row)
        print(
            phrase["kana"],
            *(f"{v}={row[v]['kana']}({row[v]['cer']:.2f})" for v in variants),
            sep="\t",
            flush=True,
        )
    for variant, (errors, length) in totals.items():
        print(f"{variant}: CER {errors / length:.3f} ({errors}/{length})")
    (args.directory / "intelligibility.json").write_text(
        json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8"
    )


if __name__ == "__main__":
    main()
