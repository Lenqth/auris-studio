# Agent tools and workflows

Saved-project MCP, the built-in Claude/Codex backends and native API agents share
canonical session commands, schemas and validation. Saved calls add `project`
(an absolute path or project_id from open_project/create_project). Live calls act
on the open document, remain unsaved and use one Undo step. Changed saved calls
create one checkpoint; reads and unchanged setters do not save.

Call `inspect_project` first for stable numeric track/clip IDs. `discover_tools`
and `tool_help` describe only enabled tools in the current runtime. Saved MCP
startup groups come from `AURIS_MCP_TOOL_GROUPS=manual,mix` (or transcription,
vocals, composition, all). Disabled tools are omitted from discovery and listing;
help explains that their group must be enabled. All backends have documentation
search. Network search availability depends on the backend and permission policy.
The execution registry supplies names, descriptions, schemas and dispatch together.

## Exact authored notes

Add a track with an explicit kind and name, then `add_clip` with its numeric track,
intended clip name, one-based `start_bar` and duration `bars`. Read the clip with
`read_notes` before editing. Pages contain up to 128 notes, exact tick/velocity
values, storage indices and a content revision. Follow next_offset without editing
between pages. Indices start at zero and are checked against the supplied revision.

`edit_notes` accepts one batch of 1..16 clip edits. A patch addresses original
indices for updates/removals, then appends new notes. A replacement supplies a
complete note list; [] clears it. Updates preserve vocal metadata. Both forms
preserve clip curves, performance, recipes and playback windows.

```json
{
  "expected_revision":"<revision from read_notes>",
  "edits":[{"kind":"patch","clip":2,
    "update":[{"index":0,"velocity":0.7}],
    "add":[{"pitch":62,"start_tick":13,"length_ticks":997,"velocity":0.8}]}]
}
```

Times are integer clip-relative ticks. `ticks_per_quarter` is returned by reads;
there is no conversion to rounded meter beats. Notes must fit the clip, have a
positive tick length, MIDI pitch 0..127 and velocity 0..1. All changes validate
before a single atomic edit. Stale revisions are refused without changing notes.
Inline input allows 4096 note changes. For larger scores, supply `source` instead
of `edits`: an absolute local UTF-8 JSON file containing the same array of clip
edits, up to 16 MiB and 65536 note changes. The host reads it once and authorizes
that immutable draft before live execution. Input is never retained as an asset.

## Arrangement, curves and timeline

`edit_clip` takes `clip` and a typed `operation` with `kind`. Move/copy/split/resize
positions use absolute timeline ticks. Copy preserves the source; optional
transposition freezes only the copied recipe. Resize changes the playback window
without regenerating stored MIDI notes. `freeze_recipe` keeps the score and removes
its recipe; `bake_performance` explicitly commits performance into notes/curves.
Audio operations expose gain, recording tempo, tempo following, source-frame fades
and crossfades. Read IDs again after splits, copies or removals.

`edit_clip_curve` reads or edits an authored bend/controller lane. For sustain:

```json
{"clip":2,"curve":{"kind":"controller","number":64},"operation":{"kind":"read"}}
```

Copy the returned revision to `expected_revision`, then use `merge`, `replace` or
`clear`. Points contain `at_tick` and `value`. Controller values are normalized
0..1; bend uses semitones. Positions must be unique and lie inside the clip.
Derived performance and other authored lanes are preserved.

`edit_timeline` reads or edits positioned tempo and meter points. Positions use
absolute ticks and note positions stay unchanged. Read reports native BPM bounds.
Meter numerator is 1..32 and denominator a power of two within 1..32. Use `set_loop`
for a one-based bar playback range.

## Sound search, controls and mixing

`search_instruments` takes one `request` object: mode `text` with query, `similar`
with a reference id, or bounded `browse`. Each supports source/library filters and
up to 50 results. Text/browse support offset paging. Copy returned IDs into
`set_instrument` sound_id. IDs are scoped and expire on refresh, changed library
identity, eviction or restart. Acoustic indexing runs in the background; continue
other work while indexing and retry later. `instrument_diagnostics` pages existing
scan and measurement errors without rescanning. A library index in a response is
not a sound ID.

`inspect_instrument` returns native parameter keys, ranges, choice previews, values
and automation flags. `set_instrument_params` accepts one bounded `edits` batch
with numeric track, param and value. One parameter is a one-element batch.
Duplicate targets and invalid/discrete values are refused atomically. Existing
parameter automation is preserved and may override static values during playback.

