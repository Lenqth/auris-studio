"""Score actual in-window note imports against a hash-verified Basic Pitch baseline."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from piano_transcription import counts, digest, metric, reference_union


def imported_notes(before: dict, after: dict) -> list[dict]:
    """Extract only a newly added piano track and reject edits that invalidate timing."""
    if before["tempo_map"] != after["tempo_map"]:
        raise ValueError("project tempo changed")
    old = {track["id"]: track for track in before["tracks"]}
    current = {track["id"]: track for track in after["tracks"]}
    if any(current.get(key) != track for key, track in old.items()):
        raise ValueError("existing music changed")
    added = [track for track in after["tracks"] if track["id"] not in old]
    if len(added) != 1:
        raise ValueError("expected exactly one new piano track")
    kind = added[0]["kind"]
    if kind["type"] != "instrument" or kind["instrument_id"] != "auris.physical.piano":
        raise ValueError("expected a physical-piano instrument")
    points = after["tempo_map"]["points"]
    if len(points) != 1 or points[0]["tick"] != 0:
        raise ValueError("this evaluation requires a constant project tempo")
    seconds_per_tick = 60.0 / points[0]["bpm"] / 960
    notes = []
    for clip in kind["clips"]:
        if any(clip.get(key) for key in ("muted", "transforms", "recipe", "loop_end")):
            raise ValueError("imported clip has unaccounted performance modifiers")
        for note in clip["notes"]:
            start = clip["start"] + note["start"]
            notes.append(
                {
                    "pitch": note["pitch"],
                    "start": start * seconds_per_tick,
                    "end": (start + note["length"]) * seconds_per_tick,
                    "strength": note["velocity"],
                }
            )
    return notes


def tool_result(report: dict, name: str) -> dict:
    """Require a successful real tool result, never a model's completion claim."""
    matches = [
        entry
        for entry in report["entries"]
        if entry.get("name") == name and entry.get("ok") is True
    ]
    if len(matches) != 1:
        raise ValueError(f"expected one successful {name} call")
    return json.loads(matches[0]["detail"])


def score(manifest: Path, baseline: Path, reports: dict[str, Path]) -> dict:
    """Verify source/provenance/acceptance, then score the notes in the actual document."""
    corpus = json.loads(manifest.read_text(encoding="utf-8"))
    basic = json.loads(baseline.read_text(encoding="utf-8"))
    if basic["manifest_sha256"] != digest(manifest):
        raise ValueError("baseline manifest changed")
    if set(reports) != {case["id"] for case in corpus["cases"]}:
        raise ValueError("case cohort changed")
    basic_cases = {case["id"]: case for case in basic["cases"]}
    cases = []
    for case in corpus["cases"]:
        path = reports[case["id"]]
        report = json.loads(path.read_text(encoding="utf-8"))
        for key in (
            "exported",
            "imported",
            "dirty_before_host_save",
            "one_undo_restored",
            "redo_restored",
        ):
            if report.get(key) is not True:
                raise ValueError(f"{case['id']}: acceptance failed: {key}")
        if report.get("failure"):
            raise ValueError("independent agent import did not complete")
        source = tool_result(report, "export_audio_attachment")
        imported = tool_result(report, "import_transcription")
        audio_hash = digest(manifest.parent / case["audio"])
        reference_path = manifest.parent / case["reference"]
        reference_hash = digest(reference_path)
        old = basic_cases[case["id"]]
        if source["sha256"] != audio_hash or old["audio_sha256"] != audio_hash:
            raise ValueError("source audio changed")
        if old["reference_sha256"] != reference_hash:
            raise ValueError("reference changed")
        program = Path(report["project"]).parent / "analysis.py"
        program_hash = digest(program)
        if imported["source"].get("artifact_sha256") != program_hash:
            raise ValueError("declared program hash differs from actual program")
        if imported["source"].get("audio_sha256") != audio_hash:
            raise ValueError("declared source hash differs from exported source")
        before = json.loads(
            (path.parent / "project-before.json").read_text(encoding="utf-8")
        )
        after = json.loads(
            (path.parent / "project-after.json").read_text(encoding="utf-8")
        )
        notes = imported_notes(before, after)
        if imported["notes"] != len(notes):
            raise ValueError("reported/imported note counts disagree")
        reference = reference_union(
            json.loads(reference_path.read_text(encoding="utf-8"))["notes"]
        )
        measured = {
            "onset_50ms": metric(reference, notes, 0.05),
            "onset_100ms": metric(reference, notes, 0.1),
            "onset_offset": metric(reference, notes, 0.05, True),
        }
        cases.append(
            {
                "id": case["id"],
                "variant": case["variant"],
                "audio_sha256": audio_hash,
                "reference_sha256": reference_hash,
                "program_sha256": program_hash,
                "report_sha256": digest(path),
                "reference_notes": len(reference),
                "estimated_notes": len(notes),
                "elapsed_seconds": report["elapsed_seconds"],
                "source": imported["source"],
                "limitations": imported.get("limitations"),
                "codex": measured,
                "basic_pitch": {key: old[key] for key in measured},
            }
        )
    if len({case["program_sha256"] for case in cases}) != 1:
        raise ValueError("recognition program changed between recordings")
    totals = {}
    for method in ("codex", "basic_pitch"):
        totals[method] = {}
        for key in ("onset_50ms", "onset_100ms", "onset_offset"):
            totals[method][key] = counts(
                *(
                    sum(case[method][key][field] for case in cases)
                    for field in ("matches", "false_positives", "false_negatives")
                )
            )
    return {
        "schema": 1,
        "manifest_sha256": digest(manifest),
        "cases": cases,
        "totals": totals,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("baseline", type=Path)
    parser.add_argument(
        "--case", action="append", required=True, help="case-id=absolute-result.json"
    )
    parser.add_argument("--json", required=True, type=Path)
    args = parser.parse_args()
    reports = dict(value.split("=", 1) for value in args.case)
    report = score(
        args.manifest,
        args.baseline,
        {key: Path(value) for key, value in reports.items()},
    )
    with args.json.open("x", encoding="utf-8") as output:
        json.dump(report, output, indent=2)
    for case in report["cases"]:
        print(
            f"{case['id']}: Codex={case['codex']['onset_50ms']['f1']:.4f} BasicPitch={case['basic_pitch']['onset_50ms']['f1']:.4f}"
        )


if __name__ == "__main__":
    main()
