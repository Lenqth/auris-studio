# Agent tool consolidation validation

[日本語](agent-tool-validation.ja.md)

Date: 2026-10-22. Implementation and measurements are in
[the validation record](../tools/eval/references/agent-tool-validation-2026-10-22.json).

## Resulting contracts

Saved-project MCP has 69 tools with all groups enabled. Built-in Claude, Codex and
rig share 48 live tools, plus `search_internet` when a Brave key is configured.
The saved adapter adds `project`; document commands, validation and typed domain
operations are shared. Runtime listing, discovery, help and execution agree.
Saved composition/library operations retain their separate purposes.

The completed workflows include exact note batches, mixer routing and parameters,
instrument controls and automation, clip arrangement and authored bend/CC lanes,
timeline maps, performance and recipe editing, audio/MIDI import, transcription
draft acceptance, capability-aware vocal controls and checkpoints. Snapshot jobs
provide protected previews, delivery WAV/stems, MIDI export and encoded verification
without saving the live document. Workflows and argument examples are documented
in [agent workflows](agent-workflows.md) and [audio delivery](agent-audio-delivery.md).

Provider registration strips schema prose while preserving constraints. Compact
parameter schemas total 38,450 bytes for saved tools and 32,496 bytes for live tools;
these are UTF-8 JSON bytes, not provider token counts. `tool_help` returns compact
schemas and examples; an `operation` selects its detailed field reference.

## Real agents

The opt-in window test uses a fresh unsaved fixture with three notes and a separate
CC7 lane. It asks for track gain/pan, one indexed note update and CC64 replacement,
independent readback, a preview, MIDI and a 24-bit WAV, then encoded verification.
It checks the complete document delta, preservation of unrelated notes and CC7,
three Undo steps, Redo, the unsaved state and actual exported files.

| Backend | Model | Elapsed | Result |
| --- | --- | ---: | --- |
| Claude Code | sonnet | 34.11 s | Passed; reread notes after one stale-revision refusal |
| Codex | gpt-6.1-sol | 33.25 s | Passed; no tool errors |
| rig / Ollama | qwen3.8:27b, 64K, thinking off | 300.33 s | Passed; recovered from one rejected argument shape |

The fixture explicitly selects Bypass mode and fresh output paths. Calls still
cross the real window host and permission path; user preferences and projects are
not changed. This checks integration and exact edits, not general model reliability
or musical listening accuracy. Saved listening comparisons additionally use a
scripted local critic to verify immutable audio bytes across fresh sessions.

Earlier local-model trials exposed misleading policy prose and oversized full help;
the shared policy frame now describes only the active mode, and help can be scoped
to an operation. At 32K, longer gpt-oss/Qwen workflows still exhausted their context;
another gpt-oss trial returned a provider JSON error. The record distinguishes those
failures from successful tool calls rather than treating a partial edit as a pass.
Gemma at 64K also made invalid calls and reached its output limit. Qwen completed
the full task at 64K, including exact document, Undo/Redo and encoded-file checks.
These results do not imply that every installed model can complete this workflow.

Run the ignored test with `AURIS_TOOL_SURFACE_EVAL` pointing to JSON containing
`backend`, `model`, an absolute fresh `output` folder and `timeout_seconds`.
Optional `provider`, `url`, `api_key_env`, `context_tokens` and `thinking` configure
the selected transport. The test name is
`ui::agent_chat::tool_surface_eval::installed_agent_edits_and_exports_the_unsaved_document`.

## Saved documents and regression checks

Actual stdio MCP validation creates a separate project and exercises exact note
editing, unchanged retries, stale rejection, mixer strips, automation, CC64,
performance, checkpoints, numeric/image inspection, preview and verified export.
Read/export operations must leave project bytes unchanged. Retired names must be
absent from listing and fail dispatch.
The successful stdio run completed in 0.90 seconds with a stereo 48 kHz, 24-bit,
96,000-frame WAV and no full-scale samples. Its single stale-revision rejection
and 21 retired-name rejections were deliberate negative checks.

This workflow found a pre-existing serialization defect: standalone clips loaded
numeric controller keys, while the internally tagged track deserializer retained
them as strings. Controller keys now deserialize explicitly. The regression checks
instrument, drum and singer tracks with CC0/64/127 through both JSON text and values;
the saved MCP workflow checks persistence across separate tool calls.

Workspace tests completed with 4,327 passed, 23 ignored and no failures. Workspace
Clippy, formatting, application/MCP builds and warning-denied workspace documentation
also passed. Numeric preview tests overload a source
and verify that attenuation affects only the audition, with no full-scale samples.
Cancellation, stale jobs, failed batches, exact units and no-op persistence are
covered at the session/toolbox/host boundaries. No music writer or DSP algorithm was
changed, so these checks do not claim an aesthetic improvement in generated music.