`mixer_graph` is the single mixer entry point in every backend. Pass one `operation`
object tagged by action. Start with `read` and copy stable track/effect node IDs.
`set_strip` changes only supplied gain_db, pan, mute or solo fields; track:master
accepts gain/pan. `set_outputs` replaces destinations atomically; `connect` adds a
branch. `add_send` sets initial level and pre/post-fader tap in one edit; `set_send`
uses the returned send ID. `set_sidechain` replaces or clears a detector source.
Insert actions add/remove/move/enable effects and read/set native parameters.
All connections must form a DAG. See [Mixer signal flow](mixer-graph.md).

Use numeric audio inspection for measurements, then normalize_mix if a common
source-fader move is appropriate. It preserves relative balance and can stop short
at fader or peak limits. Sound replacement preserves faders but can change output
level substantially, so remeasure. Audio file delivery never establishes listening.

## Audio and MIDI delivery

`render` exports the current snapshot to destination kind `mix` with an absolute
path or kind `stems` with an absolute folder. Optional range uses start_bar/bars;
bit_depth is 16, 24 (default), or 32 float. `preview` makes a protected 24 kHz WAV
of a required range, up to 120 seconds. `export_midi` writes the score to output.
`verify_render` measures an existing path against loudness/peak/ending limits.
All backends use the same contracts; saved calls add project. Work runs off the UI
thread and private stages publish only for an unchanged, uncancelled document.
Existing output files are refused. No project save or Undo step is added by export.
See [Audio delivery](agent-audio-delivery.md) for examples and measured verdicts.

## Generated phrases

Use `generate_phrase` to add a generated MIDI clip to an existing instrument or drum track.
For example, `{"track":1,"part":"lead","start_bar":5,"bars":4,"seed":42}` writes four bars
starting at bar 5. Parts are `lead`, `chords`, `pad`, `arp`, `bass`, `drums`, `kick`, `snare`
and `hat`. `start_bar` is 1-based; `bars` is 1..1024; `seed` defaults to zero.
Melodic parts use the project's key and chords, so write their harmony with `edit_harmony`
first. Drum parts use the drum track's sound assignments and do not require chords.

The result reports the new clip ID, note count and seed. The clip retains its generation
recipe and stored notes, so it can be edited or regenerated through the usual clip controls.
Existing clips are preserved, one Undo removes the new phrase, and the document remains
unsaved. The same tool is available to native model APIs, Codex and Claude Code through
their shared live catalog and permission checks. Read-only and Plan modes refuse generation.

## Non-destructive performance

Call `inspect_performance` with a MIDI `clip` ID before changing its Performance controls.
Use `set_performance` with a partial `settings` object, for example
`{"clip":2,"settings":{"swing_percent":62,"gate":0.8,"expression":{"timing":0.15}}}`.
Only supplied fields change. Within a family, the first matching stage is edited in place;
other stages retain their order. Stored notes, generation recipes, authored controller curves
and automatic volume contour shapes remain intact. Each changed call is one Undo step;
repeating the current settings adds no history. The result reads back the updated settings.

The controls include humanize, swing/grid, gate, mute, brush, slide, transpose, lean,
octave copies, expression, strum, ghost notes, automatic pitch and reference groove.
Ratios use 0..1; swing uses 50..75 percent. Time controls use milliseconds except
`lean_ticks` (960 ticks per quarter note); pitch depths and transpose use semitones.
The tool schema supplies each field's range and enum values. Pitch and octave gestures
require a melodic instrument clip. To remove stages explicitly, pass a `reset` array,
for example `{"clip":2,"settings":{"reset":["swing","expression"]}}`.

Pass an exact `drum_voice` name from the inspector to edit one writer in a drum kit,
for example `{"clip":3,"drum_voice":"Hat","settings":{"expression":{"velocity":0.2}}}`.
Global performance and other drum writers are preserved. Omitting `drum_voice` edits
the clip's global stack. For a reference groove, first set
`{"groove":{"reference_clip":4,"timing":0.8,"velocity":0.3}}` inside `settings`.
Capture uses the reference clip's performed first pass on a sixteenth-note grid.
Later strength edits preserve that snapshot; changing the source does not recapture it.

These tools share the live catalog and permission checks across native model APIs,
Codex and Claude Code. Read-only and Plan modes permit inspection and refuse edits.
See the [real backend validation](agent-performance-validation.md) for completed
Codex and Claude trials, including actual rendering and Undo/Redo checks.

