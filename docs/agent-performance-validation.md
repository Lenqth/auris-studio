# Live performance edit validation

[日本語](agent-performance-validation.ja.md)

On 2026-10-22, the built-in Codex and Claude Code backends both completed real
performance edits through the application's agent panel, including independent
readback, audio rendering and Undo/Redo checks. Claude's successful trial followed
the user's renewal of its expired OAuth session.

## Scope and results

The opt-in GPUI test opens the actual application with an isolated configuration,
an unsaved generated clip and authored bend/volume curves. It uses the real agent
worker, native CLI, embedded MCP, application permissions, session and renderer.
The harness requires no display, GPU or audio device. The default Edit policy
applies; no approval is bypassed. Existing user projects are untouched.

Both backends received the same Japanese request: inspect the clip, change six
Performance controls in one `set_performance` call, preserve other settings and
source data, then inspect the resulting settings.

| Backend | Outcome | Evidence |
| --- | --- | --- |
| Codex CLI 0.163.0, `gpt-6.1-sol` | Passed in 25.00 seconds | Read/edit/readback completed; one successful edit; score and curves preserved; Undo and Redo restored the complete projects |
| Claude Code 2.1.304, `sonnet` | Passed in 9.44 seconds after reauthentication | Read/edit/readback completed; one successful edit; score and curves preserved; Undo and Redo restored the complete projects |

Both applied swing 62% on sixteenths, gate 0.5, transpose +12 semitones,
expression velocity variation 0.3, strum spread 30 ms and vibrato depth 0.2
semitones. Readback matched all six controls. Expression timing, swell, group and
seed, strum direction and the authored automatic volume contour were retained.
The inspector's combined `humanize` value also rose to 0.3 because it reports the
maximum of expression timing and velocity; it is not an independent setting.

Playback's performed notes changed as follows, while the stored notes stayed
unchanged:

| Property | Before | After |
| --- | --- | --- |
| MIDI pitches | 60, 64, 67 | 72, 76, 79 |
| Note lengths (ticks) | 960 each | 480 each |
| Second attack (ticks) | 240 | 298 |

The host rendered the same two-second stereo range before and after each edit.
Each backend produced the same final document and render measurements.
Both 96,000-frame renders were audible, with sample peaks of 0.167717 and
0.202802. Their sample difference had RMS 0.0377444. This verifies a change in
actual rendered audio, not a human judgment of musical quality. Codex additionally
called `inspect_audio` because its application instructions request it.

### Earlier Claude attempts

The first Claude attempt stopped before any tool call with
`OAuth session expired and could not be refreshed`. After reauthentication, its
first edit applied all six requested values and passed the source, audio and
Undo/Redo checks, but omitted a separate post-edit `inspect_performance` call.
That trial failed the independent-readback criterion and remains recorded as
`readback_missing` in the evidence.

The harness now permits one explicit read-only follow-up in the same conversation
if an agent ends after editing without that inspection. It still requires a
successful separate inspection, exact values, one edit and the original numeric
and preservation checks. The final successful Claude trial needed no follow-up;
both successful backends completed the full sequence in their first turn.

The compact evidence is
[`agent-performance-2026-10-22.json`](../tools/eval/references/agent-performance-2026-10-22.json).
Full transcripts and before/after document snapshots are retained in the ignored
local `target/eval/performance-live-2026-10-22` directory.

## Reproduction

Install and authenticate the chosen CLI. Create a UTF-8 JSON configuration with
`backend` (`codex` or `claude`), `model`, `timeout_seconds: 180`, and an absolute
`output` path whose parent exists and whose final directory is new. Set
`AURIS_PERFORMANCE_EVAL` to the configuration path, then run:

```powershell
cargo test --workspace installed_cli_edits_performance_in_the_window -- --ignored --nocapture --test-threads=1
```

This invokes real model inference using the signed-in account. The test requires
successful tool calls and exact state and rendering checks; authentication and
provider limits remain failures. Ordinary test runs skip this opt-in test.

## Articulation selection and measured effects

The separate `installed_cli_selects_slide_or_glide_and_reports_effective_performance`
evaluation asks for musical gestures without naming their API properties. It uses
five short clips on native physical guitar and bass instruments:

