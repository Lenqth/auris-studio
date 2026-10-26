"""Focused guards for the bounded SongEval refinement orchestration."""

from __future__ import annotations

import copy
import json
from pathlib import Path
from types import SimpleNamespace

import composition_refine as refine
import pytest
from composition_refine import (
    audit_edit,
    compare_scores,
    extract_scores,
    recipe,
    validate_notes,
)


def project() -> dict:
    """Return a tiny singer project with protected backing state."""
    return {
        "tracks": [
            {
                "id": 1,
                "name": "Lead",
                "kind": {
                    "type": "singer",
                    "clips": [
                        {
                            "id": 7,
                            "length": 960,
                            "lyrics": ["la", "la"],
                            "notes": [
                                {"pitch": 60, "start": 0, "length": 480},
                                {"pitch": 62, "start": 520, "length": 400},
                            ],
                        }
                    ],
                },
                "mixer": {"gain_db": -2, "effects": [{"id": "vocal"}]},
            },
            {
                "id": 2,
                "name": "Backing",
                "kind": {
                    "type": "instrument",
                    "clips": [{"id": 8, "length": 960, "notes": []}],
                },
                "mixer": {"gain_db": -8, "effects": [{"id": "delay"}]},
            },
        ],
        "tempo": 112,
        "audio_sources": {"voice": {"path": "Audio/voice.wav"}},
        "harmony": [{"bar": 1, "chord": "C"}],
    }


def step(**overrides) -> dict:
    value = {
        "instruction": "Make the second phrase answer the first with a short anticipation.",
        "fields": ["start"],
        "max_changed_notes": 1,
    }
    value.update(overrides)
    return value


def test_note_validation_rejects_overlap_and_out_of_bounds():
    value = project()
    value["tracks"][0]["kind"]["clips"][0]["notes"][1]["start"] = 200
    with pytest.raises(ValueError, match="overlap"):
        validate_notes(value, "Lead")


def test_audit_rejects_protected_lyrics_backing_and_mixer_changes():
    before = project()
    for mutate in (
        lambda value: value["tracks"][0]["kind"]["clips"][0].update(lyrics=["changed"]),
        lambda value: value["tracks"][1]["mixer"].update(gain_db=-1),
        lambda value: value["tracks"][0]["mixer"]["effects"].append({"id": "reverb"}),
    ):
        after = copy.deepcopy(before)
        mutate(after)
        with pytest.raises(ValueError, match="protected"):
            audit_edit(before, after, "Lead", step())


def test_audit_allows_declared_note_change_and_rejects_budget():
    before = project()
    after = copy.deepcopy(before)
    after["tracks"][0]["kind"]["clips"][0]["notes"][1]["start"] = 500
    result = audit_edit(before, after, "Lead", step())
    assert result["valid"] is True
    assert result["changed_notes"] == 1
    assert result["changes"][0]["fields"]["start"] == {"before": 520, "after": 500}

    over_budget = copy.deepcopy(after)
    over_budget["tracks"][0]["kind"]["clips"][0]["notes"][0]["start"] = 20
    with pytest.raises(ValueError, match="budget"):
        audit_edit(before, over_budget, "Lead", step())


def test_recipe_is_bounded_and_rejects_unknown_fields(tmp_path):
    valid = {
        "schema_version": 1,
        "brief": "Compose a short pop vocal.",
        "focus_track": "Lead",
        "interventions": [step(), step(fields=["pitch", "length"], max_changed_notes=2)],
    }
    path = tmp_path / "recipe.json"
    path.write_text(json.dumps(valid), encoding="utf-8")
    assert recipe(path)["focus_track"] == "Lead"
    invalid = copy.deepcopy(valid)
    invalid["interventions"] = [step(fields=["lyrics"])]
    path.write_text(json.dumps(invalid), encoding="utf-8")
    with pytest.raises(ValueError, match="fields"):
        recipe(path)


def test_score_contract_has_five_axes_and_never_aggregates(tmp_path):
    axes = {
        "Coherence": 3.1,
        "Musicality": 3.2,
        "Memorability": 3.3,
        "Clarity": 3.4,
        "Naturalness": 3.5,
    }
    report = {
        "runtime": {"preprocessing": "fixed", "model": "pinned"},
        "rows": [{"sha256": "abc", "status": "scored", "diagnostics": {"admissible": True}, "scores": axes}],
    }
    extracted = extract_scores(report, "abc")
    delta = compare_scores(extracted, {"scores": {k: v + 0.1 for k, v in axes.items()}, "conditions": extracted["conditions"], "audio_sha256": "def"})
    assert set(delta) == set(axes)
    assert "total" not in delta
    bad = copy.deepcopy(report)
    bad["rows"][0]["scores"].pop("Clarity")
    with pytest.raises((KeyError, ValueError)):
        extract_scores(bad, "abc")


def test_extract_scores_rejects_wrong_audio_and_nonfinite_runtime():
    axes = {name: 3.0 for name in ("Coherence", "Musicality", "Memorability", "Clarity", "Naturalness")}
    report = {
        "runtime": {"revision": "pinned"},
        "rows": [{"sha256": "abc", "status": "scored", "diagnostics": {"admissible": True}, "scores": axes}],
    }
    with pytest.raises(ValueError, match="retained mix"):
        extract_scores(report, "different")
    bad = copy.deepcopy(report)
    bad["rows"][0]["scores"]["Naturalness"] = float("nan")
    with pytest.raises(ValueError, match="Invalid SongEval"):
        extract_scores(bad, "abc")
    with pytest.raises(ValueError, match="conditions"):
        compare_scores(
            extract_scores(report, "abc"),
            {"scores": axes, "conditions": {"revision": "changed"}},
        )


