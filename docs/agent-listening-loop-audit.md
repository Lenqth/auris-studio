# Rendered-audio feedback in agent composition

This is the initial investigation before the live audio path was added. The
[implementation and follow-up trials](agent-listening-loop-validation.md) now
cover a completed Codex audio-review/edit/A/B loop, protected previews, and the
Claude Code trials blocked by an account session limit. The findings below
describe the earlier build.

On 2026-10-21, the in-app Codex agent could compose, render an inspection, adjust
levels, and inspect the changed passage. It could not send that rendered audio to
a listener through its live tools. An explicit request to listen, revise and listen
again produced a measurement-based loop and an honest statement that listening was
unavailable.

The separate saved-project MCP `listen` operation successfully submitted a real
WAV to the local `gemma4:e2b` critic. That transport success did not demonstrate a
useful musical correction, or an autonomous listening loop in the application.

## Real controller trials show measurement-based revision

The controller was the installed Codex CLI, model `gpt-6.1-sol`, high effort, running
through the actual application worker, live MCP bridge, session and permission
pipeline. The opt-in GPUI harness drives the real application without a display,
audio device or GPU. These trials therefore cover tool-driven rendering and edits,
not speaker playback or human listening. The host saved each final project after
the agent finished; host actions are not counted as agent tool calls.

| Request | Result | Feedback sequence |
| --- | --- | --- |
| Ordinary 16-bar pop composition, from the preceding audit | 58.21 seconds; D major, 128 BPM; four tracks, 540 notes, three named sections | Inspect empty chorus; write notes; inspect bars 9–12; normalize; inspect bars 9–12 again |
| Fresh composition explicitly requesting listening, correction and another listen | 82.87 seconds; C major, 124 BPM; four tracks, 482 notes, five named sections | Inspect bars 9–12; normalize; inspect bars 9–12 again; inspect the ending, bars 15–16 |

Both controllers made three successful `inspect_audio` calls and one successful
`normalize_mix` call, with no `listen` call. In the fresh trial, chorus sample peak
changed from -17.08 to -1.43 dBFS and RMS from -33.91 to -18.26 dBFS. Its authored
chorus score stayed at 145 notes. These measurements establish a rendered-audio
level adjustment and verification; they do not establish a perceptual improvement.
The agent reported in Japanese that actual listening had not been performed.

The ordinary trial's first inspection contained no notes and was silent. Its later
two inspections covered the completed passage. Counting inspection calls alone
would overstate the amount of meaningful musical review.

## The live catalog has no auditory feedback operation

The relevant boundaries are:

* `auris_session::live_agent::Command::InspectAudio` renders a current document
  snapshot and returns levels, score data and mel power.
* `auris_agent::live_mcp` presents that result to Codex as text and a PNG. It does
  not attach a WAV or an `input_audio` content part.
* The rig controller uses the same live definitions and inspection presentation.
  Its instructions explicitly identify this as visual analysis, not listening.
* `ToolHost::definitions()` exposes the live catalog. `listen` belongs to the
  saved-project catalog and is deliberately absent from the live catalog. The
  existing catalog regression test asserts this distinction.

Consequently, prompting the in-app controller to listen more often cannot supply
the missing audio path. The existing saved-project operation opens a file afresh;
calling it against a saved document would also omit subsequent unsaved live edits.

Source: [live commands](../crates/auris-session/src/live_agent.rs),
[live host](../crates/auris-agent/src/host.rs),
[Codex MCP presentation](../crates/auris-agent/src/live_mcp.rs),
[rig instructions and catalog tests](../crates/auris-agent/src/lib.rs), and
[inspection context](../crates/auris-agent/src/worker.rs).

## The separate critic accepts audio but gives no actionable correction

A host-controlled MCP probe opened a copy of the ordinary trial's saved project
and called `listen` for bars 9–12. No critic environment overrides or credentials
were set. Local Ollama advertised audio capability for the default `gemma4:e2b`.
The optional audio bridge on port 11435 was unavailable.

The request completed in 18.14 seconds. A 7.5-second, 24 kHz stereo, 16-bit WAV
(720,044 bytes) was created and `audio_sent` returned true. The project hash was
unchanged. The critic described indistinct instrumental music, expressed uncertainty
about instruments and balance, and declined to suggest a concrete correction.
No unsupported edit was made and no A/B improvement was claimed. This was a
separate host-invoked probe, not an operation chosen by the in-app controller.

The saved critic path also exposed a preview-quality issue: rendering warned of
a +0.1 dBFS peak and integer-WAV clipping. Independent decoding found one sample
at the signed 16-bit endpoint. The same bars had previously measured -1.02 dBFS
in the live 48 kHz inspection. The preview uses a fresh session at 24 kHz, so that
earlier inspection does not guarantee a clean critic input. The source of the
render difference remains unisolated. This single endpoint sample does not explain
the critic's uncertain answer or establish audible distortion.

Source: [saved listening](../crates/auris-toolbox/src/listening.rs),
[preview rendering](../crates/auris-toolbox/src/audition.rs), and
[actual WAV submission](../crates/auris-session/src/audio_review.rs).
`audio_sent` establishes delivery and a nonempty response, not perceptual accuracy.

## Work needed to establish a listening loop

1. Expose an auditory review operation for the current unsaved session. Render a
   snapshot on a worker, pass actual audio to a configured capable critic, and
   return the revision, range, delivery status and observations to the controller.
2. Retain immutable excerpts before and after an edit. Verify that the second
   rendering covers the same range and reflects the edited revision. Measure the
   actual preview and reject or correct clipping before critique.
3. Let the controller make a supported local change, read it back, and request a
   second review. Preserve uncertainty and permit leaving the music unchanged.
4. Validate the critic with blinded, known audible differences before judging
   quality from its advice. Test automatic tool choice separately from explicit
   listening prompts. Require musical feedback beyond loudness measurements.

This investigation does not implement those changes. It establishes the missing
live integration and records the preview discrepancy for follow-up.

## Evidence and reproduction

The [machine-readable record](../tools/eval/references/agent-listening-loop-2026-10-21.json)
contains prompts, tool sequences, inspection measurements, hashes and the critic
response. Raw transcripts, final projects, preview audio and logs are retained in
the ignored `.local-evaluations/agent-listening-audit-2026-10-21` directory. The
ordinary baseline remains in `.local-evaluations/codex-autonomous-2026-10-21`.

To repeat the fresh trial, supply an absolute JSON configuration through
`AURIS_CODEX_TRANSCRIPTION_EVAL` and run the ignored test
`ui::agent_chat::transcription_eval::installed_codex_composes_named_sections_in_the_window`
with `cargo test -p auris-gpui --bins -- --ignored --exact TEST_NAME --nocapture`.
Use an isolated workspace, a prompt file, a fresh output directory, the installed
Codex model and effort, and a timeout. The configuration and assertions are in
[the real-model harness](../crates/auris-gpui/src/ui/agent_chat/transcription_eval.rs).
For the separate probe, run saved-project MCP `listen` against a disposable project
copy with an already configured audio server; retain the preview and response.

Verification also ran the live catalog test and the three saved-listening unit tests.
The latter use a scripted critic to verify WAV submission and immutable A/B
snapshots; they do not establish real-model listening quality.
`cargo fmt --all` and `cargo clippy --workspace --all-targets` also passed, with the
existing `proc-macro-error2` future-incompatibility warning. No application code was
changed by this investigation.
