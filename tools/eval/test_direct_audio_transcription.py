"""Prevent malformed or invented-time model replies becoming accepted drafts."""

import io
import json
from argparse import Namespace

import pytest
from direct_audio_transcription import PROMPT, infer, parse_notes, score
from piano_transcription import digest


@pytest.mark.parametrize(
    "note",
    [
        {"pitch": True, "start": 0, "end": 1},
        {"pitch": 60.5, "start": 0, "end": 1},
        {"pitch": 128, "start": 0, "end": 1},
        {"pitch": 60, "start": "0", "end": 1},
        {"pitch": 60, "start": 0, "end": float("nan")},
        {"pitch": 60, "start": 0, "end": 10},
    ],
)
def test_invalid_or_out_of_audio_notes_are_rejected(note):
    with pytest.raises(ValueError):
        parse_notes(json.dumps({"notes": [note]}), 6)


def test_empty_notes_are_valid_but_prose_and_missing_arrays_are_not():
    assert parse_notes('{"notes": []}', 6) == []
    for content in ("{}", "I heard a piano", '```json\n{"notes": []}\n```'):
        with pytest.raises((ValueError, TypeError)):
            parse_notes(content, 6)


def test_inference_does_not_read_labels_or_transmit_case_names(tmp_path, monkeypatch):
    manifest = tmp_path / "manifest.json"
    manifest.write_text(
        json.dumps(
            {
                "cases": [
                    {
                        "id": "secret-composition",
                        "split": "held_out",
                        "audio": "input.wav",
                        "reference": "does-not-exist.json",
                        "seconds": 6,
                    }
                ]
            }
        )
    )
    (tmp_path / "input.wav").write_bytes(b"opaque audio")
    observed = []

    def post(request, timeout):
        observed.append(json.loads(request.data))
        return io.BytesIO(
            json.dumps(
                {
                    "choices": [
                        {
                            "finish_reason": "stop",
                            "message": {"content": '{"notes": []}'},
                        }
                    ]
                }
            ).encode()
        )

    monkeypatch.setattr("urllib.request.urlopen", post)
    infer(
        Namespace(
            manifest=manifest,
            output=tmp_path / "results",
            model="test",
            url="http://localhost/v1",
            split="held_out",
            max_tokens=100,
            timeout=1,
            text_first=False,
            unconstrained=False,
        )
    )
    assert observed[0]["messages"][0]["content"][1]["text"] == PROMPT
    assert "secret-composition" not in json.dumps(observed)
    assert "does-not-exist.json" not in json.dumps(observed)


def test_failed_draft_keeps_raw_metrics_and_failures_distinct(tmp_path):
    audio = tmp_path / "input.wav"
    audio.write_bytes(b"opaque audio")
    manifest = tmp_path / "manifest.json"
    manifest.write_text(
        json.dumps(
            {
                "cases": [
                    {
                        "id": "sample",
                        "split": "control",
                        "variant": "melody",
                        "audio": audio.name,
                        "reference": "reference.json",
                        "seconds": 1,
                    }
                ]
            }
        )
    )
    (tmp_path / "reference.json").write_text(
        json.dumps({"notes": [{"pitch": 60, "start": 0, "end": 0.5}]})
    )
    estimates = tmp_path / "estimates"
    estimates.mkdir()
    (estimates / "run.json").write_text(
        json.dumps({"cases": ["sample"], "manifest_sha256": digest(manifest)})
    )
    (estimates / "sample.json").write_text(
        json.dumps(
            {
                "status": "failed",
                "elapsed_seconds": 1,
                "audio_sha256": digest(audio),
                "response": {
                    "choices": [
                        {
                            "message": {
                                "content": json.dumps(
                                    {"notes": [{"pitch": 60, "start": 0, "end": 7}]}
                                )
                            }
                        }
                    ]
                },
            }
        )
    )
    output = tmp_path / "score.json"
    score(
        Namespace(manifest=manifest, estimates=estimates, output=output, baseline=None)
    )
    group = json.loads(output.read_text())["groups"]["all"]
    assert group["failed"] == 1
    assert group["empty_drafts"] == 0
    assert group["onset_50ms"]["false_negatives"] == 1
    assert group["raw_onset_50ms"]["matches"] == 1
