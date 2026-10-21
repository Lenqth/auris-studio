"""Score frozen composer/audio pairs with one-to-one polyphonic note matching.

Use a uv environment with numpy/scipy. No audio or model is downloaded by this script.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path

import numpy as np
from scipy.sparse import csr_matrix
from scipy.sparse.csgraph import maximum_bipartite_matching


def digest(path: Path) -> str:
    """Hash files in chunks without retaining the recording in memory."""
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def checked_notes(notes: list[dict]) -> list[dict]:
    """Reject invalid intervals rather than letting NaNs silently evade matching."""
    for note in notes:
        pitch, start, end = (float(note[k]) for k in ("pitch", "start", "end"))
        if not all(math.isfinite(x) for x in (pitch, start, end)):
            raise ValueError("nonfinite reference/estimate")
        if not (0 <= pitch <= 127 and 0 <= start < end):
            raise ValueError("invalid MIDI pitch or source-time interval")
    if len(notes) > 10000:
        raise ValueError("evaluate excerpts with at most 10000 notes")
    return sorted(notes, key=lambda n: (n["pitch"], n["start"], n["end"]))


def reference_union(notes: list[dict]) -> list[dict]:
    """One piano key cannot represent coincident same-pitch attacks from several parts."""
    result: list[dict] = []
    for note in checked_notes(notes):
        if (
            result
            and result[-1]["pitch"] == note["pitch"]
            and note["start"] - result[-1]["start"] <= 0.012
        ):
            result[-1]["end"] = max(result[-1]["end"], note["end"])
        else:
            result.append(dict(note))
    return result


def metric(
    reference: list[dict], estimate: list[dict], tolerance: float, offsets: bool = False
) -> dict:
    """Maximum exact-pitch matching; no estimate may satisfy two reference events."""
    reference, estimate = checked_notes(reference), checked_notes(estimate)
    if len(reference) * len(estimate) > 4000000:
        raise ValueError("evaluate excerpts with at most four million note pairs")
    rows, cols = [], []
    for i, ref in enumerate(reference):
        for j, est in enumerate(estimate):
            if (
                ref["pitch"] != est["pitch"]
                or abs(ref["start"] - est["start"]) > tolerance + 1e-9
            ):
                continue
            if (
                offsets
                and abs(ref["end"] - est["end"])
                > max(0.05, 0.2 * (ref["end"] - ref["start"])) + 1e-9
            ):
                continue
            rows.append(i)
            cols.append(j)
    graph = csr_matrix(
        (np.ones(len(rows)), (rows, cols)), shape=(len(reference), len(estimate))
    )
    matches = (
        int(np.sum(maximum_bipartite_matching(graph, perm_type="column") >= 0))
        if rows
        else 0
    )
    return counts(matches, len(estimate) - matches, len(reference) - matches)


def counts(matches: int, false_positives: int, false_negatives: int) -> dict:
    """Keep event counts alongside precision/recall to make regressions visible."""
    precision = matches / max(1, matches + false_positives)
    recall = matches / max(1, matches + false_negatives)
    return {
        "matches": matches,
        "false_positives": false_positives,
        "false_negatives": false_negatives,
        "precision": precision,
        "recall": recall,
        "f1": 2 * precision * recall / (precision + recall)
        if precision + recall
        else 0.0,
    }


def verify_frozen_project(project: Path, reference: list[dict]) -> str:
    """Independently check saved frozen notes against source-second labels (960 ticks/beat)."""
    document = json.loads(project.read_text(encoding="utf-8"))
    points = document["tempo_map"]["points"]
    if len(points) != 1 or points[0]["tick"] != 0:
        raise ValueError("composer fixture must use one constant tempo")
    seconds_per_tick = 60 / points[0]["bpm"] / 960
    rendered = []
    for track in document["tracks"]:
        kind = track["kind"]
        if kind["type"] != "instrument":
            continue
        if track["mixer"]["mute"]:
            raise ValueError("frozen reference includes a muted track")
        for clip in kind["clips"]:
            if any(
                clip.get(key)
                for key in (
                    "muted",
                    "transforms",
                    "recipe",
                    "loop_end",
                    "bend",
                    "controllers",
                )
            ):
                raise ValueError(
                    "fixture notes must already have performance/loops frozen"
                )
            for note in clip["notes"]:
                start = clip["start"] + note["start"]
                rendered.append(
                    {
                        "pitch": note["pitch"],
                        "start": start * seconds_per_tick,
                        "end": (start + note["length"]) * seconds_per_tick,
                    }
                )
    actual, expected = checked_notes(rendered), checked_notes(reference)
    if len(actual) != len(expected) or any(
        a["pitch"] != e["pitch"]
        or abs(a["start"] - e["start"]) > 1e-8
        or abs(a["end"] - e["end"]) > 1e-8
        for a, e in zip(actual, expected, strict=True)
    ):
        raise ValueError("saved performance and reference annotations disagree")
    return digest(project)


def score(manifest: Path, estimates: Path) -> dict:
    """Score actual backend outputs against the immutable pair manifest."""
    corpus = json.loads(manifest.read_text(encoding="utf-8"))
    ids = [case["id"] for case in corpus["cases"]]
    if len(ids) != len(set(ids)):
        raise ValueError("duplicate case IDs")
    cases = []
    for case in corpus["cases"]:
        ref_path = manifest.parent / case["reference"]
        est_path = estimates / f"{case['id']}.json"
        raw_reference = json.loads(ref_path.read_text(encoding="utf-8"))["notes"]
        project_hash = (
            verify_frozen_project(manifest.parent / case["project"], raw_reference)
            if case.get("project")
            else None
        )
        reference = reference_union(raw_reference)
        estimated = json.loads(est_path.read_text(encoding="utf-8"))
        notes = estimated["transcription"]["notes"]
        cases.append(
            {
                "id": case["id"],
                "split": case["split"],
                "variant": case["variant"],
                "audio_sha256": digest(manifest.parent / case["audio"]),
                "reference_sha256": digest(ref_path),
                "frozen_project_sha256": project_hash,
                "estimate_sha256": digest(est_path),
                "model_sha256": estimated["transcription"]["model_sha256"],
                "elapsed_seconds": estimated["elapsed_seconds"],
                "reference_notes": len(reference),
                "estimated_notes": len(notes),
                "onset_50ms": metric(reference, notes, 0.05),
                "onset_100ms": metric(reference, notes, 0.1),
                "onset_offset": metric(reference, notes, 0.05, True),
            }
        )
    groups = {}
    for split in sorted({c["split"] for c in cases}):
        for variant in ("all", *sorted({c["variant"] for c in cases})):
            selected = [
                c
                for c in cases
                if c["split"] == split and (variant == "all" or c["variant"] == variant)
            ]
            metrics = {}
            for key in ("onset_50ms", "onset_100ms", "onset_offset"):
                sums = [
                    sum(c[key][field] for c in selected)
                    for field in ("matches", "false_positives", "false_negatives")
                ]
                metrics[key] = counts(*sums)
            groups[f"{split}/{variant}"] = dict(cases=len(selected), **metrics)
    return {
        "schema": 1,
        "suite": corpus["suite"],
        "manifest_sha256": digest(manifest),
        "matching": "Maximum one-to-one exact MIDI pitch; 50/100 ms onset; offset tolerance max(50 ms, 20% duration). Coincident same-pitch references unioned within 12 ms.",
        "cases": cases,
        "groups": groups,
    }


def compare(report: dict, baseline: dict) -> dict:
    """Only compare the same frozen recordings, annotations and model weights."""
    for field in ("schema", "suite", "matching"):
        if report[field] != baseline[field]:
            raise ValueError(f"measurement {field} changed")
    if report["manifest_sha256"] != baseline["manifest_sha256"]:
        raise ValueError("manifest changed")
    old = {c["id"]: c for c in baseline["cases"]}
    if set(old) != {c["id"] for c in report["cases"]}:
        raise ValueError("case cohort changed")
    for case in report["cases"]:
        for field in (
            "audio_sha256",
            "reference_sha256",
            "model_sha256",
            "split",
            "variant",
        ):
            if case[field] != old[case["id"]][field]:
                raise ValueError(f"{case['id']}: {field} changed")
        if case.get("frozen_project_sha256") != old[case["id"]].get(
            "frozen_project_sha256"
        ):
            raise ValueError("frozen project changed")
    return {
        name: {
            key: {
                field: group[key][field] - baseline["groups"][name][key][field]
                for field in (
                    "matches",
                    "false_positives",
                    "false_negatives",
                    "precision",
                    "recall",
                    "f1",
                )
            }
            for key in ("onset_50ms", "onset_100ms", "onset_offset")
        }
        for name, group in report["groups"].items()
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("estimates", type=Path)
    parser.add_argument("--json", required=True, type=Path)
    parser.add_argument("--baseline", type=Path)
    args = parser.parse_args()
    report = score(args.manifest, args.estimates)
    if args.baseline:
        report["delta"] = compare(
            report, json.loads(args.baseline.read_text(encoding="utf-8"))
        )
    with args.json.open("x", encoding="utf-8") as stream:
        json.dump(report, stream, indent=2)
        stream.write("\n")
    for name, group in report["groups"].items():
        onset = group["onset_50ms"]
        print(
            f"{name}: P={onset['precision']:.3f} R={onset['recall']:.3f} F1={onset['f1']:.3f} FP={onset['false_positives']} FN={onset['false_negatives']}"
        )


if __name__ == "__main__":
    main()
