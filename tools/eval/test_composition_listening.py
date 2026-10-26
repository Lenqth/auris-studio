"""Tests for the anonymous composition listening bundle."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import math
import shutil
import struct
import wave
from pathlib import Path

import pytest

MODULE_PATH = Path(__file__).with_name("composition_listening.py")
SPEC = importlib.util.spec_from_file_location("composition_listening", MODULE_PATH)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def _wav(path: Path, amplitude: float) -> None:
    frames = b"".join(
        struct.pack("<h", int(amplitude * 32767 * math.sin(2 * math.pi * i / 32)))
        for i in range(8000)
    )
    with wave.open(str(path), "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(8000)
        output.writeframes(frames)


@pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="ffmpeg is required")
def test_build_preserves_originals_and_hides_condition_labels(tmp_path: Path) -> None:
    before = tmp_path / "before.wav"
    after = tmp_path / "after.wav"
    before_stem = tmp_path / "before-stem.wav"
    after_stem = tmp_path / "after-stem.wav"
    for path, amplitude in (
        (before, 0.2),
        (after, 0.4),
        (before_stem, 0.1),
        (after_stem, 0.3),
    ):
        _wav(path, amplitude)
    pair_file = tmp_path / "pairs.json"
    pair_file.write_text(
        json.dumps(
            {
                "pairs": [
                    {
                        "id": "case-1-rep-1",
                        "conditions": {
                            "before": {
                                "mix": "before.wav",
                                "foreground": "before-stem.wav",
                            },
                            "after": {
                                "mix": "after.wav",
                                "foreground": "after-stem.wav",
                            },
                        },
                    }
                ],
            }
        ),
        encoding="utf-8",
    )

    manifest = MODULE.build(pair_file, tmp_path / "bundle", seed=7)
    public = json.dumps(manifest)
    assert "before" not in public and "after" not in public
    assert manifest["pairs"][0]["available_views"] == ["mix", "foreground"]
    assert set(manifest["pairs"][0]["loudness"]) == {"mix", "foreground"}
    assert manifest["target_lufs"] == -16.0
    assert manifest["target_truepeak"] == -1.0
    private = json.loads((tmp_path / "bundle" / "private-key.json").read_text())
    assert set(private["pairs"][0]["labels"].values()) == {"before", "after"}
    for assets in private["pairs"][0]["assets"].values():
        for audit in assets.values():
            assert Path(audit["source"]).is_absolute()
            assert len(audit["input_sha256"]) == 64
            assert len(audit["matched_sha256"]) == 64
            assert "output_lufs" in audit and "output_truepeak" in audit
    original = tmp_path / "bundle" / "pair-001" / "original" / "A" / "mix.wav"
    assert hashlib.sha256(original.read_bytes()).hexdigest() in {
        hashlib.sha256(before.read_bytes()).hexdigest(),
        hashlib.sha256(after.read_bytes()).hexdigest(),
    }
    assert (tmp_path / "bundle" / "pair-001" / "matched" / "A" / "mix.wav").is_file()
    page = (tmp_path / "bundle" / "listening.html").read_text(encoding="utf-8")
    for axis in ("naturalness", "memorability", "breathing", "arrangement"):
        assert axis in page
    assert "remember" in page.lower()
    assert "remembered_contour" in page
    assert "匿名作曲比較リスニング" in page
    assert "主旋律・歌声" in page
    assert "音量一致版を使う" in page
    assert "pair-index" in page
    assert "jumpToPair" in page
    assert "localStorage" in page
    assert "自動保存" in page
    assert "評価JSONを表示" in page
    assert "ratings-json" in page
    assert "readonly" in page
    assert "pane.value=text" in page


def test_failed_entries_are_retained_without_audio(tmp_path: Path) -> None:
    pair_file = tmp_path / "pairs.json"
    pair_file.write_text(
        json.dumps(
            {
                "pairs": [
                    {
                        "id": "case-2-rep-1",
                        "status": "failed_or_missing_delivery",
                        "reason": "render failed",
                    }
                ]
            }
        ),
        encoding="utf-8",
    )
    manifest = MODULE.build(pair_file, tmp_path / "bundle", seed=1)
    assert manifest["pairs"][0]["status"] == "delivery_failed"
    page = (tmp_path / "bundle" / "listening.html").read_text(encoding="utf-8")
    assert "delivery_failed" in page
    assert "render failed" not in page


@pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="ffmpeg is required")
def test_one_missing_condition_keeps_other_audio_and_failure(tmp_path: Path) -> None:
    after = tmp_path / "after.wav"
    _wav(after, 0.4)
    pair_file = tmp_path / "pairs.json"
    pair_file.write_text(
        json.dumps(
            {
                "pairs": [
                    {
                        "id": "case-3",
                        "conditions": {
                            "before": {"mix": "missing.wav"},
                            "after": {"mix": "after.wav"},
                        },
                    }
                ]
            }
        ),
        encoding="utf-8",
    )
    manifest = MODULE.build(pair_file, tmp_path / "bundle", seed=1)
    pair = manifest["pairs"][0]
    assert pair["status"] == "partial_failure"
    assert pair["failures"]
    assert any("mix" in views for views in pair["views"].values())
    assert "missing.wav" not in json.dumps(pair)


@pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="ffmpeg is required")
def test_nonfinite_loudness_disables_matching_without_leaking_path(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    before = tmp_path / "before.wav"
    after = tmp_path / "after.wav"
    _wav(before, 0.2)
    _wav(after, 0.2)
    pair_file = tmp_path / "pairs.json"
    pair_file.write_text(
        json.dumps(
            {
                "pairs": [
                    {
                        "id": "case-4",
                        "conditions": {
                            "before": {"mix": "before.wav"},
                            "after": {"mix": "after.wav"},
                        },
                    }
                ]
            }
        ),
        encoding="utf-8",
    )

    def nonfinite(_: Path) -> tuple[float, float]:
        raise ValueError("non-finite LUFS")

    monkeypatch.setattr(MODULE, "_measure_loudness", nonfinite)
    manifest = MODULE.build(pair_file, tmp_path / "bundle", seed=2)
    pair = manifest["pairs"][0]
    assert pair["failures"]["matching"]["mix"] == "measurement_unavailable"
    assert "before.wav" not in json.dumps(manifest)
