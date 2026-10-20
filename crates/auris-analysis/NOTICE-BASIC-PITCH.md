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
reference mono averaging, 22,050 Hz input, 43,844-sample windows, 30 overlapping
256-sample frames, initial half-overlap padding and removal of 15 output frames
at either edge. Original source duration bounds all returned notes.

The Rust decoder is an Auris implementation, not Spotify's Python postprocessor.
It uses note activity at 0.3, onset peaks at 0.5, an 11-frame gap tolerance and an
11-frame minimum note duration. Repeated attacks split notes; state persists
across inference windows. Retained frames map uniformly to their exact window
hop to avoid accumulated timing drift. Frame activity can start soft notes;
there is no Python/librosa dependency, inferred-onset normalization, iterative
Melodia energy removal or pitch-bend decoding. Output activation strength maps
to relative MIDI velocity, not a recovered original performance velocity.

Results are fallible polyphonic note drafts. They do not separate instruments,
recover unpitched percussion, engrave sheet music or guarantee a two-hand playable
arrangement. Dense mixes, reverb and anti-phase channels can reduce accuracy.
No original Basic Pitch test recordings or datasets are redistributed.
