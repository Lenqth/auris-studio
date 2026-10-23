"""Keep the sung-lyric recogniser's scoring independent of Whisper's spelling choices."""

import numpy as np
import pytest
import soundfile
from voicevox_intelligibility import cer, edit_distance, load_16k, to_hiragana


def test_transcripts_are_compared_as_bare_hiragana():
    assert to_hiragana("サクラ、さくら。") == "さくらさくら"
    assert to_hiragana("ｶｷｸｹｺ") == "かきくけこ"
    assert to_hiragana("こんにちは!") == "こんにちは"


def test_a_substituted_consonant_costs_one_mora():
    assert edit_distance("ぱぴぷぺぽ", "ぱびぶべぼ") == 4
    assert cer("はひふへほ", "あひふへほ") == pytest.approx(0.2)
    assert cer("", "あ") == 1.0


def test_probe_audio_is_resampled_for_whisper(tmp_path):
    path = tmp_path / "tone.wav"
    seconds = np.arange(24_000) / 24_000
    soundfile.write(path, np.sin(2 * np.pi * 440 * seconds), 24_000, subtype="FLOAT")
    audio = load_16k(path)
    assert audio.dtype == np.float32
    assert len(audio) == 16_000
    spectrum = np.abs(np.fft.rfft(audio))
    assert np.argmax(spectrum) == pytest.approx(440, abs=1)
