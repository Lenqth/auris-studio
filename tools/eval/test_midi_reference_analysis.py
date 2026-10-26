import json
import subprocess
import sys
from pathlib import Path

import midi_reference_analysis as analysis
import mido
import pytest
from midi_reference_analysis import _summary, analyze_file


def _write(path: Path, name: str = "MELODY", *, duplicate: bool = False) -> None:
    midi = mido.MidiFile(ticks_per_beat=480)
    track = mido.MidiTrack()
    track.append(mido.MetaMessage("track_name", name=name))
    track.extend(
        [
            mido.Message("note_on", note=60, velocity=90, time=0),
            mido.Message("note_on", note=60, velocity=0, time=240),
            mido.Message("note_on", note=60, velocity=90, time=240),
            mido.Message("note_off", note=60, velocity=0, time=120),
            mido.Message("note_on", note=62, velocity=90, time=480),
            mido.Message("note_off", note=62, velocity=0, time=240),
        ]
    )
    midi.tracks.append(track)
    if duplicate:
        midi.tracks.append(mido.MidiTrack([mido.MetaMessage("track_name", name=name)]))
    midi.save(path)


def test_explicit_track_metrics_and_zero_velocity_noteoff(tmp_path: Path) -> None:
    path = tmp_path / "fixture.mid"
    _write(path)
    row = analyze_file(path)
    assert row["notes"] == 3
    assert row["unison_fraction"] == 0.5
    assert row["small_motion_fraction"] == 0.5
    assert row["ticks_per_quarter"] == 480
    assert row["duration_median_quarters"] == 0.5


def test_duplicate_named_track_is_rejected(tmp_path: Path) -> None:
    path = tmp_path / "fixture.mid"
    _write(path, duplicate=True)
    with pytest.raises(ValueError, match="expected exactly one"):
        analyze_file(path)


def test_missing_named_track_is_rejected(tmp_path: Path) -> None:
    path = tmp_path / "fixture.mid"
    _write(path, name="PIANO")
    with pytest.raises(ValueError, match="expected exactly one"):
        analyze_file(path)


def test_smpte_timebase_is_rejected(tmp_path: Path) -> None:
    path = tmp_path / "fixture.mid"
    midi = mido.MidiFile(ticks_per_beat=-24)
    midi.tracks.append(mido.MidiTrack([mido.MetaMessage("track_name", name="MELODY")]))
    midi.save(path)
    with pytest.raises(ValueError, match="SMPTE"):
        analyze_file(path)


def test_simultaneous_onsets_do_not_claim_melodic_order(tmp_path: Path) -> None:
    path = tmp_path / "overlap.mid"
    midi = mido.MidiFile(ticks_per_beat=480)
    track = mido.MidiTrack([mido.MetaMessage("track_name", name="MELODY")])
    track.extend(
        [
            mido.Message("note_on", note=60, velocity=90, time=0),
            mido.Message("note_on", note=64, velocity=90, time=0),
            mido.Message("note_off", note=60, velocity=0, time=240),
            mido.Message("note_off", note=64, velocity=0, time=0),
        ]
    )
    midi.tracks.append(track)
    midi.save(path)
    row = analyze_file(path)
    assert row["simultaneous_onset"] is True
    assert row["interval_metrics_valid"] is False
    assert row["small_motion_fraction"] is None
    assert row["diagnostics"]["max_simultaneous_voices"] == 2
    assert row["positive_union_gap_count"] == 0


def test_unmatched_and_unclosed_events_are_reported(tmp_path: Path) -> None:
    path = tmp_path / "malformed.mid"
    midi = mido.MidiFile(ticks_per_beat=480)
    track = mido.MidiTrack([mido.MetaMessage("track_name", name="MELODY")])
    track.extend(
        [
            mido.Message("note_off", note=60, velocity=0, time=0),
            mido.Message("note_on", note=60, velocity=90, time=0),
            mido.Message("note_on", note=62, velocity=90, time=10),
            mido.Message("note_off", note=62, velocity=0, time=240),
        ]
    )
    midi.tracks.append(track)
    midi.save(path)
    row = analyze_file(path)
    assert row["status"] == "suspect"
    assert row["diagnostics"]["unmatched_note_off"] == 1
    assert row["diagnostics"]["unclosed_note_on"] == 1
    assert row["small_motion_fraction"] is None
    assert _summary([row])["files_valid_intervals"] == 0


