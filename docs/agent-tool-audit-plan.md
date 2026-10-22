# Agent tool audit and consolidation plan

Audit date: 2026-10-22. Source revision: `2b5abb48`.
Status: implementation plan; this audit does not change the public tool catalog.

The tool surface has both redundant entry points and missing workflow steps.
Consolidate equivalent operations first, then expose the existing session commands
needed to finish arrangements. Share semantic commands and contracts across
transports while retaining the distinction between the open unsaved document and
a saved project. Do not minimize tool count by making one unrestricted editing tool.

## Current inventory

| Surface | Auris tools | Source and qualification |
| --- | ---: | --- |
| Saved-project MCP, all groups enabled | 71 | `auris_toolbox::tool_catalog`; startup groups can reduce this |
| Built-in Claude/Codex live MCP | 44 | Live definitions filtered through `ToolHost`; excludes legacy `list_instruments` |
| Built-in Ollama/OpenAI-compatible agent | 46 | The same 44 live operations plus `search_documentation` and `search_internet` |

These counts exclude the native CLI's shell/file tools and are not a union of
independent catalogs. The instrument-list fallback in a no-bridge agent test is
not part of the desktop catalog. The raw live definition builder has 45 entries,
one of which is filtered out before desktop exposure.

The existing `evaluate_agent_surface` example was rebuilt and run against this
revision. Its output supplies all names in the
[machine-readable inventory](../tools/eval/references/agent-tool-audit-2026-10-22.json).
Implementation paths and schemas were then checked for capability and semantic
differences. This audit does not include a new real-model selection benchmark or
a claim about how many tokens consolidation will save.

## Fix catalog accuracy and editing contracts first

1. **Make discovery and help reflect the actual runtime.** The rig agent adds two
   research tools outside `ToolHost`; native CLI backends receive only the latter's
   live tools. Their shared preamble nevertheless recommends
   `search_documentation`. Put application documentation help behind the common
   host. Add context-aware `capabilities`, `discover_tools` and `tool_help` for
   live agents, returning only that context's names, schemas and availability.
   Preserve network authorization for internet search; its availability may remain
   backend-specific when the prompt describes that accurately.
2. **Apply MCP startup groups to discovery/help too.** `tools/list` and tool calls
   use `ToolGroups`, but `discover_tools::run` searches the complete saved catalog.
   In a manual-only server it can recommend a composition tool whose call is then
   refused. Help may describe disabled tools, but must mark them unavailable and
   explain how to enable them. Discovery should default to callable tools.
3. **Stop losing precision between reads and edits.** Saved `notes` formats
   timing and velocity through `trimmed(f32)`, which prints two decimal places.
   Live `read_notes` returns tick values. Saved note numbers are 1-based in time
   order; live removal addresses zero-based storage indices. Publish exact ticks,
   unrounded velocity, a stable clip ID, storage index and document revision in
   both. Keep human-readable labels as additional fields. Reject stale index edits.
4. **Align shared argument rules.** Examples include live `set_tempo` at 20..300
   BPM versus saved `edit_harmony`/`create_project` at 20..400; live fader edits
   requiring both gain/pan and mute/solo while the graph setter accepts partial
   changes; clip IDs versus ordinal clip numbers; and meter beats versus quarter
   notes. Derive bounds from the session contract. Never reinterpret an old
   argument silently during migration, especially in 6/8 or mixed meters.

## Consolidation decisions

The destination names below are canonical capabilities, not permission to delete
old clients' call handlers immediately.

