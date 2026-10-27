"""Tests for composition listening asset preparation."""

import importlib.util
import json
from pathlib import Path

MODULE = Path(__file__).with_name("composition_listening_assets.py")
spec = importlib.util.spec_from_file_location("composition_listening_assets", MODULE)
assets = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(assets)


def project(*tracks):
    return {"tempo_map": {}, "tracks": list(tracks), "harmony": {"key": "C"}}


def track(track_id, name, kind, *, notes=None, mute=False, solo=False):
    payload = {"type": kind, "clips": [{"notes": notes or [], "muted": False}]}
    return {
        "id": track_id,
        "name": name,
        "kind": payload,
        "mixer": {"mute": mute, "solo": solo},
    }


def note(start, length, lyric):
    return {"start": start, "length": length, "lyric": lyric}


def test_select_snapshot_uses_highest_number(tmp_path):
    (tmp_path / "project-00.json").write_text("{}")
    (tmp_path / "project-02.json").write_text("{}")
    (tmp_path / "project-01.json").write_text("{}")
    assert assets.select_snapshot(tmp_path).name == "project-02.json"


def test_select_snapshot_explicit_relative(tmp_path):
    chosen = tmp_path / "final.json"
    chosen.write_text("{}")
    assert assets.select_snapshot(tmp_path, Path("final.json")) == chosen


def test_score_projection_ignores_mixer_and_synthesis_only():
    before = project(track(1, "Vocal", "singer", notes=[note(0, 240, "a")]))
    after = json.loads(json.dumps(before))
    after["tracks"][0]["mixer"]["mute"] = True
    after["tracks"][0]["kind"]["voice"] = "voice.json"
    after["tracks"][0]["kind"]["take"] = {"state": "current"}
    assert assets.score_lyrics_projection(before) == assets.score_lyrics_projection(
        after
    )


def test_focus_prefers_singer_then_lead():
    vocal = track(1, "Vocal", "singer")
    assert assets.focus_track(project(track(2, "Lead", "instrument"), vocal))["id"] == 1
    assert assets.focus_track(project(track(2, "Lead Melody", "instrument")))["id"] == 2


def test_focus_explicit_name_is_required_to_exist():
    try:
        assets.focus_track(project(track(1, "Lead", "instrument")), "Nope")
    except ValueError as error:
        assert "focus track not found" in str(error)
    else:
        raise AssertionError("missing explicit focus did not fail")


def test_audition_mix_changes_only_mixer():
    value = project(
        track(1, "Lead", "instrument", mute=True), track(2, "Pad", "instrument")
    )
    original = json.loads(json.dumps(value))
    assets._set_audition_mix(value, 1, "foreground")
    assert value["tracks"][0]["mixer"] == {"mute": False, "solo": True}
    assert value["tracks"][1]["mixer"] == {"mute": False, "solo": False}
    value = json.loads(json.dumps(original))
    assets._set_audition_mix(value, 1, "backing")
    assert value["tracks"][0]["mixer"]["mute"] is True
    assert value["tracks"][1]["kind"] == original["tracks"][1]["kind"]


def test_singer_audit_counts_lyrics_and_breath_gaps():
    singer = track(
        1,
        "Vocal",
        "singer",
        notes=[note(0, 240, "a"), note(480, 240, ""), note(960, 120, "i")],
    )
    audit = assets.singer_audit(project(singer))
    row = audit["singers"][0]
    assert row["notes"] == 3
    assert row["lyrics_assigned"] == 2
    assert row["lyrics_unassigned"] == 1
    assert row["breath_gaps"] == 2


def test_singer_audit_ignores_muted_clips():
    singer = track(1, "Vocal", "singer", notes=[note(0, 240, "a")])
    singer["kind"]["clips"][0]["muted"] = True
    assert assets.singer_audit(project(singer))["singers"][0]["notes"] == 0


def test_prepare_keeps_sing_subcommand_and_uses_no_tail(tmp_path, monkeypatch):
    source = tmp_path / "source"
    source.mkdir()
    snapshot = project(track(1, "Vocal", "singer", notes=[note(0, 240, "a")]))
    (source / "project-01.json").write_text(json.dumps(snapshot), encoding="utf-8")
    output = tmp_path / "output"
    auris = tmp_path / "auris.exe"
    auris.write_bytes(b"fake auris")
    voice = tmp_path / "voice.voicevox.json"
    voice.write_text("{}", encoding="utf-8")
    calls = []

    def fake_run(path, args, cwd, timeout):
        calls.append(args)
        if args[0] == "sing":
            saved = json.loads((output / "song.auris").read_text(encoding="utf-8"))
            saved["tracks"][0]["kind"]["take"] = {"seed": 0, "fingerprint": 1}
            (output / "song.auris").write_text(json.dumps(saved), encoding="utf-8")
        elif args[0] == "render":
            Path(args[args.index("-o") + 1]).write_bytes(b"wav")
        return {
            "command": [str(path), *args],
            "returncode": 0,
            "stdout": "",
            "stderr": "",
            "elapsed_seconds": 0,
        }

    monkeypatch.setattr(assets, "find_auris", lambda _: auris)
    monkeypatch.setattr(assets, "find_ffmpeg", lambda _: auris)
    monkeypatch.setattr(
        assets,
        "_make_listening_wav",
        lambda *_: {"asset": {"path": "x", "bytes": 3, "sha256": "x"}, "gain_db": 0},
    )
    monkeypatch.setattr(assets, "run_cli", fake_run)
    args = assets.parser().parse_args(
        ["--source-dir", str(source), "--output", str(output), "--voice", str(voice)]
    )
    assert (
        assets.prepare(args) == 1
    )  # fixture has no take audio source; readiness must fail
    assert calls[0][0] == "help"
    assert calls[1][0] == "sing"
    assert all("--no-tail" in call for call in calls[2:] if call[0] == "render")
