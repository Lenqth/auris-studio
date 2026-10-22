# Codex composition workflow audit

Date: 2026-10-22. Production code: `c5d92b2d`. The built-in Codex backend
(`gpt-6.1-sol`) received natural Japanese requests without tool names, numeric
document IDs or API argument examples. Each follow-up used the same conversation
and open, unsaved document. These are actual model runs through the GPUI host,
not scripted tool responses.

The [prompts](../tools/eval/references/codex-composition-scenarios.json) and
[measurements](../tools/eval/references/codex-composition-validation-2026-10-22.json)
are versioned. Full transcripts, each turn's project snapshot, WAV/MIDI outputs
and diagnostic logs are in `target/eval/composition-2026-10-22/` locally.

## Results

| Scenario | Requests | Tool calls | Time | Verified result |
| --- | ---: | ---: | ---: | --- |
| Blank title-screen BGM | 2 | 52 | 119.0 s | Four parts, D minor, 100 BPM, eight-bar loop; 190 MIDI notes; 19.2 s WAV |
| Existing 6/8 melody and folk accompaniment | 3 | 99 | 171.4 s | Original notes, velocities and CC1 unchanged; 90 BPM and meter retained; eight-bar accompaniment; 16.0 s WAV; **strum request only partially achieved** |
| Four-bar sketch to verse/chorus, local drums and reversal | 5 | 161 | 217.0 s | Original four-bar clips unchanged; only chorus drums edited; reversal restored the entire pre-edit document exactly; 34.285708 s WAV |
| Generated jazz phrases and a different lead take | 3 | 60 | 105.8 s | Three generated clips retained recipes; lead changed from 45 to 39 notes with longer average duration; accompaniment tracks and other project fields unchanged; 20.0 s WAV |

All four delivery requests produced actual 48 kHz, 24-bit, non-silent WAV files
with no full-scale samples and lengths within one sample of the requested bars.
Each MIDI file was independently imported into a fresh session. Export requests
left the document unchanged and unsaved. The folk fixture contained a four-bar
melody; Codex recognized its length and added eight bars of accompaniment rather
than silently extending the original melody.

There were 372 tool calls and three rejected arguments, all recovered: two uses
of MIDI-style integer velocity instead of normalized `0..1`, and one use of
`source` as a provenance description while also supplying inline edits. These
rejections did not partially write notes. Two native CLI calls only read a local
debugging skill; musical edits and delivery used the application's tools.

## Findings and proposed changes

### 1. Performance readback cannot distinguish a setting from its effect

The folk follow-up asked for a soft stroke and subtle human timing/velocity,
without rewriting authored notes. Codex installed `strum` with a 24 ms spread,
low-to-high direction, attacks clock and zero low accent, followed by expression
timing `0.1` and velocity `0.08`. It reported that it had added the stroke.

However, all 49 guitar notes had distinct attack times. Replaying the saved clip
through the actual `MidiClip::sounding_notes_with_meter` implementation, with each
stage isolated, showed **zero changed notes for strum**, and **49 changed notes
for expression**. The strum output was exactly equal to the unperformed baseline.
This is an inapplicable setting, not a failure to persist the parameter. Changing
the strum clock would not create simultaneous chord notes either.

Priority: **P1**. Extend the existing `inspect_performance` and `set_performance`
response with bounded, note-aware applicability and effect measurements: eligible
chord groups, changed onsets/durations/velocities and a warning for an ineffective
stage. Describe that strum spreads simultaneous pitches. When the score has only
single notes, the agent should explain the limitation or request a score change,
while honoring a request to preserve authored notes. Do not add another performance
tool or silently turn an arpeggio into chords.

Acceptance: the same folk follow-up preserves the score, reports effective
humanization, and does not claim an audible stroke when strum changes zero notes.

### 2. Reversing one request relies on reconstructing old note data