## Live harmony and vocals

For a new song, write its musical decisions into the document with `edit_harmony`, for example
`{"start_bar":1,"bars":16,"key":"D major","chords":"| I | vi | IV | V |"}`.
The chart repeats over the requested bars. The previous key resumes after the range;
existing interior key points remain. The operation edits the key/chord map, preserves notes
and mix, and is one Undo step. Audible chord notes alone do not populate this map.
`clear_chords:true` clears the selected range; omit `chords` when clearing.

Write the form into the section lane too. For example,
`{"start_bar":1,"bars":4,"section":"Intro"}` and
`{"start_bar":5,"bars":8,"section":"Chorus"}` are valid `edit_harmony` calls without
key or chord changes. A label covers its requested range, replaces interior section
changes and restores the previous label after the range. Labels must be nonempty,
at most 256 UTF-8 bytes and contain no control characters. Notes and mix are preserved;
one Undo reverts the range edit. Plan sections appropriate to the music when composing
a new song and verify their positions with `inspect_project`. Clip names and a prose
outline do not create section metadata. A short loop can remain one section.

### Live vocal production

Create a `singer` track and a note clip, then add its melody with `edit_notes`.
`list_voices` discovers installed voice entries in pages of 16; follow `next_offset`.
Discovery does not validate or download a model. Use a returned absolute `voice` path in
`set_singer_voice` with the numeric `track` ID. An optional `speaker` selects a named style.
`inspect_singer` reads back the chosen voice, available speaker names, and take state.
Speaker lists page at 128 names; follow `next_offset` as `offset` to read more.
To change speaker, repeat the selected voice path with the desired name; invalid names or
models leave the prior selection unchanged. Loading and inference use the existing local
automatic-access policy: network/device model paths and remote VOICEVOX URLs are refused;
local loopback VOICEVOX is supported and its Engine must already be running.

`write_lyrics` takes `clip` and `text`, for example `{"clip":2,"text":"さくらさいた"}`.
Kana is split into moras and phonemes; non-kana text requires the installed Japanese dictionary.
Omit `from` to begin at the earliest note, or copy a zero-based storage index from `read_notes`
to begin there in time order. This differs from the saved-file MCP tool's 1-based note number.
Unfilled later notes retain their words. Check the returned `filled` count and `available`
capacity and read back the notes before claiming the whole phrase was entered.

Call `sing` with the singer's `track` and optional `seed` to create the actual playback take.
The previous take seed is retained when omitted, or zero is used for the first take.
`inspect_singer` returns `take_state:"current"` on completion, `"missing"` before synthesis,
and `"stale"` when the score or voice has changed. A temporary guide voice is not a completed
vocal take. Voice discovery, loading, synthesis and WAV staging run off the UI thread.
Stopping the conversation cancels pending work; changed documents, superseded voice loads
and revoked permissions cannot accept its results. Explicit agent work pauses automatic
singer rendering until it finishes. Selection, lyrics and completed takes are separate
Undo steps and remain unsaved; synthesis stages its audio in the session's working folder.

All these live operations use the same permission gate for rig, Codex and Claude Code.
Read-only/plan mode can list voices and inspect singers but cannot edit harmony, choose a
voice, write lyrics, or apply a sung take. Errors are returned to the model, not reported as
successful song production.

### Listen to the unsaved document

The in-app Codex, Claude Code and rig controllers expose `listen` for the open document. It renders
the current unsaved mix and uploads actual WAV bytes to the configured audio critic,
independently of whether the controller accepts audio input. No project path or save is
needed. Configure `AURIS_AUDIO_URL` and `AURIS_AUDIO_MODEL` in the application environment
as described below; the server and model must already be available.

Call `listen` with `{"start_bar":9,"bars":4,"focus":"Describe the melody and accompaniment balance."}`.
It returns `audition_id`, the captured document revision/range, `audio_sent`, the critic's
review and measurements of both the source render and the actual encoded WAV. After a
supported local edit, read back the affected notes or mixer and call `listen` on the same
bars with `compare_to` set to the earlier `audition_id`. A tempo or meter change requires
a new baseline. The last four raw excerpts are retained in the open session; replacing
the document or closing the session expires them. IDs are opaque, not file paths.

