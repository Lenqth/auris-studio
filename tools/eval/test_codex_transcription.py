"""Check anonymous trial preparation and host-context isolation without inference."""

import json
import os

import pytest
from codex_transcription import child_environment, prepare_inputs
from piano_transcription import digest


def test_preparation_copies_only_audio_and_withholds_identity_and_labels(tmp_path):
    source = tmp_path / "source"
    source.mkdir()
    (source / "secret-preset.wav").write_bytes(b"wave samples")
    manifest = source / "manifest.json"
    manifest.write_text(
        json.dumps(
            {
                "cases": [
                    {
                        "id": "secret-preset-seed",
                        "split": "held_out",
                        "audio": "secret-preset.wav",
                        "reference": "missing-score.json",
                        "seconds": 2,
                    },
                    {"id": "excluded", "split": "development", "audio": "missing.wav"},
                ]
            }
        )
    )
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    mapping = prepare_inputs([(manifest, "held_out")], workspace)
    supplied = (workspace / "inputs.json").read_text()
    assert "secret" not in supplied and "reference" not in supplied
    assert "seed" not in supplied and "manifest" not in supplied
    assert mapping[0]["id"] == "secret-preset-seed"
    assert digest(workspace / "inputs/s01.wav") == digest(source / "secret-preset.wav")
    assert {p.name for p in workspace.iterdir()} == {"inputs", "inputs.json"}


def test_empty_cohort_fails_instead_of_running_an_unrelated_agent(tmp_path):
    manifest = tmp_path / "manifest.json"
    manifest.write_text('{"cases": []}')
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    with pytest.raises(ValueError, match="empty"):
        prepare_inputs([(manifest, "held_out")], workspace)


def test_child_does_not_inherit_host_tool_pipe_or_api_routing(monkeypatch):
    monkeypatch.setenv("CODEX_APP_TOOLS_PIPE_PATH", "host-private-pipe")
    monkeypatch.setenv("OPENAI_BASE_URL", "http://host-private-routing")
    monkeypatch.setenv("AURIS_EVALUATION_MARKER", "ordinary-environment")
    env = child_environment()
    assert "CODEX_APP_TOOLS_PIPE_PATH" not in env
    assert "OPENAI_BASE_URL" not in env
    assert env.get("CODEX_HOME") == os.environ.get("CODEX_HOME")
    assert env["AURIS_EVALUATION_MARKER"] == "ordinary-environment"