The drum reversal succeeded exactly, including every other project field. Codex
achieved it with seven tool calls in 14.2 seconds, replacing the complete previous
drum note array from conversation context. There is no exposed live Undo/Redo
operation. This trial demonstrates a working fallback, not a failed reversal.

Priority: **P2**. Expose one bounded `history` operation for inspecting and reversing
an agent edit group, with an explicit revision guard. Group the edits belonging to
one user request so “undo that change” can preserve earlier requests, and reject
reversal across intervening human edits. Reuse Session history rather than creating
a second project snapshot system. Persistent checkpoints remain useful for named
milestones; they do not replace live edit history.

Acceptance: the same drum request reverses to the identical snapshot without
resending the score, and refuses a stale target after a human edit.

### 3. Exact-note input names and units invite avoidable retries

Two different workflows supplied velocities such as `83` and `94`. In another
call, `source` contained “Authored eight-bar D-minor title-screen loop” alongside
the inline edit array. Runtime validation correctly refused these arguments.

Priority: **P2**. Improve the existing `edit_notes` contract: rename the file input
to `source_path`, express inline/file exclusivity in the transmitted schema, and
keep a short normalized-velocity example in the primary tool description. Preserve
strict validation; do not guess whether `1` means normalized full strength or MIDI
velocity one, and do not introduce a second note-writing tool.

Acceptance: both initial composition requests write valid notes without these
retries; invalid mixed inputs still leave the complete document unchanged.

### 4. Repeated readback is expensive; batch existing reads before adding tools

The arrangement workflow used `read_notes` 64 times out of 161 tool calls. Existing
clips were repeatedly read before and after bounded edits. This preserved the score
correctly, but is a measurable efficiency cost rather than missing functionality.

Priority: **P3**. First evaluate a bounded multi-clip request in the existing
`read_notes` operation, with a shared revision, per-clip pagination and explicit
truncation. Avoid a separate “composition inspector” that duplicates project,
note and performance inspection. Measure calls and response size on these same
prompts before adopting the change.

## Tool coverage and limits

No deletion or merger is justified by these four runs. `generate_phrase` creates
new material, `edit_recipe` changes existing generation conditions, and
`set_performance` interprets the authored score; all three served distinct roles.
Likewise, `preview` delivers an audition, `listen` asks a separate critic and
`render` produces a delivery file. Their purposes differ. Tools unused by these
prompts are not evidence of excess.

The installed `gemma4:e2b` audio critic received actual rendered audio. Some A/B
reviews were inconclusive and Codex reported that uncertainty. A successful critic
call is not proof of musical improvement; the ineffective strum was established
with performed-note comparisons instead. There was one run per scenario on one
model, with no statistical reliability or aesthetic-quality claim. Tests used
explicit Bypass mode in fresh fixtures; interactive approvals were not evaluated.
No existing user project or saved preference was edited.

This change adds the evaluation harness, prompts and findings. It does not change
production tools. Validation: 1,123 GPUI tests passed, six opt-in tests ignored;
all four real-model runs completed; workspace/all-target Clippy and formatting
passed.

## Reproduction

Build the binary test target with `cargo test -p auris-gpui --bins --no-run`.
For each entry in the prompt manifest, write a UTF-8 JSON configuration with
`model`, `fixture`, `prompts`, an absolute fresh `output` directory and
`timeout_seconds` (600 was used here). `{output}` in prompts is replaced by that
directory. Set `AURIS_COMPOSITION_EVAL` to the configuration path and run:

```text
<test executable> --ignored --exact ui::agent_chat::composition_eval::codex_composes_and_revises_the_open_document --nocapture
```

On Windows, copy the test executable into the evidence folder first so it does not
lock Cargo's link output. Codex must already be authenticated; `listen` uses the
configured audio critic. The test writes `project-00.json`, subsequent turn
snapshots, `transcript.json` and `report.json` before asserting transport completion.
A passing test means the conversation completed; assess the stored state and
requirements separately, as the ineffective-strum example demonstrates.