Each request covers 1–8 bars and at most 30 seconds, rendered at 48 kHz without effect
tails. Both comparison excerpts use one common protective attenuation when needed for
the -1 dB estimated true-peak ceiling. Quiet audio is never increased. The source mix
stays unchanged, and source overloads remain visible in the measurements. Temporary
16-bit WAVs are decoded and measured before submission, then removed after the request.
The true-peak estimate uses 4x Catmull-Rom interpolation, not a certified BS.1770 meter.

Listening uses the `edit_project.listen` network permission. Existing exact approvals,
allow/deny rules and automatic review apply. It adds no Undo checkpoint. Rendering,
encoding and inference run on a worker; changed documents and stopped conversations
discard results. Cancellation prevents subsequent stages; an HTTP request already in
flight can run until its bounded timeout.

The controller receives a separate model's fallible observations. A successful upload
is not proof of musical understanding. When the critic cannot judge or its service is
unavailable, report that limitation and avoid unsupported edits. Only a supported
correction should lead to a second review; measurements remain separate evidence.

See the [backend validation](agent-listening-loop-validation.md) for the completed
Codex trial and the Claude Code trials blocked by the account session limit.

MCP retains the full saved-file tool catalog. The file-based composition, rendering and
listening workflows below describe MCP.


MCP exposes its saved-project commands through `auris-toolbox`.
Start with `capabilities` to check the General MIDI library, voice library paths and
project playback state. Registered sampler code is not a loaded SoundFont: when a
composition cannot load its requested GM preset, the session substitutes a built-in
instrument and reports the substitution. A singer without a rendered take plays a
temporary guide voice. Preview and export report missing instruments, guide vocals
and stale takes; `sing` produces or refreshes the actual vocals.

Voice discovery uses the same standard directories and additional `voice_paths` as
the desktop browser. Discovery lists candidate files without validating/loading them.
`capabilities` with a project also reports selected voice metadata and speaker names
when loaded. A playable preview is an audio artifact; a language model needs audio
input support to listen to it. Auris's rig Ollama adapter does not carry direct
audio attachments; use `listen` to send an audition through the audio critic's
OpenAI-compatible endpoint.

## Listen, revise, listen again

The initial [live composition audit](agent-listening-loop-audit.md) found a missing
live audio path. The live operation above addresses that integration; this section
describes the separate saved-project MCP workflow, whose `compare_to` is a WAV path.

The controller and the listener can be different models. Qwen/Ornith can choose
editing tools while `listen` sends the rendered WAV to an audio-capable critic.
The critic returns observations and has no tools that can modify the project.

1. Inspect the existing song with `describe`, `inspect_composition` and `mixer`.
2. Call `listen` with `project`, `start_bar:1`, `bars:4` and an optional `focus`.
   It returns `audio_path`, the critic model, its review, and separate measurements.
3. Decide whether the observations support one specific local edit. Leave the mix
   unchanged when no correction is warranted or the critic cannot judge it. For a
   supported correction, keep a named checkpoint and use local note, clip,
   instrument, routing, effect or gain edits.
4. After editing, read back the affected state, then call `listen` on the same bars. Pass the earlier
   `audio_path` as `compare_to` for an A/B review. Both actual audio files are attached.
   Saved-file previews can have different protective attenuation; inspect their
   measurements before interpreting absolute level differences. Live comparisons
   above instead share one attenuation for both immutable excerpts.
5. Keep, refine or restore the change according to the evidence. Stop when the
   requested result is met or the remaining uncertainty needs human judgment.

Keep `focus` concise and about the sound, such as "Describe the melody's prominence
relative to the accompaniment." Omit it to use the default instrumentation and
balance question. The session supplies the reviewer's grounding instructions;
focus does not need to repeat them or prescribe a finding. A local same-audio
comparison recovered music descriptions with a shorter question, but the resulting
details and suggested changes still require scrutiny.

The experimental default uses `gemma4:e2b` at `http://localhost:11434/v1`. Local
controls verified speech input with this model, but did not establish musical
discrimination. An answer about missing speech is not an assessment of instrumental
music. The server must already have the model. Configure a validated audio-capable OpenAI-compatible
server through the host environment before starting MCP or the agent:

```powershell
$env:AURIS_AUDIO_URL = 'http://localhost:11434/v1'
$env:AURIS_AUDIO_MODEL = 'gemma4:e2b'
# Optional: name an existing environment variable that holds the server's key.
$env:AURIS_AUDIO_API_KEY_ENV = 'MY_AUDIO_API_KEY'
```