def test_upward_downward_and_repeated_intervals_are_distinct(tmp_path: Path) -> None:
    path = tmp_path / "directions.mid"
    midi = mido.MidiFile(ticks_per_beat=480)
    track = mido.MidiTrack([mido.MetaMessage("track_name", name="MELODY")])
    for pitch in (60, 62, 60, 60, 67):
        track.extend(
            [
                mido.Message("note_on", note=pitch, velocity=80),
                mido.Message("note_off", note=pitch, time=480),
            ]
        )
    midi.tracks.append(track)
    midi.save(path)
    row = analyze_file(path)
    assert row["small_motion_fraction"] == 0.5
    assert row["unison_fraction"] == 0.25
    assert row["mean_abs_nonzero_interval_semitones"] == pytest.approx(
        11 / 3, abs=0.0001
    )
    assert row["positive_union_gap_count"] == 0
    assert row["positive_union_gap_fraction_ge_1_quarter"] is None


def test_long_overlap_does_not_invent_silence_after_short_note(tmp_path: Path) -> None:
    path = tmp_path / "union.mid"
    midi = mido.MidiFile(ticks_per_beat=480)
    track = mido.MidiTrack([mido.MetaMessage("track_name", name="MELODY")])
    track.extend(
        [
            mido.Message("note_on", note=60, velocity=80),
            mido.Message("note_on", note=62, velocity=80, time=120),
            mido.Message("note_off", note=62, time=120),
            mido.Message("note_on", note=64, velocity=80, time=240),
            mido.Message("note_off", note=64, time=120),
            mido.Message("note_off", note=60, time=360),
            mido.Message("note_on", note=67, velocity=80, time=480),
            mido.Message("note_off", note=67, time=120),
        ]
    )
    midi.tracks.append(track)
    midi.save(path)
    row = analyze_file(path)
    assert row["positive_union_gap_count"] == 1
    assert row["median_positive_union_gap_quarters"] == 1
    assert row["diagnostics"]["max_simultaneous_voices"] == 2


@pytest.mark.parametrize("kind", ["empty", "type2"])
def test_empty_and_asynchronous_tracks_are_rejected(tmp_path: Path, kind: str) -> None:
    path = tmp_path / "unsupported.mid"
    midi = mido.MidiFile(type=2 if kind == "type2" else 1)
    midi.tracks.append(mido.MidiTrack([mido.MetaMessage("track_name", name="MELODY")]))
    midi.save(path)
    with pytest.raises(
        ValueError, match="type-2" if kind == "type2" else "no closed notes"
    ):
        analyze_file(path)


def test_cli_freezes_selection_errors_and_measured_summary(tmp_path: Path) -> None:
    inputs = tmp_path / "inputs"
    inputs.mkdir()
    _write(inputs / "a.mid")
    _write(inputs / "b.mid", name="PIANO")
    output = tmp_path / "analysis.json"
    command = [
        sys.executable,
        str(Path(analysis.__file__)),
        str(inputs),
        "--output",
        str(output),
        "--limit",
    ]
    subprocess.run([*command, "1"], check=True, capture_output=True)
    first = json.loads(output.read_text(encoding="utf-8"))
    assert first["source"]["files"] == ["a.mid"]
    assert first["source"]["commit"] is None
    assert first["summary"]["unison_fraction"]["mean"] == 0.5
    subprocess.run([*command, "2"], check=True, capture_output=True)
    second = json.loads(output.read_text(encoding="utf-8"))
    assert second["source"]["files"] == ["a.mid", "b.mid"]
    assert second["summary"]["files_total"] == 2
    assert second["summary"]["files_suspect_or_error"] == 1
    assert "error" in second["files"][1]
    assert first["source"]["analyzer_sha256"] == second["source"]["analyzer_sha256"]