def _fake_args(tmp_path: Path, recipe_path: Path) -> SimpleNamespace:
    executable = tmp_path / "panel.exe"
    executable.write_bytes(b"panel")
    return SimpleNamespace(
        recipe=recipe_path,
        output=tmp_path / "run",
        executable=executable,
        auris=executable,
        evaluator_python=executable,
        songeval_config=recipe_path,
        seed_project=None,
        source_suite=None,
        voice=None,
        speaker=None,
        model="fake",
        backend="codex",
        prompt_profile="legacy",
        timeout=60,
    )


def _patch_fake_pipeline(monkeypatch, revisions, evaluations):
    calls = []
    evaluated = []

    def freeze_runtime(source, destination):
        destination.mkdir()
        target = destination / source.name
        target.write_bytes(source.read_bytes())
        return target

    def run_stage(args, root, prompt, seed):
        index = len(calls)
        calls.append(seed)
        folder = root / "agent" / "song-01"
        folder.mkdir(parents=True)
        value = copy.deepcopy(project()) if index == 0 else copy.deepcopy(revisions[index - 1])
        snapshot = folder / "project-02.json"
        snapshot.write_text(json.dumps(value), encoding="utf-8")
        return snapshot

    def fake_render(args, snapshot, source, destination, focus_name):
        destination.mkdir(parents=True)
        assets = {}
        for mode in ("mix", "foreground", "backing"):
            path = destination / f"{mode}.wav"
            path.write_bytes(mode.encode())
            assets[mode] = {"path": str(path), "sha256": "same", "seconds": 1.0, "sample_rate": 1}
        return {"assets": assets}

    def fake_evaluate(args, stage_root, asset):
        evaluated.append(stage_root.name)
        return {"scores": evaluations[len(calls) - 1], "conditions": {"revision": "fixed"}, "audio_sha256": "same"}

    monkeypatch.setattr(refine, "freeze_runtime", freeze_runtime)
    monkeypatch.setattr(refine, "run_stage", run_stage)
    monkeypatch.setattr(refine, "render", fake_render)
    monkeypatch.setattr(refine, "evaluate", fake_evaluate)
    return calls, evaluated


def test_run_never_promotes_score_winner_and_revisions_branch_baseline(tmp_path, monkeypatch):
    recipe_value = {"schema_version": 1, "brief": "Compose", "focus_track": "Lead", "interventions": [step(), step()]}
    recipe_path = tmp_path / "recipe.json"
    recipe_path.write_text(json.dumps(recipe_value), encoding="utf-8")
    first = copy.deepcopy(project())
    second = copy.deepcopy(project())
    first["tracks"][0]["kind"]["clips"][0]["notes"][1]["start"] = 500
    second["tracks"][0]["kind"]["clips"][0]["notes"][1]["start"] = 510
    calls, evaluated = _patch_fake_pipeline(monkeypatch, [first, second], [{"Coherence": 3.0, "Musicality": 3.0, "Memorability": 3.0, "Clarity": 3.0, "Naturalness": 3.0}, {"Coherence": 4.0, "Musicality": 4.0, "Memorability": 4.0, "Clarity": 4.0, "Naturalness": 4.0}, {"Coherence": 5.0, "Musicality": 5.0, "Memorability": 5.0, "Clarity": 5.0, "Naturalness": 5.0}])
    args = _fake_args(tmp_path, recipe_path)
    refine.run(args)
    report = refine.load(args.output / "report.json")
    assert report["selected_stage"] == "00-baseline"
    assert [stage["status"] for stage in report["stages"]] == ["baseline_retained", "candidate_for_listening", "candidate_for_listening"]
    assert calls[1] == calls[2]
    assert evaluated == ["00-baseline", "01-revision", "02-revision"]
    assert set(report["stages"][0]["scores"]["scores"].values()) == {3.0}
    assert set(report["stages"][2]["score_delta"].values()) == {2.0}


def test_run_persists_and_stops_on_protected_revision(tmp_path, monkeypatch):
    recipe_value = {"schema_version": 1, "brief": "Compose", "focus_track": "Lead", "interventions": [step()]}
    recipe_path = tmp_path / "recipe.json"
    recipe_path.write_text(json.dumps(recipe_value), encoding="utf-8")
    invalid = copy.deepcopy(project())
    invalid["tracks"][1]["mixer"]["gain_db"] = -1
    _, evaluated = _patch_fake_pipeline(monkeypatch, [invalid], [{"Coherence": 3.0, "Musicality": 3.0, "Memorability": 3.0, "Clarity": 3.0, "Naturalness": 3.0}, {"Coherence": 4.0, "Musicality": 4.0, "Memorability": 4.0, "Clarity": 4.0, "Naturalness": 4.0}])
    args = _fake_args(tmp_path, recipe_path)
    with pytest.raises(ValueError, match="protected"):
        refine.run(args)
    report = refine.load(args.output / "report.json")
    assert report["status"] == "failed"
    assert report["stages"][1]["status"] == "failed_or_rejected"
    assert evaluated == ["00-baseline"]
    assert not (args.output / "01-revision" / "songeval.json").exists()