`audio_sent` records successful transmission, not verified musical understanding.
Do not treat an HTTP 200, a fluent response, or numeric measurements as proof that a
model heard the excerpt correctly. If the critic says it cannot hear, report that
response as a limitation and check the request and audio backend, or ask for human
listening feedback.
Validate a new listener with known audible differences before trusting its advice.
Treat a completed request as transport evidence only; it does not establish that the model
heard a difference or that its musical judgment is reliable.

An optional [local audio service](../tools/audio-review/README.md) provides
the same endpoint independently of Ollama, with Qwen2-Audio and an opt-in native
MusicFlamingo backend. It includes GPU/CPU setup, revision pinning and bounded WAV
requests. For the MusicFlamingo research configuration, start its backend as shown
in the service guide, then configure the Auris process:

```powershell
$env:AURIS_AUDIO_URL = 'http://127.0.0.1:11435/v1'
$env:AURIS_AUDIO_MODEL = 'nvidia/music-flamingo-2601-hf'
```

MusicFlamingo's weights are restricted to noncommercial research by their NVIDIA
license. The bridge reviews paired excerpts independently and labels each result;
it explicitly reports that the audio model made no direct A/B judgment. The
controller can compare those observations, but should retain that distinction.
The local service downmixes audio to mono at 16 kHz and reports this in every
response. Its reviews cannot assess stereo placement, even if the model claims
otherwise. Incomplete reviews are errors with retry guidance; the MusicFlamingo
pair path allows each independent observation the same maximum output budget as
a single review.
Successful model loading and music recognition still require separate validation
of proposed corrections. The local trial found Qwen2-Audio's comparisons unreliable
and MusicFlamingo's descriptions useful but fallible; neither is an automatic
guarantee of mix quality.
The tested AMD Windows audio runtime also had a native crash during a later model
reload. A running HTTP service and an earlier successful review do not establish
restart reliability; inspect `/healthz` and the server log when a request fails.

## Ollama configuration and failure recovery

Settings → Agent exposes context size (32K, 64K, 128K, 256K) and thinking (model default,
off, on), stored in shared preferences. Output token limit selects 4K, 8K, 16K,
32K, or 64K per response and applies to the next request. Raising output also raises
the context window when needed to leave room for input; lowering context can lower
the output limit. The default request sets `options.num_ctx=32768`,
defaults each completion to `options.num_predict=4096`, and uses temperature zero for
repeatable tool arguments rather than inheriting a
model's chat sampling preset. These are request settings and do not change the
installed model or Ollama server configuration. The requested context is
independent of Ollama's server default. Values below 16384 are refused because the
tool catalog alone needs substantial context. Model discovery reports requested
context separately from the architecture's maximum. Preflight checks tool support
and refuses requests exceeding a reported model maximum.

Before each model call, the context guard budgets the tools, preamble, history and
4096 tokens of output reserve. After a response reports its input token count, an
unchanged request prefix uses that measured count plus an estimate for newly added
messages. The estimate counts message content directly, without counting JSON
transport escaping again, and allows extra room for non-ASCII text. First calls,
changed or shortened histories, unavailable usage counts and opaque media use a
conservative structural estimate. These fallback counts are not tokenizer measurements.

Before a new request, old completed conversation exchanges can be removed together
to make room. The current request and its in-progress tool calls and results remain
intact. If the remaining budget exceeds the window, the agent stops with instructions
to increase context or start a fresh conversation. OpenAI-compatible providers keep
their own context policy and do not receive Ollama-specific parameters.
An Ollama response stopped by the generation limit is an incomplete turn, even if
it contains text or a syntactically valid tool call. The agent reports that limit
before executing those calls; previous live edits remain available for a smaller
follow-up request.

The panel's context gauge uses input tokens from the last model request, rather than
adding the inputs of every tool-loop step. JSON answers expose that count as
`input_tokens`, with the run's aggregate separately available as `total_input_tokens`.

Five minutes without provider progress ends a stalled model request. Tool work is
allowed to finish, and active multi-step composition can exceed five minutes overall.
Two failures with identical tool arguments stop a third execution; changing arguments
allows a correction. Successful calls clear that signature's failure count. Interrupted
JSON conversations retain the request and interruption summary so a following turn can
inspect the current document and resume. Completed live edits remain in the session.


## Agent permissions and context compaction

