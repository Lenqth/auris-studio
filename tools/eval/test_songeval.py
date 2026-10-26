"""Unit tests for the SongEval manifest adapter (no model download/inference)."""

import importlib.util
import json
from pathlib import Path

import numpy as np
import soundfile as sf

SPEC = importlib.util.spec_from_file_location("songeval", Path(__file__).with_name("songeval.py"))
assert SPEC and SPEC.loader
songeval = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(songeval)


def wav(path: Path, value: float) -> None:
    sf.write(path, np.full((4800, 1), value, dtype=np.float32), 48000, subtype="PCM_16")


def test_manifest_assigns_unique_ids_and_hashes(tmp_path: Path):
    first, second = tmp_path / "a.wav", tmp_path / "b.wav"
    wav(first, 0.1)
    wav(second, 0.2)
    manifest = tmp_path / "manifest.json"
    manifest.write_text(json.dumps([
        {"path": str(first), "id": "mix", "group": "one", "condition": "before"},
        {"path": str(second), "id": "mix", "group": "one", "condition": "after"},
    ]), encoding="utf-8")
    rows = songeval.load_manifest(manifest)
    assert [row["id"] for row in rows] == ["mix", "mix-2"]
    assert all(len(row["sha256"]) == 64 for row in rows)


def test_diagnostic_marks_silence_inadmissible(tmp_path: Path):
    path = tmp_path / "silence.wav"
    wav(path, 0.0)
    result = songeval.audio_diagnostic(path)
    assert result["admissible"] is False
    assert result["silence_reason"] == "silent_or_empty"


def test_comparisons_are_deltas_and_do_not_select_winner():
    rows = [
        {"id": "base", "group": "g", "condition": "before", "status": "scored", "scores": {axis: 1.0 for axis in songeval.AXES}},
        {"id": "candidate", "group": "g", "condition": "after", "status": "scored", "scores": {axis: 1.25 for axis in songeval.AXES}},
    ]
    result = songeval._comparisons(rows)
    assert result == [{
        "group": "g",
        "status": "paired",
        "members": ["base", "candidate"],
        "delta_after_minus_before": {axis: 0.25 for axis in songeval.AXES},
        "selection": "none",
    }]


def test_comparisons_reject_duplicate_conditions():
    rows = [
        {"id": "a", "group": "g", "condition": "before", "status": "scored"},
        {"id": "b", "group": "g", "condition": "before", "status": "scored"},
    ]
    try:
        songeval._comparisons(rows)
    except ValueError as error:
        assert "duplicate" in str(error)
    else:
        raise AssertionError("duplicate comparison conditions must fail closed")


def test_diagnostic_rejects_overlong_audio(tmp_path: Path):
    path = tmp_path / "long.wav"
    wav(path, 0.1)
    result = songeval.audio_diagnostic(path, max_seconds=0.01)
    assert result["admissible"] is False
    assert result["silence_reason"] == "too_long"
