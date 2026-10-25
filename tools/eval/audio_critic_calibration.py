# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""Collect audio observations with identity and obvious impairment controls.

Synthetic controls test gross discrimination and order bias, not musical taste.
The Rust worker uses the production assessment path and never receives condition labels.
"""

from __future__ import annotations

import argparse
import array
import math
import shutil
import subprocess
import sys
import wave
from pathlib import Path

from composition_quality import digest, read_json, write_json

RATE = 24000
SECONDS = 4


def phrase(variant: int, disordered: bool = False) -> list[float]:
    pitches = [60, 64, 67, 72, 71, 67, 62, 60]
    if disordered:
        pitches = [60, 83, 49, 78, 56, 85, 43, 66]
    samples = []
    for index in range(RATE * SECONDS):
        position = index / RATE
        note = min(int(position * 2), 7)
        local = position - note / 2
        frequency = 440 * 2 ** ((pitches[note] + variant * 2 - 69) / 12)
        envelope = min(local / 0.01, 1.0) * max(0.0, min((0.45 - local) / 0.08, 1.0))
        lead = envelope * (
            math.sin(2 * math.pi * frequency * local)
            + 0.18 * math.sin(4 * math.pi * frequency * local)
        )
        bass = math.sin(2 * math.pi * 130.8128 * 2 ** (variant / 6) * position)
        samples.append(0.22 * lead + 0.025 * bass)
    return samples


def rms(samples: list[float]) -> float:
    return math.sqrt(sum(x * x for x in samples) / len(samples))


def degraded(samples: list[float], kind: str, variant: int) -> list[float]:
    if kind == "identity":
        return samples.copy()
    if kind == "silence":
        return [0.0] * len(samples)
    if kind == "clipping":
        result = [max(-0.035, min(0.035, sample)) for sample in samples]
    elif kind == "disordered_pitch":
        result = phrase(variant, disordered=True)
    else:
        raise ValueError(kind)
    gain = rms(samples) / rms(result)
    return [sample * gain for sample in result]


def wav(path: Path, samples: list[float]) -> None:
    if any(not math.isfinite(x) or abs(x) >= 1 for x in samples):
        raise ValueError("Calibration audio must be finite and below full scale")
    pcm = array.array("h", (round(x * 32767) for x in samples))
    if sys.byteorder != "little":
        pcm.byteswap()
    with wave.open(str(path), "wb") as stream:
        stream.setparams((1, 2, RATE, 0, "NONE", "not compressed"))
        stream.writeframes(pcm.tobytes())


def controls(output: Path) -> list[dict]:
    trials = []
    for variant in range(3):
        clean = phrase(variant)
        reference = output / f"reference-{variant}.wav"
        wav(reference, clean)
        for kind in ["identity", "silence", "clipping", "disordered_pitch"]:
            candidate = output / f"{kind}-{variant}.wav"
            wav(candidate, degraded(clean, kind, variant))
            for reverse in [False, True]:
                paths = [reference, candidate]
                if reverse:
                    paths.reverse()
                trials.append(
                    {
                        "id": f"{kind}-{variant}-{'ba' if reverse else 'ab'}",
                        "kind": kind,
                        "variant": variant,
                        "reference_label": "b" if reverse else "a",
                        "paths": [str(p.resolve()) for p in paths],
                        "hashes": [digest(p) for p in paths],
                    }
                )
    return trials


def outcome(trial: dict, reply: dict) -> str:
    if "error" in reply:
        return "transport_failed"
    if isinstance(reply.get("raw_response"), str) and "assessment" not in reply:
        return (
            "manual_review_required"
            if reply["raw_response"].strip()
            else "invalid_response"
        )
    if not reply.get("assessment") or reply.get("assessment_error"):
        return "invalid_response"
    verdict = reply["assessment"]["verdict"]
    if verdict in {"tie", "cannot_judge"}:
        return verdict
    if trial["kind"] == "identity":
        return "false_preference"
    return (
        "reference_preferred"
        if verdict == f"prefer_{trial['reference_label']}"
        else "impairment_preferred"
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--limit", type=int, default=24, help="Use 2 for a transport smoke test"
    )
    args = parser.parse_args()
    if not 1 <= args.limit <= 24:
        parser.error("--limit must be within 1..24")
    args.output.mkdir(parents=True, exist_ok=False)
    runtime = args.output / "runtime"
    runtime.mkdir()
    executable = args.worker.resolve(strict=True)
    worker = runtime / executable.name
    shutil.copy2(executable, worker)
    for folder in [executable.parent.parent, executable.parent]:
        for source in folder.glob("*.dll"):
            shutil.copy2(source, runtime / source.name)
    trials = controls(args.output)[: args.limit]
    report = {
        "schema_version": 1,
        "worker_sha256": digest(worker),
        "interpretation": "Identity false preferences and order sensitivity measure calibration failures. Impairment controls are screening hypotheses, not universal musical preferences. Cannot-judge and invalid responses are not passes.",
        "trials": [],
    }
    for trial in trials:
        request, response = args.output / "request.json", args.output / "response.json"
        write_json(
            request,
            {
                "a": trial["paths"][0],
                "b": trial["paths"][1],
                "seconds": SECONDS,
                "aspect": "sound"
                if trial["kind"] == "clipping"
                else "musical_coherence",
            },
        )
        if response.exists():
            response.unlink()
        with (args.output / f"{trial['id']}.log").open("w", encoding="utf-8") as log:
            try:
                process = subprocess.run(
                    [str(worker), str(request.resolve()), str(response.resolve())],
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    timeout=330,
                    check=False,
                )
                reply = (
                    read_json(response)
                    if response.exists()
                    else {"error": f"Worker exit {process.returncode} without a report"}
                )
            except subprocess.TimeoutExpired:
                reply = {"error": "Worker timed out after 330 seconds"}
        trial["reply"] = reply
        trial["outcome"] = outcome(trial, reply)
        report["trials"].append(trial)
        report["counts"] = {
            name: sum(t["outcome"] == name for t in report["trials"])
            for name in sorted({t["outcome"] for t in report["trials"]})
        }
        write_json(args.output / "report.json", report)
        print(f"{trial['id']}: {trial['outcome']}", flush=True)


if __name__ == "__main__":
    main()
