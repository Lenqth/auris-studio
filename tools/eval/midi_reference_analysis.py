#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["mido==1.3.3", "openpyxl==3.1.5"]
# ///
"""Describe explicitly labelled MIDI melody tracks (never guess a melody)."""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import itertools
import json
import statistics
import subprocess
from pathlib import Path

import mido
import openpyxl

Note = tuple[int, int, int, int]


def _parse_notes(track: mido.MidiTrack) -> tuple[list[Note], dict[str, int]]:
    """Parse events and retain malformed-event diagnostics rather than hiding them."""
    now = 0
    active: dict[tuple[int, int], list[tuple[int, int]]] = {}
    notes: list[Note] = []
    unmatched_note_off = 0
    overlap_pairs = 0
    zero_duration = 0
    for message in track:
        now += message.time
        if message.type == "note_on" and message.velocity:
            key = (message.channel, message.note)
            if active.get(key):
                overlap_pairs += 1
            active.setdefault(key, []).append((now, message.velocity))
        elif message.type in {"note_off", "note_on"}:
            key = (message.channel, message.note)
            starts = active.get(key)
            if not starts:
                unmatched_note_off += 1
                continue
            start, velocity = starts.pop(0)
            if now > start:
                notes.append((message.note, start, now, velocity))
            else:
                zero_duration += 1
    sounding = []
    for _, start, end, _ in notes:
        sounding.extend(((start, 1), (end, -1)))
    voices = 0
    max_voices = 0
    for _, delta in sorted(sounding, key=lambda event: (event[0], event[1])):
        voices += delta
        max_voices = max(max_voices, voices)
    return sorted(notes, key=lambda note: (note[1], note[0])), {
        "unmatched_note_off": unmatched_note_off,
        "unclosed_note_on": sum(len(values) for values in active.values()),
        "zero_duration_note": zero_duration,
        "same_pitch_overlap_pairs": overlap_pairs,
        "max_simultaneous_voices": max_voices,
    }


def _named_track(
    midi: mido.MidiFile, track_name: str
) -> tuple[int, mido.MidiTrack, list[str]]:
    candidates = []
    for index, track in enumerate(midi.tracks):
        names = [message.name for message in track if message.type == "track_name"]
        if track_name.casefold() in {name.casefold() for name in names}:
            candidates.append((index, track, names))
    if len(candidates) != 1:
        raise ValueError(
            f"expected exactly one {track_name!r} track, found {len(candidates)}"
        )
    return candidates[0]


def _union_gaps(notes: list[Note]) -> list[int]:
    """Return silent gaps between the union of note intervals, in ticks."""
    spans = sorted((start, end) for _, start, end, _ in notes)
    merged: list[list[int]] = []
    for start, end in spans:
        if merged and start <= merged[-1][1]:
            merged[-1][1] = max(merged[-1][1], end)
        else:
            merged.append([start, end])
    return [
        start - previous_end
        for (_, previous_end), (start, _) in itertools.pairwise(merged)
    ]


def analyze_file(path: str | Path, track_name: str = "MELODY") -> dict[str, object]:
    """Analyze one file; units are quarter notes even when meter is compound."""
    midi = mido.MidiFile(path)
    if midi.type == 2:
        raise ValueError("type-2 MIDI files are unsupported")
    if midi.ticks_per_beat <= 0:
        raise ValueError("SMPTE/invalid timebase is unsupported")
    index, track, names = _named_track(midi, track_name)
    notes, diagnostics = _parse_notes(track)
    if not notes:
        raise ValueError("melody track has no closed notes")
    pitches = [note[0] for note in notes]
    onsets = [note[1] for note in notes]
    intervals = [b - a for a, b in itertools.pairwise(pitches)]
    nonzero = [abs(value) for value in intervals if value]
    durations = [end - start for _, start, end, _ in notes]
    simultaneous_onset = any(a == b for a, b in itertools.pairwise(onsets))
    gaps = _union_gaps(notes)
    positive_gaps = [gap for gap in gaps if gap > 0]
    quarter = midi.ticks_per_beat
    diagnostics_bad = any(
        diagnostics[key]
        for key in ("unmatched_note_off", "unclosed_note_on", "zero_duration_note")
    )
    interval_valid = not simultaneous_onset and not diagnostics_bad
    interval_metrics = (
        {
            "mean_abs_nonzero_interval_semitones": round(statistics.mean(nonzero), 4)
            if nonzero
            else None,
            "small_motion_fraction": round(
                sum(0 < abs(value) <= 2 for value in intervals) / len(intervals), 4
            )
            if intervals
            else None,
            "unison_fraction": round(
                sum(value == 0 for value in intervals) / len(intervals), 4
            )
            if intervals
            else None,
        }
        if interval_valid
        else {
            "mean_abs_nonzero_interval_semitones": None,
            "small_motion_fraction": None,
            "unison_fraction": None,
        }
    )
    return {
        "file": str(path),
        "sha256": hashlib.sha256(Path(path).read_bytes()).hexdigest(),
        "track_index": index,
        "track_names": names,
        "ticks_per_quarter": quarter,
        "notes": len(notes),
        "pitch_min": min(pitches),
        "pitch_max": max(pitches),
        "pitch_range_semitones": max(pitches) - min(pitches),
        "pitch_median": statistics.median(pitches),
        **interval_metrics,
        "duration_median_quarters": round(statistics.median(durations) / quarter, 4),
        "duration_mean_quarters": round(statistics.mean(durations) / quarter, 4),
        "positive_union_gap_count": len(positive_gaps),
        "positive_union_gap_fraction_ge_1_quarter": round(
            sum(gap >= quarter for gap in positive_gaps) / len(positive_gaps), 4
        )
        if positive_gaps
        else None,
        "median_positive_union_gap_quarters": round(
            statistics.median(positive_gaps) / quarter, 4
        )
        if positive_gaps
        else None,
        "simultaneous_onset": simultaneous_onset,
        "interval_metrics_valid": interval_valid,
        "status": "suspect" if diagnostics_bad or simultaneous_onset else "valid",
        "diagnostics": diagnostics,
    }


