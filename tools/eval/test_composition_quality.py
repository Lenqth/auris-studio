import json
from pathlib import Path

import composition_quality as quality
import pytest


def suite(root: Path, *, success=True, model="model"):
    root.mkdir()
    folder = root / "loop_title-01"
    folder.mkdir()
    (folder / "result.wav").write_bytes(b"frozen-audio")
    value = {
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
