# Agent tool audit and consolidation plan

Audit date: 2026-10-22. Source revision: `2b5abb48`.
Status: implementation plan; this document does not change the public catalog.

Use one name, one argument contract and one implementation for each capability
across saved-project MCP and the built-in agents. Backward compatibility is not
required. Replace superseded tools directly, delete their handlers and update all
callers, prompts and tests in the same change. Do not add aliases, compatibility
adapters, permission migrations or deprecation periods.

Keep saved-project persistence and live-document ownership behind thin execution
adapters serving the same contract. Do not translate old names, units or selectors.
Keep operations with different purposes distinct.

## Current inventory

| Surface | Auris tools | Qualification |
| --- | ---: | --- |
| Saved-project MCP, all groups enabled | 71 | Startup groups can reduce this |
| Built-in Claude/Codex live MCP | 44 | `ToolHost` excludes `list_instruments` |
| Built-in Ollama/OpenAI-compatible agent | 46 | The same 44 plus `search_documentation` and `search_internet` |

The raw live definition builder has 45 entries. These counts exclude native CLI
shell/file tools and describe the implementation before consolidation. The rebuilt
`evaluate_agent_surface` example and source inspection supply the
[inventory and findings](../tools/eval/references/agent-tool-audit-2026-10-22.json).
No new real-model selection benchmark or token-savings measurement was performed.

## Final consolidation targets

Go directly to these contracts. Do not introduce temporary intermediate tools.
Use a tagged operation enum for closely related domain operations, with a precise
schema for each variant; do not introduce a generic command executor.

| Current entry points | Single destination | Contract |
| --- | --- | --- |
| Saved `edit_notes`, `replace_notes`, `replace_notes_batch`; live `add_note`, `add_notes`, `remove_notes`, `replace_notes` | `edit_notes` | `patch` adds/removes/updates notes; `replace` replaces complete note lists. A bounded batch accepts one or several clips with one Undo/save boundary. |
| Saved `mixer`, `effects`, `set_effect`, `routing`, `set_level`, `set_track_state`, `mixer_graph`; live fader/state tools and the eleven graph tools | `mixer_graph` | Typed read, strip, routing, send and effect operations using shared graph commands. Partial strip updates include gain, pan, mute and solo. |
| Saved `set_instrument_param`, `set_instrument_params` | `set_instrument_params` | One bounded batch; a single parameter is a one-element batch. Expose it live too. |
| Saved `add_part`, live `generate_phrase` | `generate_phrase` | One request/result contract with explicit range and seed. |
| Saved `describe`, live `inspect_project` | `inspect_project` | Shared structured arrangement summary and object references. |
| Saved `notes`, live `read_notes` | `read_notes` | Exact note values and references accepted directly by `edit_notes`. |
| `search_instruments`, `similar_instruments`, `list_instruments` | `search_instruments` | Explicit text, reference-sound or bounded browse mode; common filters and instrument IDs. |
| Saved `transcribe_audio`, `transcribe_mixture`, live `create_piano_cover` | `transcribe_audio` | Explicit YIN, Basic Pitch or MuScriptor backend; `import_transcription` accepts recognized notes. |

### Preserve useful behavior in the unified contracts

- Mixer routing needs explicit atomic output replacement, configured send creation
  and sidechain replacement. Existing graph output connection is additive, while
  existing `routing.output` replaces the destination. Support both intentions as
  explicit operations without intermediate disconnection. Read operations expose
  paged parameter keys, values, units, choices and automation status. Use stable
  effect IDs instead of effect-slot selectors.
- Instrument search needs a bounded browse mode before the list entry is deleted.
  Keep acoustic-index availability and indexing status visible.
- Transcription retains model prerequisites and explicit per-invocation MuScriptor
  noncommercial acknowledgement. Recognition never implicitly replaces the score.

### Distinctions to keep

| Capabilities | Different purpose |
| --- | --- |
| Recipe editing and performance editing | Regenerating stored notes versus changing how an existing score is played |
| `preview_composition` and audio preview | Symbolic inspection versus rendered audio |
| `preview` and `render` | Short protected audition versus delivery output, stems and export settings |
| `inspect_audio`, `analyze_music`, `analyze_audio`, `analyze_chords`, `analyze_instruments` | Rendered measurements, authored-note statistics, source-audio analysis, score harmony and instrument recognition |
| `listen` and numeric inspection | External listening critique consumes real audio and requires network authorization |
| `verify_render` and `render` | Verification also applies to an existing encoded file |
| `inspect_composition` and `inspect_project` | Recipe/provenance comparison versus arrangement overview |
| `set_instrument` and `set_singer_voice` | Instrument selection versus singing-model/style selection |
| `setup_tracks` and elementary edits | Atomic multi-track setup with retry receipts |

## Common contracts

1. **One registry.** Define names, descriptions, schemas, availability and examples
   once in `auris-toolbox`. Derive registration, discovery and help from it. Put
   document operations and authorization classification in `auris-session`;
   keep transports in their frontends. Reuse existing session commands and
   `mixer_graph::GraphAction`. Use ordinary registration/dispatch tables.
2. **One runtime view.** List, discovery, help and execution must agree about
   enabled capabilities and startup groups. Put documentation search behind the
   shared host, fixing the current Claude/Codex preamble mismatch. Advertise
   internet search only when available and authorized. Do not load saved-file
   editing tools into the live document backend.