| Existing tools | Canonical destination | Required work before hiding duplicates |
| --- | --- | --- |
| Live `add_note`, `add_notes` | `add_notes` initially | A single note is a one-element batch; translate `start`/`beats` to `start_beat`/`duration_beats` explicitly |
| Live `set_level`, `set_track_state`, `set_mixer_track` | `set_mixer_track` | Use optional fields and stable graph node IDs; preserve master support and additive solo |
| Saved `set_instrument_param`, `set_instrument_params` | `set_instrument_params` | A one-element batch covers the scalar call; preserve units, discrete choices and automation warnings |
| Saved `edit_notes`, `replace_notes`, `replace_notes_batch`; live add/remove/replace tools | `edit_notes` with typed `patch` and `replace` operations | Support atomic add/remove/update together, complete replacement, one or several clips, and bounded inline/file input; retain one Undo/save boundary |
| Saved `effects`, `set_effect`, `routing`, `mixer`, `set_level`, `set_track_state`, `mixer_graph` | Shared graph commands; saved `mixer_graph` facade | Close the gaps described below; preserve all parameter detail and optional expected-effect checks |
| Saved `add_part`, live `generate_phrase` | `generate_phrase` | One request/result contract; preserve saved default range behavior through the legacy adapter and make new ranges explicit |
| Saved `describe` / live `inspect_project`; saved `notes` / live `read_notes` | `inspect_project` and `read_notes` | Share structured results, IDs and units; retain legacy selectors only in adapters |
| `search_instruments`, `similar_instruments`, legacy `list_instruments` | `search_instruments` with text, reference-sound and bounded browse modes | Preserve acoustic indexing/status, filters and IDs; replace the compact built-in summary before removing its entry |
| Saved `transcribe_audio`, `transcribe_mixture`, live `create_piano_cover` | `transcribe_audio` with an explicit backend, then `import_transcription` for acceptance | Preserve YIN versus Basic Pitch versus MuScriptor behavior; retain explicit model requirements and per-invocation noncommercial acknowledgement |

### Mixer migration has real semantic gaps

`routing.output` replaces the main destination. Graph `connect(kind:output)` adds
an output while retaining other destinations. They cannot be aliased directly.
Similarly, legacy send creation accepts level and pre-fader mode in the same call,
whereas graph creation and adjustment are separate operations. Legacy sidechain
replacement is also a single operation. Add explicit atomic output replacement,
configured send creation and sidechain replacement before hiding `routing` and
`effects`. Do not simulate these with a destructive intermediate disconnection.

Saved `mixer` also returns parameter detail that a graph overview alone does not.
The new read path must retain paged parameter keys, values, units, choices and
automation status. Resolve legacy effect slots to stable IDs and retain the
optional expected-effect consistency check from `set_effect`.

The live graph's eleven flat tools represent different operations, not eleven
copies of the same function. Keep their small schemas unless a model evaluation
supports a different presentation. A single `GraphAction` implementation can
serve these flat tools and the saved facade; the duplicate fader/switch entry
points are the immediate removal candidates.

### Similar names that should remain distinct

| Tools or controls | Reason to retain the distinction |
| --- | --- |
| `edit_recipe` and `set_performance` | Recipe changes regenerate stored notes; performance changes how the existing score is played |
| `preview_composition` and `preview` | One inspects a symbolic composition plan; the other creates actual audio |
| `preview` and `render` | Protected audition output versus delivery/stems, different rate, tail, range and file-lifetime contracts; share rendering internals, not an ambiguous default |
| `inspect_audio`, `analyze_music`, `analyze_audio`, `analyze_chords`, `analyze_instruments` | Rendered levels/spectrogram, authored-note statistics, source-audio tempo/chords, score-derived harmony and learned instrument-presence estimates have different inputs and meanings |
| `listen` and numeric inspection | The former sends real WAV bytes to an external critic; measurements do not establish subjective listening |
| `verify_render` and `render` | An existing encoded delivery file needs independent verification even if it was exported elsewhere |
| `inspect_composition` and basic `inspect_project` | Provenance, original specification and recipe comparisons add information beyond the arrangement summary |
| `set_instrument` and `set_singer_voice` | Plugin/sampler selection and singing-model/style selection have different lifecycle and validation requirements |
| `setup_tracks` and elementary track/clip operations | The batch has atomic setup and retry-receipt semantics; retain it until the common command transaction offers those guarantees |

## Missing workflow steps