The Agent Panel has five persistent modes, following the permission model in
[Picocode](https://github.com/uthree/picocode):

| Mode | Behavior |
| --- | --- |
| Read-only | Read immediately; reject document changes and request approval for internet search unless allowed by a rule. |
| Edit | Apply ordinary live edits; confirm removals, arrangement replacement, and internet search. |
| Plan | Inspect and propose a plan; reject document changes, including allow-listed changes. |
| Auto review | Apply ordinary live edits; have an independent model review operations that would require confirmation in Edit mode. |
| Bypass | Skip confirmations while continuing to enforce deny rules. |

Deny rules win over every mode and allow rule. Read-only and Plan reject document
changes before allow rules are considered. The Permissions section offers Default,
Allow, and Deny for each exact operation. Its `edit_project.*` and `*` rows switch
only between Default and Deny; deliberate broad allowances remain available through
the `/allow` command.
An approval shows the project, operation and exact arguments. Allow once applies
only to that command at the current document revision; edits made while awaiting
approval invalidate it. Always allow saves an operation rule. Rejecting an operation
returns a refusal to the model. Changing modes or rules cancels pending approvals.
Stopping the agent cancels its pending work.

Starting a new conversation asks for explicit confirmation before it stops active
work, clears the visible transcript, and deletes the saved conversation history.
Cancelling the confirmation leaves the conversation and active work untouched.

Use the mode buttons or `/mode read_only|edit|plan|auto_review|bypass`. Shift+Tab cycles the
three ordinary modes; bypass requires an explicit choice. While an approval is
pending, Shift+Tab denies it without changing the mode. `/permissions` opens
rules, and `/allow OPERATION`, `/deny OPERATION`, `/default OPERATION` edit them.
While confirmation is pending, Escape denies, Ctrl+Enter (Command+Enter on macOS)
allows once, and adding Shift always allows.

These modes govern live tools in every backend. Rig exposes the live document
catalog. Codex and Claude Code also expose native shell and file tools. Native
approvals appear as `cli_command`, `cli_file_change`, and (for Claude) `cli_file_read`
in Permissions. File changes and commands require confirmation in Edit mode unless
allowed by an exact or broad rule. Read-only and Plan reject native command and
file-change approvals even when allow-listed. Native file changes and command effects
are outside Auris Undo. Edit the open song through live tools rather than rewriting
its `.auris` file behind the window.

CLI tools start in the saved project's folder. An unsaved song uses temporary
scratch space that is removed when the conversation stops; name an explicit
persistent destination for deliverables. Codex retains a read-only sandbox and an
untrusted-command approval policy, so trusted sandboxed reads do not prompt. Writes
and elevated commands go through the panel's approval gate. Claude exposes Bash,
Read, Glob, Grep, Write, Edit and NotebookEdit, using ask rules to route native calls
through the panel. Unrelated MCP servers, plugins and hooks remain isolated.
Standalone MCP retains its saved-file tools and does not apply the panel policy.

Select **Auto review** or enter `/mode auto_review` to have a model approve on your
behalf. The reviewer uses the selected backend and model in a separate conversation
without tools. It assesses the human requests, exact arguments, command text and
available file diff. Clear, low-risk work can receive a one-time approval; clear
refusals block the operation. Uncertain decisions, timeouts, unavailable models,
oversized input and invalid responses return to manual confirmation. The transcript
shows the decision and reason. You can approve or deny manually while review runs;
doing so cancels the pending review. A changed document invalidates a mutation
approval, and late results cannot approve another request. Review never creates an
Always allow rule and never overrides a deny rule. It adds a model request for each
review; it is a fallible recommendation, not an operating-system sandbox. Bypass
continues to skip both human confirmation and automatic review.

The reviewer also receives bounded current project-script snapshots and recent
completed native calls. Long repeated command text is encoded without losing its
contents. See [automatic approval evidence](agent-review-evidence.md) for the
collection limits, integrity checks and remaining file-change race boundary.

Compact context or `/compact` summarizes older exchanges with the configured model,
without granting that model tools. The latest two completed exchanges are kept
verbatim. The summary retains goals, constraints, decisions, identifiers, progress,
failures and unfinished work. It is saved with conversation history and restored
when that conversation resumes. An empty, oversized, or failed summary leaves the
original history intact. Very large exchanges that cannot fit a summary request
also remain intact.

Automatic compaction runs between requests at the selected estimated history
threshold (70% or 85%; default 85%), or after 16 completed exchanges. It can be
turned off in Permissions. Counts are estimates, not tokenizer measurements;
the existing per-request context guard remains the final budget check. Compression
never runs in the middle of a tool operation or an approval request.
