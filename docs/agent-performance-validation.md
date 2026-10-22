# Live performance edit validation

On 2026-10-22, the built-in Codex backend completed a real performance edit through
the application's agent panel. Claude Code stopped before any tool call because
its OAuth session had expired and could not be refreshed. A catalog connection
alone does not verify Claude's inference or editing behavior.

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
| Claude Code 2.1.304, `sonnet` | Authentication blocked | `OAuth session expired and could not be refreshed`; zero tool calls |

Codex applied swing 62% on sixteenths, gate 0.5, transpose +12 semitones,
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

The host rendered the same two-second stereo range before and after the edit.
Both 96,000-frame renders were audible, with sample peaks of 0.167717 and
0.202802. Their sample difference had RMS 0.0377444. This verifies a change in
actual rendered audio, not a human judgment of musical quality. Codex additionally
called `inspect_audio` because its application instructions request it.

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
