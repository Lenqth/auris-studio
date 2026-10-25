import json
from argparse import Namespace
from pathlib import Path

import composition_quality as quality
import pytest


def suite(root: Path, *, success=True, model="model"):
    root.mkdir()
    folder = root / "loop_title-01"
    folder.mkdir()
    (folder / "result.wav").write_bytes(b"frozen-audio")
    value = {
        "manifest": {
            "schema_version": 1,
            "axes": quality.AXES,
            "cases": [
                {
                    "id": "loop_title",
                    "split": "development",
                    "fixture": "blank",
                    "prompts": ["a short loop"],
                }
            ],
        },
        "manifest_sha256": "manifest",
        "model": model,
        "backend": "codex",
        "split": "development",
        "repetitions": 1,
        "selected_cases": ["loop_title"],
        "trials": [
            {
                "id": "loop_title-01",
                "transport_pass": success,
                "delivery_present": success,
                "artifacts": {"result.wav": quality.digest(folder / "result.wav")},
            }
        ],
    }
    quality.write_json(root / "suite.json", value)
    return value


def test_manifest_keeps_development_and_holdout_cohorts_separate():
    data = quality.manifest(quality.MANIFEST)
    assert sum(c["split"] == "development" for c in data["cases"]) == 8
    assert sum(c["split"] == "holdout" for c in data["cases"]) == 4


def test_runtime_smoke_uses_versioned_eight_bar_fixture():
    data = quality.manifest(
        quality.MANIFEST.with_name("composition-runtime-smoke-v2.json")
    )
    assert data["cases"][0]["fixture"] == "melody_six_eight_eight_bars"


def test_changed_audio_cannot_be_substituted_after_scoring(tmp_path):
    before, after = tmp_path / "before", tmp_path / "after"
    suite(before)
    suite(after)
    (after / "loop_title-01/result.wav").write_bytes(b"replacement")
    with pytest.raises(ValueError, match="Changed listening artifact"):
        quality.pairs(before, after, 42)


def test_comparison_refuses_changed_model_conditions(tmp_path):
    before, after = tmp_path / "before", tmp_path / "after"
    suite(before)
    suite(after, model="different")
    with pytest.raises(ValueError, match="model"):
        quality.pairs(before, after, 42)


def test_failed_trials_remain_visible_in_comparison(tmp_path):
    before, after = tmp_path / "before", tmp_path / "after"
    suite(before)
    suite(after, success=False)
    assert quality.pairs(before, after, 42) == [
        {"trial": "loop_title-01", "status": "failed_or_missing_delivery"}
    ]


def test_missing_trial_is_not_silently_removed(tmp_path):
    before, after = tmp_path / "before", tmp_path / "after"
    suite(before)
    data = suite(after)
    data["trials"] = []
    quality.write_json(after / "suite.json", data)
    with pytest.raises(ValueError, match="cohorts"):
        quality.pairs(before, after, 42)


def test_pair_order_is_reproducible_and_does_not_modify_sources(tmp_path):
    before, after = tmp_path / "before", tmp_path / "after"
    suite(before)
    suite(after)
    result = quality.pairs(before, after, 42)
    assert result == quality.pairs(before, after, 42)
    assert {x["condition"] for x in result[0]["audio"]} == {"before", "after"}
    assert json.loads((before / "suite.json").read_text())["trials"][0][
        "transport_pass"
    ]


def test_matching_but_incomplete_cohorts_cannot_hide_interrupted_runs(tmp_path):
    before, after = tmp_path / "before", tmp_path / "after"
    for root in [before, after]:
        data = suite(root)
        data["repetitions"] = 2
        quality.write_json(root / "suite.json", data)
    with pytest.raises(ValueError, match="Incomplete trial cohorts"):
        quality.pairs(before, after, 42)


def test_frozen_source_uses_original_provenance_and_rejects_changed_runtime(tmp_path):
    reference = tmp_path / "suite.json"
    original = {
        "runtime": {"test.exe": "original-binary"},
        "source_commit": "original-commit",
        "source_diff": "original-diff",
        "untracked_rust_sources": {"draft.rs": "original source"},
    }
    quality.write_json(reference, original)
    snapshot = tmp_path / "source-suite.json"
    bound = quality.frozen_source(reference, original["runtime"], snapshot)
    assert bound["source_commit"] == "original-commit"
    assert bound["untracked_rust_sources"] == original["untracked_rust_sources"]
    assert bound["source_reference_sha256"] == quality.digest(reference)
    assert snapshot.read_bytes() == reference.read_bytes()
    quality.write_json(reference, original | {"trials": [{"id": "later-result"}]})
    assert bound["source_reference_sha256"] == quality.digest(snapshot)
    assert quality.digest(reference) != quality.digest(snapshot)
    with pytest.raises(ValueError, match="runtime"):
        quality.frozen_source(reference, {"test.exe": "different-binary"})


def test_old_manifest_without_listening_rubric_remains_valid(tmp_path):
    path = tmp_path / "old.json"
    quality.write_json(
        path,
        {
            "schema_version": 1,
            "axes": quality.AXES,
            "cases": [
                {
                    "id": "case",
                    "split": "development",
                    "fixture": "blank",
                    "prompts": ["a brief"],
                }
            ],
        },
    )
    assert "listening_rubric" not in quality.manifest(path)


@pytest.mark.parametrize(
    "rubric",
    [
        {"unknown": ["question"]},
        {"melody": []},
        {"melody": [" "]},
        {"melody": "question"},
        {"melody": ["question", 3]},
    ],
)
def test_manifest_rejects_malformed_listening_rubric(tmp_path, rubric):
    path = tmp_path / "bad.json"
    quality.write_json(
        path,
        {
            "schema_version": 1,
            "axes": quality.AXES,
            "listening_rubric": rubric,
            "cases": [
                {
                    "id": "case",
                    "split": "development",
                    "fixture": "blank",
                    "prompts": ["a brief"],
                }
            ],
        },
    )
    with pytest.raises(ValueError, match="Listening rubric"):
        quality.manifest(path)


def test_blind_page_renders_escaped_rubric_questions(tmp_path):
    before, after = tmp_path / "before", tmp_path / "after"
    before_data = suite(before)
    after_data = suite(after)
    question = '<strong>Hear the lead & "hook"?</strong>'
    for data, root in [(before_data, before), (after_data, after)]:
        data["manifest"]["listening_rubric"] = {"melody": [question]}
        quality.write_json(root / "suite.json", data)
    output = tmp_path / "listening"
    quality.blind(Namespace(before=before, after=after, output=output, seed=42))
    page = (output / "listening.html").read_text(encoding="utf-8")
    assert "Hear the lead &amp; &quot;hook&quot;?" in page
    assert "<strong>Hear the lead" not in page
    assert 'data-axis="melody"' in page