def _index_metadata(index_path: Path) -> dict[str, dict[str, str]]:
    if not index_path.exists():
        return {}
    workbook = openpyxl.load_workbook(index_path, read_only=True, data_only=True)
    try:
        rows = iter(workbook.active.values)
        headers = [str(value) for value in next(rows)]
        result = {}
        for values in rows:
            record = dict(zip(headers, values))
            if record.get("song_id") is None:
                continue
            song_id = str(record["song_id"]).zfill(3)
            result[song_id] = {
                dest: str(record[source])
                for source, dest in (("name", "title"), ("artist", "artist"))
                if record.get(source) is not None
            }
        return result
    finally:
        workbook.close()


def analyze_paths(
    root: Path, paths: list[Path], track_name: str, index_path: Path | None
) -> list[dict[str, object]]:
    metadata = _index_metadata(index_path) if index_path else {}
    rows = []
    for path in paths:
        try:
            row = analyze_file(path, track_name)
            row["file"] = path.relative_to(root).as_posix()
            row["source_index"] = metadata.get(path.parent.name, {})
            rows.append(row)
        except (OSError, ValueError, EOFError) as error:
            rows.append(
                {"file": path.relative_to(root).as_posix(), "error": str(error)}
            )
    return rows


def _git(root: Path, *args: str) -> str | None:
    try:
        return subprocess.run(
            ["git", "-C", str(root), *args], capture_output=True, text=True, check=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def _summary(rows: list[dict[str, object]]) -> dict[str, object]:
    valid = [row for row in rows if row.get("interval_metrics_valid")]
    fields = [
        "notes",
        "pitch_range_semitones",
        "mean_abs_nonzero_interval_semitones",
        "small_motion_fraction",
        "unison_fraction",
        "duration_median_quarters",
        "positive_union_gap_fraction_ge_1_quarter",
    ]
    result: dict[str, object] = {
        "files_total": len(rows),
        "files_valid_intervals": len(valid),
        "files_suspect_or_error": len(rows) - len(valid),
    }
    for field in fields:
        values = [
            row[field] for row in valid if isinstance(row.get(field), (int, float))
        ]
        result[field] = (
            {
                "files": len(values),
                "mean": round(statistics.mean(values), 4),
                "median": round(statistics.median(values), 4),
                "min": min(values),
                "max": max(values),
            }
            if values
            else None
        )
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pattern", default="**/*.mid")
    parser.add_argument("--track", default="MELODY")
    parser.add_argument(
        "--index",
        type=Path,
        help="POP909/index.xlsx (defaults to root/POP909/index.xlsx)",
    )
    parser.add_argument(
        "--limit", type=int, help="Analyze only the first N sorted matching files"
    )
    args = parser.parse_args()
    if args.limit is not None and args.limit < 1:
        parser.error("--limit must be positive")
    index = args.index if args.index else args.root / "POP909" / "index.xlsx"
    paths = sorted(args.root.glob(args.pattern))
    if args.limit is not None:
        paths = paths[: args.limit]
    if not paths:
        parser.error("No files match the selection")
    rows = analyze_paths(args.root, paths, args.track, index)
    provenance = {
        "repository": _git(args.root, "remote", "get-url", "origin"),
        "commit": _git(args.root, "rev-parse", "HEAD"),
        "working_tree_status": _git(args.root, "status", "--porcelain"),
        "pattern": args.pattern,
        "limit": args.limit,
        "files": [row["file"] for row in rows],
        "tool": "mido " + importlib.metadata.version("mido"),
        "index_parser": "openpyxl " + importlib.metadata.version("openpyxl"),
        "analyzer_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    }
    if index.exists():
        provenance["index_sha256"] = hashlib.sha256(index.read_bytes()).hexdigest()
    payload = {
        "schema_version": 3,
        "source": {
            **provenance,
        },
        "track": args.track,
        "summary": _summary(rows),
        "files": rows,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
