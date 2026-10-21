# VOICEVOX pronunciation checks

The 2026-10-22 investigation uses VOICEVOX Engine 0.25.2, prediction style 6000,
and the adapter's 93.75 fps clock. Its baseline is commit `8025e4ab`.

Three adapter problems affected pronunciation:

- Extended kana were split into ordinary syllables. For example, `スィ` became
  `s u i` instead of `s i`, and `クォ` became `k u o` instead of `kw o`.
  The kana reader now recognizes the missing extended moras and Engine aliases.
  Across all 187 spellings, the baseline reader split 40 into extra syllables
  and did not recognize `ヶ`; all 41 readings are corrected.
- A predicted consonant can anticipate its vowel inside the preceding note.
  Applying that preceding note's gain could silence the incoming consonant;
  applying its pitch edits could also put the consonant at the wrong pitch.
  Predicted articulation ranges now take controls from their own note.
- A short note's vowel could receive zero frames, or a temporary query frame
  could be removed from the following consonant. The adapter reserves a vowel
  frame and recalculates acoustic curves without changing vowel onsets or the
  total duration. A one-frame note immediately before a consonant cannot hold
  both sounds on this grid; it is reported as a lyric-duration error.

## Inventory measurement

An individual-query sweep covered 163 primary mora spellings at MIDI 60. The
target mora lasts 24 frames and follows `ア`; the preceding note length and gap
vary as shown below. Both boundaries have 94 padding frames.

| Preceding note | Gap | Cases | Baseline phone mismatches | Fixed mismatches | Duration errors |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 24 | 0 | 163 | 35 | 0 | 0 |
| 24 | 2 | 163 | 35 | 0 | 0 |
| 24 | 8 | 163 | 35 | 0 | 0 |
| 4 | 0 | 163 | 68 | 0 | 0 |
| 2 | 0 | 163 | 36 | 0 | 0 |
| 1 | 0 | 163 | 156 | 0 | 156 |

All 822 representable cases preserve the Engine's intended non-pause phoneme
sequence. The other 156 are rejected before inference. The retained
[measurement](../tools/eval/references/voicevox-pronunciation.json) includes the
35 incorrect splits, conditions, Engine version and inventory-source hash.

The regression additionally covers all 187 spellings, including aliases, in
batched scores: five timing conditions are compared to queries sent directly
to the Engine, independently of Auris's kana parser. Every sung phoneme must
retain a positive duration. All 187 spellings also receive the one-frame
duration validation check.

These checks measure phoneme preservation, not perceived intelligibility for
every singer and pitch. A paired audio probe with 四国めたん / ノーマル keeps
the Engine's phonemes and predicted curves identical and changes only control
ownership: incoming `/k/` and `/s/` remain audible after a zero-velocity note.
Their waveform RMS over the predicted consonant windows rises by about 20 and
22 dB respectively; the intended incoming note velocity is 0.8 in both versions.
Numeric tests also check that the incoming pitch edit is used and the preceding
vowel keeps its own gain.

## Reproduction

With the Engine running, set `AURIS_VOICEVOX_TEST_URL` to its URL and run:

```sh
cargo test -p auris-vocal
cargo test -p auris-singer
```

The live inventory regression is
`a_running_engine_preserves_the_mora_inventory_across_timing_adaptation`.
Without the environment variable, Engine-dependent tests return without
contacting a service. Existing saved takes require an explicit re-render.
