# Live rendered-audio feedback validation

On 2026-10-21, the in-app Codex backend completed a real composition, audio review,
local edit and A/B review using the current unsaved document. Claude Code reached
its native CLI and exclusive live MCP connection, but both Sonnet and Haiku hit
the account session limit before making a tool call. Its actual listening loop
remains unverified. This follows the earlier
[missing-path investigation](agent-listening-loop-audit.md).

## Implementation and scope

The shared live `listen` command snapshots the open document, renders 1–8 bars
(at most 30 seconds) at 48 kHz and submits actual PCM16 WAV bytes to the configured
audio critic. It works independently of the controller's direct audio-input
support. Codex and Claude Code receive the critic's text and exact WAV
measurements; the controller itself does not receive or hear the WAV.

Rendering, encoding and inference run on a background worker. The existing
`edit_project.listen` permission requires network approval/review before taking
the snapshot. Changed documents, denied rules and cancelled conversations reject
pending results. Cancellation prevents later stages, although an in-flight HTTP
request can run until its bounded timeout. No listening call saves the project,
changes its faders or creates an Undo checkpoint.

The session retains four immutable raw excerpts, identified by opaque IDs. An
A/B request must use the same bars, tempo map and meter map. Its before/current
files share one attenuation, chosen to protect both without masking their
relative level change. The estimate targets -1 dB true peak using the existing
4x Catmull-Rom analysis; it is not a certified BS.1770 measurement. The encoded
files are decoded and checked before upload. Source overload, applied
attenuation, encoded peaks/RMS and exact WAV hashes are reported separately.
Audition protection does not repair an overdriven project.

The saved-project MCP preview also uses the protected writer. Its file-path
comparison workflow remains distinct from live immutable IDs. No composer writer
or instrument sound constant changed in this work.

## Actual backend trials

The opt-in GPUI harness ran the real application worker, native CLI, live MCP,
AutoReview, session and renderer without a display, application GPU or audio
device. Ollama served the existing local `gemma4:e2b` audio-capable critic. These
are tool-driven audio-submission trials, not speaker playback or human listening.
The harness saved an empty initial project, let the agent edit the live session,
and saved the final document only after the agent ended. Host saves are not agent
actions. Direct session tests additionally cover listening with no project path.

Each backend received the same request: create a bright 16-bar pop song with
melody, accompaniment and named sections; submit the rendered audio to the local
critic; revise supported problems; compare the same passage; report uncertainty.

| Backend | Outcome | Evidence |
| --- | --- | --- |
| Codex CLI 0.163.0, `gpt-6.1-sol`, high effort | Passed in 178.30 seconds | 4 tracks, 470 notes, C major, 124 BPM; Intro, verse, prechorus, chorus and outro; two successful audio reviews with an intervening edit/readback |
| Claude Code 2.1.304, `sonnet`, high effort | Blocked in 15.04 seconds | CLI/MCP startup succeeded; session limit before any tool call |
| Claude Code 2.1.304, `haiku`, default effort | Blocked in 5.28 seconds | Same session limit before any tool call |

Claude reported: `You've hit your session limit · resets 5:40pm (Asia/Tokyo)`.
The reported reset is **2026-10-21 17:40 JST**. Shared-tool tests and a successful
MCP connection establish integration coverage, not a completed Claude composition
or listening trial. Neither approvals nor the quota were bypassed.

### Codex's completed feedback loop

Codex reviewed chorus bars 9–12, a 7.742-second passage at revision 528. The critic
described a relatively clear melody that could be somewhat buried, while noting
uncertainty about the balance and phrasing. Codex chose a small accompaniment
change rather than rewriting notes: it lowered the piano by 1.5 dB, normalized
the mix with a peak ceiling, read back the mixer and inspected the range.

It then submitted the immutable before and the current render of the same bars
at revision 534. Normalization moved the overall gain, while preserving the
intended relative piano reduction. Both comparison excerpts used zero protective
attenuation. The before WAV hash exactly matched the original baseline; the
current WAV differed.

| Encoded excerpt | Sample peak | RMS | Protective attenuation |
| --- | ---: | ---: | ---: |
| Initial chorus / retained before | -1.696 dBFS | -19.403 dBFS | 0 dB |
| Revised chorus | -1.793 dBFS | -19.632 dBFS | 0 dB |

The source excerpts contained no full-scale samples. The actual encoded files
also passed the peak guard. The saved final project has SHA-256
`6f8c00f413ada38c6ef99df2e36da7768aceada571fbb021bef469513e7611c6`.

The second critic response could not confidently distinguish easier melody
hearing or an overly weak accompaniment. It also repeated the supplied edit
description and used first-person wording about lowering the piano, despite
having no editing tools. This is evidence of an unreliable attribution and
possible prompt influence, not proof of auditory discrimination. Codex's final
answer correctly reported uncertainty. The trial proves that the loop executes;
it does not establish improved musical quality or a reliable critic.

Machine-readable prompts, tool sequence, actual reviews, revision/range data,
WAV measurements/hashes, CLI versions and source hashes are retained in
[`cli-agent-listening-2026-10-21.json`](../tools/eval/references/cli-agent-listening-2026-10-21.json).
Raw transcripts and the copied final project remain under the ignored local
evaluation directory; temporary critic WAVs are deleted after each request.

## Reproduction

Configure an available audio-capable critic through `AURIS_AUDIO_URL` and
`AURIS_AUDIO_MODEL`, and install/authenticate the controller CLI. Create a JSON
configuration with absolute `workspace`, `output` and UTF-8 `prompt` file paths,
`backend: "codex"` or `"claude"`, a supported `model`, `effort`,
`timeout_seconds: 1200`, `natural_workflow: true` and `listening_required: true`.
The output directory must be fresh. Set `AURIS_CODEX_TRANSCRIPTION_EVAL` to that
configuration and run:

```powershell
cargo test -p auris-gpui --bins -- --ignored --exact ui::agent_chat::transcription_eval::installed_cli_composes_and_relistens_in_the_window --nocapture --test-threads=1
```

The test requires actual audio submission, a matching immutable baseline,
increased document revision, state readback between reviews and common A/B
attenuation. Review failures and native backend limits remain failures; the test
does not substitute a scripted response for the real critic.

## Regression coverage

Session tests cover unsaved rendering, actual WAV uploads to a scripted HTTP
fixture, unchanged baseline bytes, relative gain preservation under overload,
source/encoded measurements, document/revision ownership, timing changes,
retention eviction, cancellation and non-finite sample rejection. A saved-preview
regression first reproduced PCM clipping at high gain and now checks protection
without changing the saved project. Host/policy tests cover permission ordering;
desktop tests cover worker cancellation, changed documents and denied rules.

The PC reboot interrupted an earlier build; the source changes survived. Testing
resumed with one Cargo build job. A Windows test-fixture cleanup race was fixed
by waiting for conversation storage to flush. The broad workspace run passed all
session and desktop tests, then found a stale toolbox catalog-count expectation
after adding `listen`; that expectation was corrected and the affected and
remaining package tests passed on rerun. Formatting and workspace documentation
tests passed; strict workspace Clippy is recorded separately with the validation
evidence.
