import audio_critic_calibration as calibration
import pytest


def test_controls_repeat_identical_audio_and_reverse_every_pair(tmp_path):
    trials = calibration.controls(tmp_path)
    assert len(trials) == 24
    for first, second in zip(trials[::2], trials[1::2], strict=True):
        assert first["hashes"] == second["hashes"][::-1]
        assert first["reference_label"] != second["reference_label"]
        if first["kind"] == "identity":
            assert first["hashes"][0] == first["hashes"][1]


def test_clipping_changes_waveform_without_loudness_advantage():
    clean = calibration.phrase(0)
    changed = calibration.degraded(clean, "clipping", 0)
    assert changed != clean
    assert calibration.rms(clean) == pytest.approx(calibration.rms(changed))
    assert max(abs(x) for x in changed) < 1


def test_invalid_judgments_and_identity_preferences_do_not_count_as_success():
    trial = {"kind": "identity", "reference_label": "a"}
    assert calibration.outcome(trial, {"error": "offline"}) == "transport_failed"
    assert calibration.outcome(trial, {"assessment": None}) == "invalid_response"
    assert (
        calibration.outcome(trial, {"raw_response": '{"verdict":"prefer_a"}'})
        == "manual_review_required"
    )
    assert calibration.outcome(trial, {"raw_response": " "}) == "invalid_response"
    assert (
        calibration.outcome(trial, {"assessment": {"verdict": "prefer_a"}})
        == "false_preference"
    )
    assert (
        calibration.outcome(trial, {"assessment": {"verdict": "cannot_judge"}})
        == "cannot_judge"
    )
