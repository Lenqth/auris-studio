# Bundled Basic Pitch model

Copyright 2022 Spotify AB. Basic Pitch is distributed under Apache License 2.0;
the complete license is in `LICENSE-BASIC-PITCH` and the original notice is in
`NOTICE-BASIC-PITCH-UPSTREAM` in this directory.

The unmodified ICASSP 2022 ONNX model is bundled as `src/basic-pitch.onnx`:

* Source: <https://github.com/spotify/basic-pitch>
* Revision: `fa5997af0a8210982619003269994a1be25eddf3`
* Original path: `basic_pitch/saved_models/icassp_2022/nmp.onnx`
* SHA-256: `2c3c1d144bfa61ad236e92e169c13535c880469a12a047d4e73451f2c059a0ec`
* Size: 230,444 bytes

Auris runs the waveform graph on ONNX Runtime's CPU provider. It follows the
reference mono averaging except for catastrophic channel cancellation, 22,050 Hz
input, 43,844-sample windows, 30 overlapping
256-sample frames, initial half-overlap padding and removal of 15 output frames
at either edge. Original source duration bounds all returned notes.

The Rust decoder is an Auris implementation, not Spotify's Python postprocessor.
Onset peaks at 0.5 require sustained note support (activity threshold 0.3, mean
at least 0.4, six frames minimum). Four inactive frames bridge brief dropouts.
A second pass recovers confident residual activity (eight frames, mean 0.45,
peak 0.5), with guards around accepted attacks/releases to avoid duplicates.
Repeated attacks remain separate across inference windows. Retained frames map
uniformly to their exact window hop to avoid accumulated timing drift.
When averaging loses over 20 dB relative to the strongest source channel, a
window uses that channel. Normal coherent-channel preprocessing is unchanged.
The released model weights are unchanged; the decoder is calibrated with original
composer/audio pairs and checked on separate seeds and an existing score.
See `docs/piano-transcription-evaluation.md` in the repository root for the method,
per-case results and remaining mixed-audio limitations. Output activation strength
maps to relative MIDI velocity, not a recovered original performance velocity.

Results are fallible polyphonic note drafts. They do not separate instruments,
recover unpitched percussion, engrave sheet music or guarantee a two-hand playable
arrangement. Dense mixes, reverb and complex stereo cancellation can reduce accuracy.
No original Basic Pitch test recordings or datasets are redistributed.