| Priority | Gap in the current public surface | Planned addition or extension |
| --- | --- | --- |
| P0 | Live agents can compose and inspect but cannot export their unsaved mix/MIDI through a domain tool | Live export/preview and encoded-file verification using the current document snapshot; file authorization, staging and cancellation; no forced project save |
| P0 | No live clip move/copy/split/resize/remove/freeze operation | Expose existing session clip commands; expand saved `edit_clip` to the same stable-ID contract; distinguish freezing recipes from baking performance |
| P0 | Native CLI help mismatch and no shared live discovery | Runtime-scoped capabilities/discovery/help, as above |
| P1 | No live static instrument-parameter editor or automation editor | Shared parameter discovery and bounded batch writes; bring saved `set_instrument_params` and `automation` capabilities to the live session |
| P1 | No public clip bend/MIDI-controller editor on either catalog, including CC64 sustain | Paged read plus atomic merge/replace/clear of clip curves; preserve authored versus derived curves and MIDI value units |
| P1 | No saved-file `set_performance` equivalent; no live recipe editing/regeneration | Saved adapters for the existing performance command; live inspection/edit/regeneration with hand-edit protection and explicit seeds |
| P1 | Existing tempo-map and meter-map edits are not fully exposed live; existing-project meter editing is absent in saved tools | Extend timeline editing with explicit positions/ranges, tempo and meter, plus remove/restore behavior; do not move notes implicitly |
| P1 | No live audio/MIDI import or drum-assignment editing | Cancellable import into the open document and existing drum-map commands; MIDI import into a live project must not silently create/replace the document |
| P1 | No saved Basic Pitch tool equivalent, and voice inspection/selection are bundled differently | Shared transcription backend contract and common voice discovery/inspection/selection; retain existing saved `sing(voice,speaker)` compatibility |
| P2 | No public phoneme/timing/per-note singing ornament editing | One capability-aware vocal-note editor using existing session setters; refuse unsupported model controls |
| P2 | No named live checkpoint/history operation | Bounded history/checkpoint inspection and explicit restore after revision checks; prevent broad Undo from reverting unrelated user edits |
| P2 | Audio clip gain, stretch/tempo-following, fades and crossfades are not publicly editable | Extend clip editing by clip kind using the existing session commands |

Saved project create/open/save behavior is intentionally different from operating
on the open document. Adding export or import does not justify giving every live
agent the saved-file editor or allowing a CLI to rewrite `.auris` behind the window.
UI-only actions and every internal getter do not need separate AI tools.

## Shared implementation and compatibility rules

- Put document operations in `auris-session`; vocabulary, schemas, aliases,
  examples and catalog metadata in `auris-toolbox`; keep transports in their own
  frontends. Reuse `mixer_graph::GraphAction` and the existing performance/vocal
  worker boundaries rather than introducing a competing implementation.
- Define canonical capabilities once, with applicability, read/mutation status,
  confirmation needs, limits and aliases. Generate discovery, help, descriptions
  and transport registrations from that metadata. A runtime view may flatten a
  command, but must not advertise both its flat form and an equivalent facade.
  Permission classification stays with the session command; toolbox metadata
  projects that classification without adding an upward session dependency.
  Prefer ordinary registration/dispatch tables over a new code-generating macro.
- A saved adapter resolves the project and saves only changed results with one
  checkpoint. A live adapter edits the owned document with one Undo step and
  remains unsaved. Expensive jobs work on revision-bound snapshots off the UI
  thread and publish only to their originating document.
- Expose one canonical definition in `tools/list`. Keep deprecated handlers as
  unadvertised adapters for an announced compatibility window; examples and
  preambles immediately use the canonical form. Return bounded migration guidance.
- Resolve existing permission rules before executing aliases. A deny must remain
  effective, and allowing an old fader-only tool must not grant newly merged mute,
  solo, routing, file or network operations. Test affected-field permissions and
  preserve exact-command approval binding. Discovery filtering is not authorization.
- Standardize exact ticks and clip-relative quarter-note beats for machine-facing
  edits; retain explicit legacy conversion for meter beats and ordinal selectors.
  Return document revisions and exact note references. Do not persist new note IDs
  or change the project format merely to rename a tool.
