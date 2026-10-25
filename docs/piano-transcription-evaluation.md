# Piano-cover transcription: confidence and missing notes

[日本語](piano-transcription-evaluation.ja.md)

The 2026-10-21 decoder revision improves the bundled Basic Pitch piano-cover backend.
It changes note decoding and catastrophic stereo-cancellation handling; model weights,
the composer, instrument synthesis and existing notes in saved projects are unchanged.
The live agent uses the same `transcribe_piano_file` / `create_piano_cover` session path.

The [measurement record](../tools/eval/references/piano-transcription-2026-10-21.json)
retains recording/reference hashes, executable hashes, every case, onset/offset metrics,
selection decisions and regressions. The baseline is revision `e0d97d06`.

## What changed

Previously any note activity above 0.3 could start a note without an onset. Weak sustained
predictions and the lead-in/release around real attacks could become extra events. An
11-frame minimum also removed short repeated notes, and the first frame could not be
a learned onset. Unit tests reproduce those failures before the change.

The new decoder first accepts learned onset peaks at 0.5, supported by activity at 0.3,
mean confidence at least 0.4 and six retained frames (about 69 ms). It bridges at most four
inactive frames. A second pass recovers onset-weak notes from residual activity with at
least eight frames, mean 0.45 and peak 0.5. Guards of eight frames before and four after
accepted notes prevent duplicate attacks/releases; recovery backtracks at most four frames
through activity at least 0.1. Repeated attacks, overlaps, source-duration bounds and the
existing long-file timestamp mapping are preserved.

Channel averaging remains the normal preprocessing. If a window's mono power falls below
1% of its strongest channel's power, it uses that channel rather than erasing the recording.
Numeric waveform tests and actual model inference cover inverted stereo, coherent stereo,
silence and nonfinite samples. This does not solve arbitrary cancellation within a mix.

