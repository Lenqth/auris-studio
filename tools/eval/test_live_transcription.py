"""Protect live evaluation from grading an altered song or model completion prose."""

import json
from copy import deepcopy

import pytest
from live_transcription import imported_notes, score, tool_result
from piano_transcription import digest


def documents():
    before = {
        "tempo_map": {"points": [{"tick": 0, "bpm": 90}]},
        "tracks": [{"id": 1, "name": "Existing"}],
    }
    after = deepcopy(before)
    after["tracks"].append(
        {
            "id": 2,
            "kind": {
                "type": "instrument",
                "instrument_id": "auris.physical.piano",
                "clips": [
                    {
                        "start": 960,
                        "notes": [
                            {"pitch": 60, "start": 240, "length": 480, "velocity": 0.01}
                        ],
                    }
                ],
            },
        }
    )
    return before, after


def test_grades_actual_import_with_project_timing_and_preserves_weak_dynamics():
    before, after = documents()
    (note,) = imported_notes(before, after)
    assert note == pytest.approx(
        {"pitch": 60, "start": 5 / 6, "end": 7 / 6, "strength": 0.01}
    )


@pytest.mark.parametrize("change", ["tempo", "existing", "instrument", "loop"])
def test_refuses_document_changes_that_would_make_the_comparison_misleading(change):
    before, after = documents()
    if change == "tempo":
        after["tempo_map"]["points"][0]["bpm"] = 120
    elif change == "existing":
        after["tracks"][0]["name"] = "Overwritten"
    elif change == "instrument":
        after["tracks"][1]["kind"]["instrument_id"] = "other"
    else:
        after["tracks"][1]["kind"]["clips"][0]["loop_end"] = 3840
    with pytest.raises(ValueError):
        imported_notes(before, after)


def test_an_answer_or_failed_tool_call_is_not_delivery_acceptance():
    for entries in [
        [{"kind": "assistant", "text": "Imported"}],
        [{"name": "import_transcription", "ok": False}],
    ]:
        with pytest.raises(ValueError, match="successful"):
            tool_result({"entries": entries}, "import_transcription")


def evaluation_files(tmp_path):
    before, after = documents()
    audio = tmp_path / "audio.wav"
    audio.write_bytes(b"frozen source")
    reference = tmp_path / "reference.json"
    reference.write_text(json.dumps({"notes": imported_notes(before, after)}))
    manifest = tmp_path / "manifest.json"
    manifest.write_text(
        json.dumps(
            {
                "cases": [
                    {
                        "id": "case",
                        "variant": "piano",
                        "audio": "audio.wav",
                        "reference": "reference.json",
                    }
                ]
            }
        )
    )
    program = tmp_path / "analysis.py"
    program.write_text("# frozen recognizer\n")
    metrics = {
        "matches": 1,
        "false_positives": 0,
        "false_negatives": 0,
        "precision": 1,
        "recall": 1,
        "f1": 1,
    }
    baseline = tmp_path / "baseline.json"
    baseline.write_text(
        json.dumps(
            {
                "manifest_sha256": digest(manifest),
                "cases": [
                    {
                        "id": "case",
                        "audio_sha256": digest(audio),
                        "reference_sha256": digest(reference),
                        **{
                            k: metrics
                            for k in ("onset_50ms", "onset_100ms", "onset_offset")
                        },
                    }
                ],
            }
        )
    )
    report = {
        k: True
        for k in (
            "exported",
            "imported",
            "dirty_before_host_save",
            "one_undo_restored",
            "redo_restored",
        )
    }
    report.update(
        {
            "project": str(tmp_path / "Cover.auris"),
            "elapsed_seconds": 3,
            "entries": [
                {
                    "name": "export_audio_attachment",
                    "ok": True,
                    "detail": json.dumps({"sha256": digest(audio)}),
                },
                {
                    "name": "import_transcription",
                    "ok": True,
                    "detail": json.dumps(
                        {
                            "notes": 1,
                            "source": {
                                "artifact_sha256": digest(program),
                                "audio_sha256": digest(audio),
                            },
                        }
                    ),
                },
            ],
        }
    )
    result = tmp_path / "result.json"
    for path, value in [
        (result, report),
        (tmp_path / "project-before.json", before),
        (tmp_path / "project-after.json", after),
    ]:
        path.write_text(json.dumps(value))
    return manifest, baseline, result, report


def test_hash_checked_live_scoring_uses_actual_document_notes(tmp_path):
    manifest, baseline, result, _ = evaluation_files(tmp_path)
    measured = score(manifest, baseline, {"case": result})
    assert measured["cases"][0]["estimated_notes"] == 1
    assert measured["totals"]["codex"]["onset_50ms"]["f1"] == 1


@pytest.mark.parametrize("tamper", ["source", "program", "reference", "count", "undo"])
def test_integration_evidence_cannot_silently_drift(tmp_path, tamper):
    manifest, baseline, result, report = evaluation_files(tmp_path)
    if tamper == "source":
        (tmp_path / "audio.wav").write_bytes(b"changed source")
    elif tamper == "program":
        (tmp_path / "analysis.py").write_text("# retuned after scoring\n")
    elif tamper == "reference":
        (tmp_path / "reference.json").write_text('{"notes": []}')
    elif tamper == "count":
        detail = json.loads(report["entries"][1]["detail"])
        detail["notes"] = 2
        report["entries"][1]["detail"] = json.dumps(detail)
    else:
        report["one_undo_restored"] = False
    result.write_text(json.dumps(report))
    with pytest.raises(ValueError):
        score(manifest, baseline, {"case": result})
