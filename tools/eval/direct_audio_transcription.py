"""Blind, local audio-LLM transcription trials against frozen score/audio pairs.

Use the same uv environment as piano_transcription.py. Inference sends audio and
one fixed prompt only; reference scores are opened in the separate scoring step.
"""

from __future__ import annotations

import argparse
import base64
import json
import math
import time
import urllib.request
import wave
from pathlib import Path

import numpy as np
from piano_transcription import (
    checked_notes,
    counts,
    digest,
    metric,
    reference_union,
    verify_frozen_project,
)

PROMPT = """Transcribe the attached instrumental audio into all audible pitched notes.
Return only a JSON object with keys "notes" and "limitations".
"notes" is a list of objects with "pitch" (integer MIDI key), "start" and "end"
(seconds from the beginning of the attached audio). Include simultaneous notes
separately and repeated attacks as separate events. Do not replace transcription
with a newly composed melody. For silence return an empty notes list. If you
cannot determine any notes, return an empty list and explain in "limitations".
Do not invent a score. No reference score or transcription tool is available."""


def save(path: Path, value: dict) -> None:
    """Write human-readable evidence without overwriting an earlier trial."""
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write("\n")


def parse_notes(content: str, seconds: float) -> list[dict]:
    """Require complete, bounded events; never extract guesses from prose/reasoning."""
    value = json.loads(content)
    if not isinstance(value, dict) or not isinstance(value.get("notes"), list):
        raise TypeError("response must contain a notes array")
    notes = value["notes"]
    for note in notes:
        if not isinstance(note, dict) or type(note.get("pitch")) is not int:
            raise ValueError("MIDI pitch must be an integer")
        if any(type(note.get(k)) not in (int, float) for k in ("start", "end")):
            raise ValueError("note times must be numeric")
        if note["end"] > seconds + 0.05:
            raise ValueError("note extends beyond the supplied audio")
    return checked_notes(notes)


def controls(directory: Path) -> None:
    """Generate unseen pitch/time and silence controls, independent of the composer."""
    directory.mkdir()
    patterns = {
        "silence": [],
        "melody": [
            {"pitch": p, "start": 0.4 + i * 0.7, "end": 0.9 + i * 0.7}
            for i, p in enumerate((62, 69, 65, 74, 57, 66))
        ],
        "chords": [
            {"pitch": p, "start": start, "end": start + 0.8}
            for start, pitches in ((0.4, (50, 57, 65)), (2.1, (55, 62, 70)))
            for p in pitches
        ],
    }
    cases = []
    for name, notes in patterns.items():
        signal = np.zeros(6 * 16000, dtype=np.float64)
        for note in notes:
            begin, end = (round(note[k] * 16000) for k in ("start", "end"))
            t = np.arange(end - begin) / 16000
            envelope = np.minimum(1, np.minimum(t / 0.01, (len(t) / 16000 - t) / 0.02))
            frequency = 440 * 2 ** ((note["pitch"] - 69) / 12)
            signal[begin:end] += 0.16 * envelope * np.sin(2 * math.pi * frequency * t)
        audio = directory / f"{name}.wav"
        with wave.open(str(audio), "wb") as stream:
            stream.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
            stream.writeframes((signal * 32767).astype("<i2").tobytes())
        save(directory / f"{name}.json", {"notes": notes})
        cases.append(
            {
                "id": name,
                "audio": audio.name,
                "reference": f"{name}.json",
                "seconds": 6.0,
                "split": "control",
                "variant": name,
            }
        )
    save(
        directory / "manifest.json",
        {"suite": "direct-audio-controls-v1", "cases": cases},
    )


def infer(args: argparse.Namespace) -> None:
    """Send independent requests without scores, case names, tempo or note candidates."""
    corpus = json.loads(args.manifest.read_text(encoding="utf-8"))
    cases = [c for c in corpus["cases"] if c["split"] == args.split]
    if not cases:
        raise ValueError("empty evaluation cohort")
    args.output.mkdir()
    metadata = {
        "model": args.model,
        "url": args.url,
        "prompt": PROMPT,
        "manifest_sha256": digest(args.manifest),
        "split": args.split,
        "temperature": 0,
        "max_tokens": args.max_tokens,
        "reasoning_effort": "none",
        "audio_first": not args.text_first,
        "response_format": None if args.unconstrained else "json_object",
        "cases": [c["id"] for c in cases],
    }
    save(args.output / "run.json", metadata)
    for case in cases:
        audio = args.manifest.parent / case["audio"]
        data = base64.b64encode(audio.read_bytes()).decode("ascii")
        parts = [
            {"type": "input_audio", "input_audio": {"data": data, "format": "wav"}},
            {"type": "text", "text": PROMPT},
        ]
        if args.text_first:
            parts.reverse()
        body = {
            "model": args.model,
            "messages": [{"role": "user", "content": parts}],
            "temperature": 0,
            "max_tokens": args.max_tokens,
            "reasoning_effort": "none",
            "stream": False,
        }
        if not args.unconstrained:
            body["response_format"] = {"type": "json_object"}
        request = urllib.request.Request(
            args.url.rstrip("/") + "/chat/completions",
            data=json.dumps(body).encode("utf-8"),
            headers={"Content-Type": "application/json"},
        )
        started = time.perf_counter()
        result = {
            "id": case["id"],
            "audio_sha256": digest(audio),
            "seconds": case["seconds"],
        }
        try:
            with urllib.request.urlopen(request, timeout=args.timeout) as response:
                raw = json.load(response)
            result["response"] = raw
            choice = raw["choices"][0]
            if choice["finish_reason"] != "stop":
                raise ValueError(f"incomplete response: {choice['finish_reason']}")
            result["notes"] = parse_notes(choice["message"]["content"], case["seconds"])
            result["status"] = "valid"
        except (ValueError, TypeError, KeyError, IndexError, OSError) as error:
            result.update(status="failed", error=str(error))
        result["elapsed_seconds"] = time.perf_counter() - started
        save(args.output / f"{case['id']}.json", result)
        print(
            json.dumps({k: result[k] for k in ("id", "status", "elapsed_seconds")}),
            flush=True,
        )


