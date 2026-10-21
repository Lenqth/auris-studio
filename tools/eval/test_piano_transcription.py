"""Independent event matching and annotation integrity guards."""

import copy
import json

import pytest
from piano_transcription import compare, metric, reference_union, verify_frozen_project


def note(start, end=1.0, pitch=60):
    return {"pitch": pitch, "start": start, "end": end}


def test_matching_reassigns_events_and_counts_duplicates():
    # The first reference can use either estimate; a greedy match would lose the second.
    score = metric([note(0.1), note(0.17)], [note(0.13), note(0.06)], 0.05)
    assert score["matches"] == 2
    score = metric([note(0.1)], [note(0.1), note(0.1)], 0.05)
    assert score["matches"] == 1
    assert score["false_positives"] == 1


def test_exact_pitch_offsets_short_notes_and_missing_notes_are_distinct():
    reference = [note(0.1, 0.2), note(0.5, 0.8, 64)]
    estimate = [note(0.11, 0.7), note(0.5, 0.8, 65)]
    assert metric(reference, estimate, 0.05)["matches"] == 1
    assert metric(reference, estimate, 0.05, True)["matches"] == 0
    assert metric(reference, estimate, 0.05)["false_negatives"] == 1
    assert metric([], estimate, 0.05)["false_positives"] == 2
    assert metric([], [], 0.05)["matches"] == 0


def test_coincident_parts_union_without_merging_distinct_repeated_attacks():
    assert reference_union([note(0.1, 0.4), note(0.105, 0.5), note(0.2, 0.6)]) == [
        note(0.1, 0.5),
        note(0.2, 0.6),
    ]
    with pytest.raises(ValueError, match="nonfinite"):
        metric([note(float("nan"))], [], 0.05)


def test_comparison_rejects_changed_recordings_annotations_and_model():
    report = {
        "schema": 1,
        "suite": "test",
        "matching": "exact",
        "manifest_sha256": "frozen",
        "cases": [
            {
                "id": "case",
                "audio_sha256": "a",
                "reference_sha256": "b",
                "model_sha256": "c",
                "split": "development",
                "variant": "piano",
            }
        ],
        "groups": {},
    }
    assert compare(report, report) == {}
    for field in (
        "audio_sha256",
        "reference_sha256",
        "model_sha256",
        "split",
        "variant",
    ):
        changed = copy.deepcopy(report)
        changed["cases"][0][field] = "changed"
        with pytest.raises(ValueError, match=field):
            compare(changed, report)
    for field in ("schema", "suite", "matching"):
        changed = copy.deepcopy(report)
        changed[field] = "changed"
        with pytest.raises(ValueError, match=field):
            compare(changed, report)


def test_saved_performance_labels_are_verified_independently(tmp_path):
    project = tmp_path / "pair.auris"
    document = {
        "tempo_map": {"points": [{"tick": 0, "bpm": 120}]},
        "tracks": [
            {
                "mixer": {"mute": False},
                "kind": {
                    "type": "instrument",
                    "clips": [
                        {
                            "start": 960,
                            "notes": [{"pitch": 60, "start": 480, "length": 240}],
                        }
                    ],
                },
            }
        ],
    }
    project.write_text(json.dumps(document), encoding="utf-8")
    assert len(verify_frozen_project(project, [note(0.75, 0.875)])) == 64
    with pytest.raises(ValueError, match="disagree"):
        verify_frozen_project(project, [note(0.8, 0.9)])
    document["tracks"][0]["kind"]["clips"][0]["transforms"] = [{"swing": 60}]
    project.write_text(json.dumps(document), encoding="utf-8")
    with pytest.raises(ValueError, match="frozen"):
        verify_frozen_project(project, [note(0.75, 0.875)])