3. **One reference and unit format.** Use stable track/clip/effect references;
   reference notes by zero-based storage index plus expected document revision.
   Use integer clip-relative ticks for note and curve timing, expose ticks per
   quarter note, and return full numeric precision. Arrangement ranges use
   explicitly named one-based bars. Share session-derived bounds, dB and native
   parameter units. Delete ordinal-selector and alternate beat-unit conversions.
   Do not change the project file format merely to rename tools.
4. **One transaction per edit.** Validate complete batches before mutation. Saved
   execution saves changed results with one checkpoint; live execution creates one
   Undo step and leaves the document unsaved. Bound notes, curve points, file bytes
   and output sizes. Preserve no-op setters/replacements; use receipts for retryable
   creation. Expensive jobs run on revision-bound snapshots off the UI thread and
   reject stale results.
5. **Authorize the requested operation.** Classify the operation and affected
   fields, including file/network access. Bind approvals to the exact request.
   Catalog filtering is not authorization. Remove permission keys for deleted
   tools; reject unknown keys explicitly instead of translating them or granting
   broader access. Preserve score data and authored curves unless explicitly edited.

## Missing workflow steps

Reuse existing session commands through the common contracts. Internal getters
and UI-only actions do not each need a tool.

| Priority | Current gap | Planned capability |
| --- | --- | --- |
| P0 | Live agents cannot export the unsaved mix or MIDI | Audio preview/export, MIDI export and encoded-file verification from the current snapshot, with staging and cancellation |
| P0 | No live clip move/copy/split/resize/remove/freeze | Shared `edit_clip`; distinguish recipe freezing from baking performance |
| P0 | Discovery/help differs by backend | Runtime-scoped capabilities, discovery, help and documentation search |
| P1 | No live instrument-parameter or automation editor | Parameter discovery, batch writes and automation operations |
| P1 | No public clip bend/controller editor, including CC64 | Paged read and atomic merge/replace/clear of authored clip curves |
| P1 | Performance is live-only; recipe editing/regeneration is saved-only | Common performance and recipe commands with hand-edit protection and explicit seeds |
| P1 | Incomplete tempo/meter-map editing | Positioned tempo/meter operations without implicit note movement |
| P1 | Missing live audio/MIDI import and drum-assignment editing | Cancellable import into the existing document and drum-map editing |
| P1 | Transcription and singer selection differ by surface | Shared transcription backends and voice discovery/inspection/selection |
| P2 | No phoneme/timing/per-note singing controls | Capability-aware vocal-note editing that rejects unsupported controls |
| P2 | No live checkpoint/history control | Revision-checked history inspection/restore protecting unrelated user edits |
| P2 | No audio-clip gain/stretch/fade/crossfade editing | Extend `edit_clip` by clip kind |

## Implementation sequence

1. **Unify the catalog and contracts.** Build the common registry and fix
   discovery/help. Consolidate each row directly to its final form, removing old
   definitions and dispatch branches in the same unit. Update callers, prompts,
   examples, policy keys and tests together. Check list/help/call agreement for
   every backend/group, exact note round trips, partial updates, atomic routing
   and affected-operation authorization.
2. **Complete arrangement and delivery.** Add P0 workflows first, then P1 and P2
   in priority order. Check score/curve preservation, one Undo, no-op retries,
   cancellation and stale-result rejection. Include 6/8 and mixed-meter timing,
   unavailable-model errors and explicit transcription acceptance.
3. **Verify actual agent use.** Run applicable tasks through built-in Claude/Codex,
   rig and saved-project MCP. Agents must invoke canonical tools, change only
   requested controls, read back exact results and export/verify the result.
   Record argument errors, repeated edits, schema size, operation counts and wall
   time. Check deleted names are absent from registration and dispatch.

Each unit includes relevant session/toolbox/host/window tests, formatting and
workspace Clippy. Changing a composer writer or audio constant also requires the
existing music evaluation baselines. There is no separate compatibility phase.

## Source anchors

- [Saved catalog](../crates/auris-toolbox/src/catalog.rs),
  [live definitions](../crates/auris-toolbox/src/live_agent.rs) and
  [live host](../crates/auris-agent/src/host.rs)
- [Rig research tools](../crates/auris-agent/src/lib.rs),
  [CLI instructions](../crates/auris-agent/src/backends/mod.rs),
  [saved discovery](../crates/auris-toolbox/src/tool_exposure.rs) and
  [MCP filtering](../crates/auris-mcp/src/main.rs)
- [Session commands](../crates/auris-session/src/live_agent.rs) and
  [authorization](../crates/auris-session/src/agent_policy.rs)
- [Graph commands](../crates/auris-session/src/mixer_graph/commands.rs),
  [routing](../crates/auris-toolbox/src/track_editing.rs) and
  [effect editing](../crates/auris-toolbox/src/mix_editing.rs)
- [Saved note/generation tools](../crates/auris-toolbox/src/lib.rs),
  [batch editing](../crates/auris-toolbox/src/batch_editing.rs) and
  [replacement input](../crates/auris-toolbox/src/replace_notes.rs)
- [Clip/curve commands](../crates/auris-session/src/session/clips.rs),
  [timeline](../crates/auris-session/src/session/transport.rs),
  [singer controls](../crates/auris-session/src/session/singer.rs) and
  [transcription](../crates/auris-toolbox/src/recognition.rs)