def score(args: argparse.Namespace) -> None:
    """Open withheld labels only after inference; failed requests remain visible."""
    corpus = json.loads(args.manifest.read_text(encoding="utf-8"))
    run = json.loads((args.estimates / "run.json").read_text(encoding="utf-8"))
    if digest(args.manifest) != run["manifest_sha256"]:
        raise ValueError("manifest changed since inference")
    cases = []
    for case in corpus["cases"]:
        if case["id"] not in run["cases"]:
            continue
        path = args.estimates / f"{case['id']}.json"
        result = json.loads(path.read_text(encoding="utf-8"))
        audio_hash = digest(args.manifest.parent / case["audio"])
        if result["audio_sha256"] != audio_hash:
            raise ValueError("audio changed since inference")
        reference_path = args.manifest.parent / case["reference"]
        raw = json.loads(reference_path.read_text(encoding="utf-8"))["notes"]
        project_hash = (
            verify_frozen_project(args.manifest.parent / case["project"], raw)
            if case.get("project")
            else None
        )
        reference = reference_union(raw)
        # Operational failures supply no usable draft, but are not musical empty outputs.
        notes = result.get("notes", [])
        raw_notes, raw_error = [], None
        try:
            # Preserve musically scoreable events even when they exceed source duration.
            # No clipping/repair or implicit partial draft is applied to the actual result.
            raw_notes = parse_notes(
                result["response"]["choices"][0]["message"]["content"], float("inf")
            )
        except (ValueError, TypeError, KeyError, IndexError) as error:
            raw_error = str(error)
        measured = {
            "id": case["id"],
            "variant": case["variant"],
            "status": result["status"],
            "elapsed_seconds": result["elapsed_seconds"],
            "audio_sha256": audio_hash,
            "reference_sha256": digest(reference_path),
            "project_sha256": project_hash,
            "result_sha256": digest(path),
            "reference_notes": len(reference),
            "estimated_notes": len(notes),
            "raw_candidate_notes": len(raw_notes),
            "raw_parse_error": raw_error,
            "raw_onset_50ms": metric(reference, raw_notes, 0.05),
        }
        for key, tolerance, offsets in (
            ("onset_50ms", 0.05, False),
            ("onset_100ms", 0.1, False),
            ("onset_offset", 0.05, True),
        ):
            measured[key] = metric(reference, notes, tolerance, offsets)
        if args.baseline:
            baseline_path = args.baseline / f"{case['id']}.json"
            baseline = json.loads(baseline_path.read_text())
            measured["basic_pitch"] = metric(
                reference, baseline["transcription"]["notes"], 0.05
            )
            measured["basic_pitch_seconds"] = baseline["elapsed_seconds"]
            measured["basic_pitch_estimate_sha256"] = digest(baseline_path)
        cases.append(measured)
    groups = {}
    for variant in ("all", *sorted({c["variant"] for c in cases})):
        selected = [c for c in cases if variant == "all" or c["variant"] == variant]
        fields = ["onset_50ms", "onset_100ms", "onset_offset", "raw_onset_50ms"]
        if args.baseline:
            fields.append("basic_pitch")
        groups[variant] = {
            "cases": len(selected),
            "failed": sum(c["status"] == "failed" for c in selected),
            "empty_drafts": sum(
                c["status"] == "valid" and c["estimated_notes"] == 0 for c in selected
            ),
            "seconds": sum(c["elapsed_seconds"] for c in selected),
        }
        for field in fields:
            groups[variant][field] = counts(
                *(
                    sum(c[field][k] for c in selected)
                    for k in ("matches", "false_positives", "false_negatives")
                )
            )
    save(
        args.output,
        {
            "schema": 1,
            "run": run,
            "run_sha256": digest(args.estimates / "run.json"),
            "cases": cases,
            "groups": groups,
        },
    )
    print(json.dumps(groups, indent=2))


def main() -> None:
    """Keep input-only inference and reference-aware scoring separate."""
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    control = commands.add_parser("controls")
    control.add_argument("output", type=Path)
    run = commands.add_parser("infer")
    run.add_argument("manifest", type=Path)
    run.add_argument("output", type=Path)
    run.add_argument("--model", required=True)
    run.add_argument("--url", default="http://localhost:11434/v1")
    run.add_argument("--split", default="held_out")
    run.add_argument("--max-tokens", type=int, default=8192)
    run.add_argument("--timeout", type=float, default=180)
    run.add_argument("--text-first", action="store_true")
    run.add_argument("--unconstrained", action="store_true")
    scoring = commands.add_parser("score")
    scoring.add_argument("manifest", type=Path)
    scoring.add_argument("estimates", type=Path)
    scoring.add_argument("output", type=Path)
    scoring.add_argument("--baseline", type=Path)
    args = parser.parse_args()
    if args.command == "controls":
        controls(args.output)
    elif args.command == "infer":
        infer(args)
    else:
        score(args)


if __name__ == "__main__":
    main()
