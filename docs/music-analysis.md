# Music analysis

[日本語](music-analysis.ja.md)

Auris estimates chords from written notes and constant tempo and major/minor chords
from audio. These commands use Rust signal processing and fixed templates on the CPU,
without learned weights or a GPU. Optional [local YAMNet inference](music-analysis-models.md)
adds instrument-presence tagging.

## Desktop controls

The **Analysis** menu collects chord estimation, audio analysis, instrument recognition
and drum measurement. **Analyze Chords across Tracks** analyzes the project;
**Analyze Written Chords** analyzes the selected note track. Known percussion and muted
material are excluded. The default resolution is one quarter-note beat.

Select an audio clip and choose **Analyze Audio BPM / Chords**. Analysis reads one pass
of the trimmed source before effects and time stretching. A background worker reports
progress and can be cancelled. **Analysis → Analysis Results…** reopens its panel.
The synchronous decode/resample steps finish before cancellation is observed.

**View Analysis Draft** shows chord alternatives, their scores and BPM alternatives.
Previews show up to 256 intervals; JSON output contains the full result. Audio times
are original source seconds, including the clip's trim offset. Written-note times
are zero-based quarter-note beats.

Nothing changes until an acceptance control is used:

* **Apply Recognized Chords** replaces recognized intervals and clears silent intervals
  in the harmony timeline. Unknown intervals and outside harmony are preserved.
  Symbolic candidates can also be accepted individually.
* **Use as Clip Source BPM** adopts the estimate as the audio clip's source tempo.
  It preserves the project tempo map; a clip following project tempo may stretch.

Each acceptance is one undo step. Any document edit or replacement of the source audio
invalidates its report. Reports are transient and add no project format fields.

## CLI and model tools

Commands print JSON. Beat arguments are zero-based quarter-note beats.

```sh
auris analyze-chords song.auris --track Piano --from-beat 0 --to-beat 32 --window-beats 1
auris analyze-chords song.auris --apply
auris analyze-audio recording.wav
```

`--track` also accepts `id:number`; omitting it selects all pitched note tracks.
`--apply` accepts written harmony and saves a checkpoint. File audio analysis requires
no project or audio device. MCP and agent tools expose `analyze_chords` and `analyze_audio`
through the same session commands. Scores measure template agreement or periodicity,
not calibrated probabilities.

## Algorithms and practical limits

Written-note analysis accumulates duration-weighted pitch classes on a fixed grid,
caps octave and unison doubling, and considers sustained bass. It searches twelve roots
for major, minor, diminished, augmented, suspended, seventh and sixth templates.
A transition penalty discourages unnecessary changes. Four alternatives are retained;
weak or tied interpretations remain unknown. Grid boundaries limit change timing.

Audio is resampled to 11,025 Hz. A 2,048-sample FFT and roughly 20 ms hop extract spectral
peaks. Channel magnitudes are pooled after transformation so opposite-phase stereo does
not cancel. Half-second pitch-class windows use major/minor templates at A4 = 440 Hz.
Harmonics, percussion, detuning and dense mixtures can confuse these estimates.

Tempo uses positive level change and spectral flux, onset autocorrelation over 40–240 BPM,
and dynamic-programming beat alignment. Up to three candidates expose competing pulse
rates. Short or nonperiodic signals may have no estimate. Beat timestamps do not assert
meter, downbeats or tempo automation.

Requests are limited to thirty minutes/eight audio channels and 16,384 chord intervals.
Low-rate features are retained instead of a full spectrogram. File decoding loads the
complete file before checking duration, and clip jobs copy their trim before resampling;
these limits do not establish a streaming memory ceiling.

## Verification

```sh
cargo test -p auris-analysis
cargo test -p auris-session --lib recognition
cargo test -p auris-gpui --bins music_analysis
cargo run -p auris-analysis --release --example measure_analysis
```

Numeric fixtures cover chord spelling, ambiguity/silence, loops and inversions,
72/120/180 BPM pulses, beat alignment, cancellation, invalid samples and opposite-phase
stereo. Session and window tests cover explicit acceptance, stale reports and Undo.
The measurement example prints timing and tempo estimates for three minutes of prepared
stereo A4 pulses at 120 BPM, excluding file decoding and resampling. Synthetic checks
establish regression behavior rather than accuracy on real recordings.