The model is Spotify's unmodified Apache-2.0 ICASSP 2022 ONNX artifact; provenance is in
[the bundled notice](../crates/auris-analysis/NOTICE-BASIC-PITCH.md). The Rust postprocessor
is an Auris implementation. The [upstream postprocessor](https://github.com/spotify/basic-pitch/blob/fa5997af0a8210982619003269994a1be25eddf3/basic_pitch/note_creation.py)
provided a reference for onset/residual decoding, not a claim of Python parity.

## Frozen score/audio pairs

`evaluate_piano_transcription` creates 36 short pairs using the existing composer:

* `city-pop`, `jazz-trio` and `orchestral`, four verse bars plus one second of release.
* Development seeds 101/102, validation 301/302, and held-out seeds 901/902.
* A piano rendition of every pitched part, excluding drums, and a native-instrument mixture
  that retains drums as interference. Native instruments avoid external SoundFont dependencies.

The generator resolves the clips' performed notes, including timing/velocity transformations
and repetitions, once. Those exact events are frozen into ordinary clips; the annotations
and renderer consume the same frozen performance. The dry mix has a common peak of 0.7.
The scorer independently reads each saved project's constant tempo and 960-tick beat grid
and verifies every annotation against the frozen notes. Unresolved performance transforms,
loops, muted tracks or mismatched timestamps fail evaluation.

Coincident same-pitch references within 12 ms are unioned: a piano key cannot represent
two simultaneous unison parts separately. Matching then uses maximum one-to-one bipartite
matching with exact MIDI pitch and 50 ms onset tolerance. A timing error can count as both
an unmatched estimate (FP) and a missing reference (FN). The report also retains 100 ms
onset scores and onset/offset scores using max(50 ms, 20% of reference duration) for releases.
These metrics do not measure arranging quality or hand playability.

The development-only exploration compared 414 postprocessing configurations. A stricter
0.6 onset threshold improved F1 but increased validation omissions, so it was rejected.
The chosen 0.5 threshold and confidence-gated recovery reduce both FP and FN on validation.
Held-out and existing-score candidate results were read only after that selection.
All published estimates below come from the actual Rust session backend, not the
exploratory Python decoder. Python supplies independent matching and annotation checks.

| Composer split, piano and mixture combined | FP before → after | FN before → after | Onset F1 before → after |
| --- | ---: | ---: | ---: |
| Development, 12 cases | 486 → 266 | 248 → 232 | 0.616 → 0.708 |
| Validation, 12 cases | 460 → 267 | 277 → 270 | 0.619 → 0.693 |
| Held out, 12 cases | 568 → 327 | 292 → 287 | 0.595 → 0.675 |

Improvements are not uniform. Held-out mixtures reduce FP 189 → 105 but increase FN
202 → 203. Two validation city-pop mixture cases have slightly lower onset F1. Orchestral
mixtures still miss many events. The per-case record keeps these failures visible.

## An independent existing score

An independent Mutopia SATB *Ode to Joy* score from an earlier audit was rendered as
piano and as reeds/strings. Neither its notes nor its new-decoder results were used
to select parameters. The 238-event reference is read independently from the
exported MIDI; input WAVs match the files hashed in the
[measurement snapshot](../tools/eval/references/agent-production-audit-2026-10-21.json).
This is controlled synthesis of an existing work, not an independent acoustic recording.

| Input | FP before → after | FN before → after | Onset F1 before → after |
| --- | ---: | ---: | ---: |
| Piano | 35 → 20 | 0 → 0 | 0.932 → 0.960 |
| Reeds + strings | 60 → 19 | 144 → 115 | 0.480 → 0.647 |

The mixed source's onset precision rises from 61.0% to 86.6%, and recall from 39.5% to
51.7%. Its onset/offset F1 rises from 0.311 to 0.453; many releases and notes remain wrong.
Existing-score drafts were applied through the actual piano-cover session command, saved,
rendered, and verified with one-step Undo and Redo. Comparison WAVs use the same gain on
both versions; their hashes and gain are in the record. No subjective listening score is claimed.

## Reproduce and extend

Generate once, then keep the same manifest, audio and annotations for every decoder:

```sh
cargo run -p auris-session --example evaluate_piano_transcription -- generate target/piano-transcription/corpus
cargo run -p auris-session --example evaluate_piano_transcription -- run target/piano-transcription/corpus/manifest.json target/piano-transcription/before
uv run --with numpy --with scipy tools/eval/piano_transcription.py target/piano-transcription/corpus/manifest.json target/piano-transcription/before --json target/piano-transcription/before.json
# Change the decoder, retaining the frozen corpus and archived baseline executable.
cargo run -p auris-session --example evaluate_piano_transcription -- run target/piano-transcription/corpus/manifest.json target/piano-transcription/after
uv run --with numpy --with scipy tools/eval/piano_transcription.py target/piano-transcription/corpus/manifest.json target/piano-transcription/after --json target/piano-transcription/after.json --baseline target/piano-transcription/before.json
cargo run -p auris-session --example evaluate_piano_transcription -- cover target/piano-transcription/after/city-pop-901-mixture.json target/piano-transcription/listening-cover
```

Output directories/reports must be new. Historical baseline reproduction can build this
development example against `e0d97d06`; its session APIs already existed there. Pair scoring
rejects changed recordings, labels, frozen projects, model weights, matching rules or cohorts.
Comparisons are bounded to 10,000 notes per file/four million candidate pairs; use excerpts.
Python/scipy are evaluation dependencies only. Production inference remains Rust/ONNX CPU,
with cancellation and the existing ten-minute/eight-channel/200,000-note limits.

The metric's regression tests cover maximum matching, duplicate estimates, pitch/timing
errors, frozen-note annotation checks and incompatible baseline rejection:

```sh
uv run --with numpy --with scipy --with pytest pytest tools/eval/test_piano_transcription.py
uv run --with ruff ruff check tools/eval/piano_transcription.py tools/eval/test_piano_transcription.py
```

Original composer pairs make inexpensive controlled regressions possible. Their native
instruments and dry mixes are a limited domain: these results do not establish accuracy
on vocals, dense mastered/commercial mixes, or independent acoustic piano recordings.
Broader reviewed recording/annotation pairs remain necessary for that claim.

The [direct audio-LLM experiment](direct-audio-transcription-experiment.md) uses the
same held-out recordings to test replacing recognition with local model-generated notes.
