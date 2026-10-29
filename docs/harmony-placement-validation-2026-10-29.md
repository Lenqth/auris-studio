# Agent harmony placement

Whitespace separates chords within one chart bar. Passing `I V vi IV` to an
eight-bar harmony edit therefore put all four chords in each bar. The data was
already dense before the harmony lane rendered it.

`edit_harmony` now accepts explicit bar arrays:

```json
{"start_bar":1,"bars":8,"chords":["I","V","vi","IV"]}
```

Each entry is one bar; this four-bar progression repeats twice. For intentional
within-bar changes, use entries such as `"I V"`. Explicit pipe-delimited strings,
single chords and catalogue references remain supported. Bare multi-chord strings
are rejected with correction examples before key, chord or section edits begin.
Empty arrays and entries that embed additional bars are rejected too.

The response reports the input `chart_bars` and actual `chord_changes` in the
requested range. The shared prompt asks the agent to check these before generating
backing. Harmony edits preserve existing notes; repositioning harmony in an existing
song and regenerating any backing remain explicit commands.

Regression tests cover one chord per bar at bar 25, repeated progressions,
intentional subdivisions, catalogue charts, invalid input, preserved notes and
Undo/Redo. Existing meter/grid/boundary restoration tests still run.

A real Codex (`gpt-6.1-sol`) panel trial used
`tools/eval/references/harmony-placement-v1.json`. It placed `I V vi IV` twice using
an eight-entry bar array. The captured score has eight chord starts at ticks
0, 3840, 7680, 11520, 15360, 19200, 23040 and 26880, then restores the previous
empty harmony at tick 30720. No notes or exports were requested. Local frozen
runtime, prompts, transcript and score snapshots are under
`target/harmony-placement-2026-10-29/codex/`.

Both model turns completed with zero tool errors, and the before/after track data
is identical. Validation also passed 4,767 workspace unit/doc tests with
`cargo test --workspace --exclude auris-gpu -- --test-threads=4`,
`cargo clippy --workspace --all-targets` and `cargo fmt --all --check`.