- Validate complete batches before mutation; bound notes, curve points, file bytes,
  output size and work. Add request IDs/receipts where creating tracks, phrases or
  imports could be duplicated by a retried call. Preserve no-op retry behavior for
  setters and replacements. Never turn recognition or regeneration into implicit
  acceptance of a changed score.

## Implementation sequence and acceptance gates

1. **Catalog contracts and low-risk duplicates.** Add runtime-aware help/discovery,
   resolve the CLI preamble mismatch, publish exact note readback, and adapt live
   `add_note` to `add_notes`, fader/switch setters to `set_mixer_track`, and the saved
   scalar instrument setter to its batch form. Before additions, these changes
   remove three advertised live entries and one saved entry. They do not remove
   functionality. Acceptance: catalog uniqueness, help/list/call agreement for
   every runtime/group, aliases, units, one-note/batch parity, partial preservation
   and permissions. Start by fixing the current mismatches, not by loading all
   saved-file tools into the live agent.
2. **Finish local arrangement and delivery.** Add clip manipulation, live export,
   parameter/automation and clip-curve editing, saved performance and live recipe
   adapters. In the same units of work, complete the graph migration and unified
   note editing before hiding their superseded entry points. Acceptance: a live
   agent moves/splits a phrase, edits only requested controls, writes sustain,
   exports the unsaved result and verifies its encoded output; source preservation,
   one Undo, no-op retries, cancellation and stale-result rejection must pass.
3. **Align composition, recognition and timeline workflows.** Standardize
   generation/read names, consolidate sound search and transcription, and expose
   remaining import, drum, voice and tempo/meter operations. Acceptance includes
   Basic Pitch/YIN backend selection, explicit MuScriptor acknowledgement,
   unavailable-model errors, 6/8/mixed-meter timing and no implicit regeneration.
4. **Validate model use and retire compatibility handlers.** Run the same tasks
   through rig, built-in Claude/Codex and standalone saved MCP. Record successful
   calls, argument errors, repeated edits, loaded schema bytes, operation counts
   and wall time. Native CLI tests must actually invoke the canonical tools; a
   successful connection is insufficient. Remove hidden aliases only in a
   documented breaking release after compatibility and saved permission migration
   are tested. Tool counts are an audit metric, not the optimization target.

Each unit should update the guide/workflow text and catalog/policy tests together.
Run formatting, relevant session/toolbox/host/window tests and workspace Clippy.
Changing a composer writer or an audio constant additionally requires the existing
music evaluation baselines; merely exposing an existing command does not change it.

## Source anchors

- [Saved catalog and schemas](../crates/auris-toolbox/src/catalog.rs)
- [Live definitions and argument translation](../crates/auris-toolbox/src/live_agent.rs)
- [Actual live host filtering and execution](../crates/auris-agent/src/host.rs)
- [Rig-specific research tools](../crates/auris-agent/src/lib.rs) and
  [CLI instructions](../crates/auris-agent/src/backends/mod.rs)
- [Saved group discovery](../crates/auris-toolbox/src/tool_exposure.rs) and
  [MCP list/call filters](../crates/auris-mcp/src/main.rs)
- [Live command contracts](../crates/auris-session/src/live_agent.rs) and
  [permission operations](../crates/auris-session/src/agent_policy.rs)
- [Shared graph operations](../crates/auris-session/src/mixer_graph/commands.rs),
  [legacy routing](../crates/auris-toolbox/src/track_editing.rs) and
  [legacy effect editing](../crates/auris-toolbox/src/mix_editing.rs)
- [Saved notes and generation](../crates/auris-toolbox/src/lib.rs),
  [batch editors](../crates/auris-toolbox/src/batch_editing.rs) and
  [replacement input](../crates/auris-toolbox/src/replace_notes.rs)
- [Clip/CC commands](../crates/auris-session/src/session/clips.rs),
  [timeline commands](../crates/auris-session/src/session/transport.rs),
  [singer corrections](../crates/auris-session/src/session/singer.rs) and
  [recognition backends](../crates/auris-toolbox/src/recognition.rs)