| Clip | Requested or existing gesture | Required evidence |
| --- | --- | --- |
| Guitar chord | Low-to-high stroke | Changed performed onsets and rendered audio |
| Bass A | One brief intermediate pitch | Added slide note, no continuous glide, changed audio |
| Bass B | Smooth continuous connection | Glide's isolated bend contribution, unchanged note events, changed audio |
| Bass C | Existing glide across a long rest | Measured ineffective glide with the gap reason; unchanged audio |
| Bass D | Existing slide between repeated pitches | No inserted note with the interval reason; unchanged audio |

The model must inspect each edited clip before its edit and independently afterward.
As in the earlier evaluation, one explicit read-only follow-up in the same conversation
is allowed when the model finishes editing without the separate performance readback.
The result records whether that reminder was needed; it does not waive the readback,
source-preservation or audio checks. Permissions and approval requests are not bypassed.
Ineffective-gesture evidence must appear in the model's own successful inspection
results, rather than only in the harness's final reads. The harness compares the
complete project after substituting only the permitted performance stacks, and
checks Undo/Redo against the complete before/after projects. Authored bend and
controller curves and generated recipes are present in the fixture.

Use the same JSON configuration fields as above, but set `AURIS_ARTICULATION_EVAL`
and run:

```powershell
cargo test -p auris-gpui --bins installed_cli_selects_slide_or_glide_and_reports_effective_performance -- --ignored --nocapture --test-threads=1
```

Each run saves `transcript.json`, `result.json`, and five before/after WAV pairs in
its fresh output directory. Audio RMS differences verify that these native
instruments respond; they do not measure musical quality or establish the MIDI
mapping of an unrelated hosted patch. The source performer algorithms are unchanged.

### Results on 2026-10-23

| Backend | Final outcome | Independent readback | Read-only reminder |
| --- | --- | --- | --- |
| Codex CLI 0.163.0, `gpt-6.1-sol` | Passed in 120.81 seconds | All five clips | Not needed |
| Claude Code 2.1.309, `sonnet` | Passed in 20.35 seconds | All five clips | Not needed |

Both selected three partial edits: guitar strum (36 ms for Codex, 40 ms for Claude),
discrete slide strength 0.8, and continuous glide 120 ms. Both preserved the complete
project except those performance stacks, and one Undo/Redo restored the complete
before/after projects. The guitar had three changed onsets, the discrete slide added
one note, and glide changed 48 of 403 curve samples without changing note events.
Both correctly reported the measured 1,000 ms rest against glide's 60 ms limit and
the repeated-pitch slide's zero inserted notes.

| Native render | Codex RMS difference | Claude RMS difference |
| --- | ---: | ---: |
| Guitar strum | 0.01081237 | 0.03119759 |
| Discrete bass slide | 0.00793864 | 0.00793864 |
| Continuous bass glide | 0.04398251 | 0.04398251 |
| Gapped bass | 0 | 0 |
| Repeated-pitch bass | 0 | 0 |

Codex additionally called local `inspect_audio` before and after its changes. Claude
used the symbolic diagnostics and explicitly left instrument response unverified in
its report; the harness independently established the native audio differences above.

Earlier trials are retained rather than counted as final passes. Codex initially
requested external `listen` criticism and stopped at the application's approval
request before editing. The rerun explicitly scoped verification to local measurements
with the same permission policy. Claude's first trial passed the structural checks
but misstated the rest as 500 ms; adding tempo-derived gap measurements corrected that
report in later trials. Another Claude trial selected all gestures correctly but omitted
independent performance readback for edited clips, so it failed that criterion. The
bounded read-only reminder now addresses that omission without relaxing the assertion;
the final trial completed it without a reminder.

Compact evidence, including final model reports and the earlier outcomes, is saved in
[`agent-articulation-2026-10-23.json`](../tools/eval/references/agent-articulation-2026-10-23.json).
Full transcripts and WAV pairs are in the ignored local
`.local-evaluations/articulation-2026-10-23/codex-3` and `claude-4` directories.
