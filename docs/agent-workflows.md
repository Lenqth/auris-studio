# Agent tools and workflows

[日本語](agent-workflows.ja.md)

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

Live backends include `search_internet` only with a nonempty `BRAVE_SEARCH_API_KEY`.
It uses the common host, runtime discovery/help and network permission gate in
Claude, Codex and rig. Transmitted schemas and default `tool_help` responses omit
explanatory field prose while retaining every validation constraint. Request
`tool_help` with `name` and `operation` (for example `mixer_graph`, `set_strip`)
for the selected operation's full field explanations and examples.

## Exact authored notes

For an authored foreground with generated backing, use
[`accompaniment`](agent-accompaniment.md). It protects explicit melody clip IDs, delegates
backing to Auris writers, and can change harmony while regenerating only selected backing
clips in one edit. It is available in both the `manual` and `composition` groups.
Before revising harmony, `suggest_harmony` reads the selected melody and proposes short
progressions with per-bar reasons, tensions and a plain alternative. Mood affects how much
supported colour earns its cost; no proportion of 7th, sus4 or diminished chords is required.
The proposal's revision and harmony object can be passed to selective backing regeneration.
It is read-only and available in the same two tool groups.

Add a track with an explicit kind and name, then `add_clip` with its numeric track,
intended clip name, one-based `start_bar` and duration `bars`. Read the clip with
the returned `clip` address, not the owning `track`. `inspect_project` uses
`tracks[].track` and `tracks[].clips[].clip`, matching creation results and tool
arguments. These are distinct numeric addresses; no positional track numbering
or implicit track-to-clip conversion is performed. Use
`read_notes` before editing. Supply `clips:[{"clip":2},{"clip":3,"offset":128}]`
for one to eight distinct clip pages. The response has one top-level content
`revision` and a `clips` array. Each page contains up to 128 notes, exact ticks,
normalized velocities, original storage indices, `truncated` and `next_offset`.
Follow each clip's next offset without editing between pages. Indices start at zero
and are checked against the supplied revision.

`edit_notes` takes a top-level `expected_revision` and one typed `input` object.
`input.kind="inline"` accepts a batch of 1..16 clip edits. A patch addresses original
indices for updates/removals, then appends new notes. A replacement supplies a
complete note list; [] clears it. Updates preserve vocal metadata. Both forms
preserve clip curves, performance, recipes and playback windows.

```json
{
  "expected_revision":"<revision from read_notes>",
  "input":{"kind":"inline","edits":[{"kind":"patch","clip":2,
    "update":[{"index":0,"velocity":0.7}],
    "add":[{"pitch":62,"start_tick":13,"length_ticks":997,"velocity":0.8}]}]}
}
```

Times are integer clip-relative ticks. `ticks_per_quarter` is returned by reads;
there is no conversion to rounded meter beats. Notes must fit the clip, have a
positive tick length, MIDI pitch 0..127 and velocity 0..1. All changes validate
before a single atomic edit. Stale revisions are refused without changing notes.
Inline input allows 4096 note changes. For larger scores, choose
`input:{"kind":"file","source_path":"/absolute/notes.json"}`: a local UTF-8 JSON
file containing the same array of clip
edits, up to 16 MiB and 65536 note changes. The host reads it once and authorizes
that immutable draft before live execution. Input is never retained as an asset.

## Live request history

The desktop groups edits from one user request in the existing Session Undo stack,
without keeping a gesture open or delaying playback. `history` is live-only:
`{"operation":{"kind":"read"}}` returns bounded undo/redo entries, an
`available_group` for each direction and a numeric `revision`. Reverse that exact
group with `{"operation":{"kind":"undo","group":2,"expected_revision":123}}`;
`redo` has the same fields. Use this instead of reconstructing earlier note arrays.
Human edits and open gestures are boundaries; stale or interrupted groups are
refused. Empty requests add no step. Checkpoints serve separately named milestones.

## Arrangement, curves and timeline

With `AURIS_ARRANGEMENT_HARNESS=1`, use `arrangement` to inspect capabilities and phrase
conditions, prepare a bounded section-specific proposal, and apply its exact plan as one
Undo edit. See [arrangement proposals](arrangement-harness.md) for revoicing/layers,
scoped automation, SFX alignment, permissions and verification.

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

`automation` selects a typed target (`track_gain`, `track_pan`, `master_gain`,
`master_pan`, `send`, `instrument` or `effect`) and an operation. `read` pages
points and choices in native units. Copy its revision into `expected_revision`
for `set` or `clear`. `set` accepts up to 4096 `{at_tick,value}` points, optional
`replace` and `curve` (`linear` or `hold`). Absolute ticks, discrete parameter
steps and all points validate before mutation. Static controls remain unchanged.

## Audio and MIDI delivery

`checkpoints` lists named snapshots, creates a new named snapshot or restores one.
Use operation kind `list`, `create` or `restore`. Restoration requires
`expected_revision` from `inspect_project`, retains the current file path and is
one Undo step. Both the supplied revision and worker ownership are checked, so
unrelated edits made while loading are protected. Existing checkpoint names are
never replaced. Snapshots retain asset references; audio files are not duplicated.

`import_audio` and `import_midi` append to the existing document at `at_tick`.
Both take an absolute local `path`, preserve the current tempo/meter maps and run
copy/decode/parse work off the UI thread. Audio imports retain an independent
source copy; an optional `name` labels the new track. MIDI imports preserve note,
bend and controller timing, using default instruments that need timbre review.
Cancellation or a changed document discards the result. Each import is one Undo.

`render` exports the current snapshot to destination kind `mix` with an absolute
path or kind `stems` with an absolute folder. Optional range uses start_bar/bars;
bit_depth is 16, 24 (default), or 32 float. `preview` makes a protected 24 kHz WAV
of a required range, up to 120 seconds. `export_midi` writes the score to output.
`verify_render` measures an existing path against loudness/peak/ending limits.

Agent `render` measures the exact requested waveform before integer encoding and
applies attenuation only as needed for `ceiling_db` (default -1 dBTP, range
-12..-0.1). It returns `source_true_peak_db`, nonpositive `attenuation_db`, the
ceiling and measurements of the actual encoded file. This changes neither the
document nor its mixer, notes, dynamics or duration. It is uniform output gain,
not a limiter or loudness normalization. Stems share one gain to preserve their
relative levels. Quiet exports remain unchanged. Earlier mix measurements can
become stale after arrangement edits; exported bytes are checked before publication.
Peak protection can prevent a loudness target from being reached, so verify the
actual result and report that constraint instead of claiming both targets passed.
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

`edit_recipe` reads, patches or regenerates 1..16 explicit clip IDs. A `patch`
changes only supplied recipe settings; `regenerate` selects the same, next or an
explicit seed. Both regenerate stored notes. `subdivision` is one of `eighth`, `sixteenth`,
`eighth-triplet`, `sixteenth-triplet`; it sets the grid, not held note duration.
`rhythm` repeats grid steps: `x` hit, `X` accent, `o` ghost, `.`/`~`/`-` rest, with optional
spaces, tabs and `|` separators (for example `x...x...`). Empty clears the pattern.
Use `density` and `gate` for note count and held length. Enum and rhythm constraints
remain in compact schemas sent to every backend. Hand-edited notes require
`replace_hand_edits:true`; otherwise the complete batch is refused. Optional
`drum_voice` selects a single drum lane. `set_drum_assignment` changes a drum role's
MIDI note without rewriting notes or other mappings.
Existing clips are preserved, one Undo removes the new phrase, and the document remains
unsaved. The same tool is available to native model APIs, Codex and Claude Code through
their shared live catalog and permission checks. Read-only and Plan modes refuse generation.

## Non-destructive performance

Call `inspect_performance` with a MIDI `clip` ID before changing its Performance controls.
Use `set_performance` with a partial `settings` object, for example
`{"clip":2,"settings":{"swing_percent":62,"gate":0.8,"expression":{"timing":0.15}}}`.
Only supplied fields change. Within a family, the first matching stage is edited in place;
other stages retain their order. Stored notes, generation recipes, authored controller curves
and automatic volume contour shapes remain intact. The live host groups changed calls by
user request; repeating current settings adds no history. The result reads back settings.

`note_effects` measures the first loop pass through the actual performer, with at most
2048 notes and 32 stages. It reports changed onsets, durations, velocities and pitches,
added/removed notes, simultaneous chord groups, warnings and explicit truncation.
Strum spreads simultaneous pitches: a single-note arpeggio may retain a strum setting
that changes no events. Do not claim an effective stroke in that case. Pitch curves,
timbre and volume contours are outside note-event measurements; `pitch_effects` separately
reports controller changes. Select `drum_voice`
to inspect a named writer's stages; whole-clip measurement stops before nested scopes.
Time-grid generators on clips longer than 64 quarter notes also stop measurement,
so sparse long clips cannot make an inspection scan the entire song grid.

Choose `settings.slide` for a brief discrete intermediate note. It connects an unambiguous
single-note line with at least two semitones of movement, no overlapping notes and no gap
longer than a quarter-note beat. `note_effects` reports inserted notes and transition
conditions; a stored strength with zero inserted notes is not an effective slide.

Choose `settings.pitch.glide_ms` for a continuous pitch connection. It follows the final
performed notes, after gate and timing edits. Notes must be isolated, at least 80 ms long,
at most 60 ms apart and within two octaves. Repeated pitches need no pitch movement.
The independent `pitch_effects` report compares the actual combined bend with only glide
disabled, retaining authored bends and other gestures. It therefore distinguishes a
working glide from a bend that was already present. Incomplete measurements are unknown,
not success or evidence of no effect. Diagnostics describe the first loop pass only.

Each `pitch_effects[].report` labels its top-level `effective` as the contribution of
`control: "glide_ms"`. `generated_curves.bend` measures that same isolated contribution;
`all_bend_gestures` measures all bend gestures in that stage. `modulation` and `volume_swell`
measure the corresponding controller separately. Each comparison disables only that
control, preserving other stages and authored curves. `measured: false` and
`effective: null` mean unknown. Curve work is bounded to 2,048 source/performed notes,
32 stages, 64 quarter-note beats, 30 seconds and 2,048 comparison timestamps.
`instrument_response: "unknown"` requires an audio check to establish the patch's response.
Eligibility includes `maximum_gap_ms` from the tempo map and `gap_limit_ms`; use those
measurements when explaining rests instead of guessing their duration from beat labels.

Read back with a separate `inspect_performance` call. If an articulation has no effect,
explain its conditions without rewriting the score or filling intentional rests. Compare
the same short rendered range before and after when verifying audible response, especially
for hosted patches whose controller mapping is unknown. A changed control curve or audio
signal is not a musical quality judgment.
Local `inspect_audio` and performance diagnostics cover this mechanical verification;
external `listen` criticism is optional and retains its normal permission requirements.

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

These tools share the canonical catalog across saved MCP and live native model APIs,
Codex and Claude Code. Read-only and Plan modes permit inspection and refuse edits.
See the [real backend validation](agent-performance-validation.md) for completed
Codex and Claude trials, including actual rendering and Undo/Redo checks.

## Live harmony and vocals

Use `import_transcription` to accept externally prepared note JSON, supplied as exactly
one inline `transcription` or absolute JSON `path`. Optional `at_tick` controls placement
and `name` names the piano track. Notes remain estimates; review timing and timbres.
See the [external note API](transcription-api.md) for its validation and permission contract.

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

The shared harness treats a new vocal-song request without a short-form limit as
a complete song: plan and write the second verse and later chorus/ending as well
as the first verse. Explicit lengths, one-chorus demos, excerpts and local edits
retain their requested scope. Plan actual section ranges before writing notes;
give later verses their own lyrics and narrative development, with intentional
melodic reuse and accompaniment changes. A first-verse sketch is an intermediate
result. This guidance applies to both default and experimental prompt profiles.

`inspect_singer` includes `section_coverage`, even without a selected voice or a
rendered take. Named sections report note attacks, lyric-bearing notes, phoneme
assignments, continuation markers, unassigned notes and attacks from clip loops.
Use `section_coverage.next_section_offset` as `section_offset` to read the next
page of at most 32 sections; this is independent of speaker-list `offset`.
Explicit section endings include planned later sections beyond the current score.
`unbounded_last_section` flags a final label with no inferable positive range.
Counts cover authored onsets in unmuted clips, before performance; sustained notes
count only where they begin. `track_muted` separately reports the track state.
Inspection is read-only and caps note checks at 100,000 per page. A section with
`analysis_complete:false` has partial counts, which cannot establish absence.

Compare coverage with the planned sung sections and read back their actual
notes/lyrics. Empty instrumental introductions, breaks and endings are valid.
Looped lyrics do not establish a newly written second verse; positive counts or
a current take alone do not establish complete lyrics, correct pronunciation or
musical quality. Synthesize the complete score when requested. If synthesis is
blocked, preserve the score and report the specific unfinished work.

The [vocal form smoke cases](../tools/eval/references/vocal-song-form-v1.json)
exercise an unspecified song and a 16-bar one-chorus request. Run them with
`composition_quality.py run --manifest tools/eval/references/vocal-song-form-v1.json
--no-candidate-tools --repetitions 1` and the normal executable/output arguments.
The [recorded score checks](../tools/eval/references/vocal-song-form-validation-2026-10-24.json)
cover initial completion and readback; they do not evaluate singing or listening quality.

`edit_vocal_notes` applies 1..256 ordered lyric, phoneme, timing-pin or pitch-ornament
corrections to one singer clip. Supply `expected_revision` from `read_notes`.
`inspect_singer` reports manual-phoneme/timing support and the model frame hop;
unsupported controls are refused. Note/phoneme indices must exist. Scoop/fall and
vibrato retain the original pitch, duration and velocity. A failed batch rolls back
all its corrections. Run `sing` again after changing a rendered score.

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
to begin there in time order. Saved and live calls use the same storage indices.
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
bars with `compare_to` set to the earlier `audition_id`, `aspect` set to `balance`,
`phrasing`, `ensemble`, `sound` or `musical_coherence`, and `focus` omitted. A tempo or meter change requires
a new baseline. The last four raw excerpts are retained in the open session; replacing
the document or closing the session expires them. IDs are opaque, not file paths.

Local endpoints use automatic reviewer selection by default. Before uploading audio, Auris
checks the configured endpoint, the audio bridge on `127.0.0.1:11435`, and installed Ollama
models on `127.0.0.1:11434`. A model must advertise audio input: the bridge's `/v1/models`
returns `capabilities: ["audio"]`, and Ollama models are verified through `/api/show`.
The configured model is preferred; other Ollama candidates are checked in ascending download
size, with at most 24 candidates and a 20-second discovery budget. This is a memory-oriented
selection policy, not a music-quality ranking. Ollama models marked as remote are excluded.

`reviewer_selection` reports whether discovery ran, the originally requested model and whether
a fallback was chosen; `model` identifies the actual reviewer. No candidate produces an explicit
error with setup guidance. Discovery does not start servers or download weights. Remote API
settings remain fixed, and credentials are never forwarded to a fallback endpoint. Set
`AURIS_AUDIO_AUTO_SELECT=false` to pin a local endpoint, including compatible servers that do
not expose audio capability metadata. Once audio is submitted, generation failures, refusals
and truncated responses are returned without retrying another model.

Single-excerpt diagnosis accepts a descriptive question. Comparison accepts only a neutral
criterion: the critic receives anonymous A/B recordings in randomized order, with no edit
narrative. The response contains short free text in `review`, with `review_format` set to
`plain_text`. The original response is retained without extracting a verdict, confidence or
preference; `audio_sent` records successful delivery, not accurate perception. The returned
A/B mapping is revealed to the caller after review. Music Flamingo describes the two excerpts
independently and explicitly labels its output accordingly. These observations must not
automatically select edits. See [critic calibration](audio-critic-calibration.md).

Each request covers 1–8 bars and at most 30 seconds, rendered at 48 kHz without effect
tails. Both comparison excerpts use one common protective attenuation when needed for
the -1 dB estimated true-peak ceiling. Quiet audio is never increased. The source mix
stays unchanged, and source overloads remain visible in the measurements. Temporary
16-bit WAVs are decoded and measured before submission, then removed after the request.
The true-peak estimate uses 4x FIR interpolation, not a certified BS.1770 meter.

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

Saved-project MCP and live agents use the same listening command, source rendering,
protected WAV encoding and immutable comparison IDs. The controller chooses edits;
a separate audio critic receives actual audio and cannot modify the project.

1. Read `inspect_project`, `mixer_graph` and the relevant notes or controls.
2. Call `listen` with explicit `start_bar`, `bars` and optional `focus`. Saved calls
   add `project`. The result includes `audition_id`, measurements and the critic's
   review. It does not establish that the controller personally heard the file.
3. Make one supported local correction, or preserve the music when the evidence
   does not justify a change. Keep a checkpoint when a comparison may need restoring.
4. Read back the edit, then call `listen` on the same range with `compare_to` set
   to the first `audition_id`, a neutral `aspect`, and no `focus`. Both excerpts use the same protective attenuation;
   the original raw excerpt stays immutable. Tempo/meter or range changes require
   a new baseline.
5. Keep, refine or restore the edit according to the evidence, and stop when the
   requested result is met or a human judgment is needed.

Each live document retains its latest four excerpts. Saved MCP retains those same
bounded contexts for two projects in the current server process; eviction/restart
expires their IDs. Reads, reviews and previews do not save or alter faders. Use
`preview` for a playable local file; its attenuation applies only to that audition.

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

## Live agent system prompt

The standard live prompt starts with musical decisions: motif development,
phrase direction, lyric prosody, harmonic motion, section contrasts, arrangement
roles and a complete ending. The model chooses a writing order suited to the brief
and can author accompaniment or use a generation recipe. It completes the requested
form before repeated local polishing, checks the actual score and rendered passages,
and distinguishes those observations from listening preference.

The editable source is `crates/auris-agent/src/prompts/composition.md`; the standard
profile also includes `prompts/live-tools.md` for operation contracts. Shared vocal
form, phrasing and performance guidance is appended by `default_system_prompt`.
The experimental modular profile uses the same musical guidance with its workflow
instructions. Codex and Claude Code receive the same resolved composition prompt.

Settings → Agent includes a multiline **System prompt** editor shared by Codex,
Claude Code and Rig models. It starts with the current standard prompt in full.
Editing replaces the common prompt; it is not appended to the standard text.
Enter inserts a line break; the platform Control/Command+Enter shortcut applies
the settings. The editor supports Japanese input, clipboard operations and wheel
scrolling without moving the selection.

**Apply** saves the prompt in the application's agent preferences and uses it
from the next request. A running request finishes with its original settings.
Closing Settings without applying leaves the saved prompt unchanged. **Restore
standard prompt** restores the draft; apply it to resume following built-in
defaults, including future updates. A blank prompt also selects those defaults.
A failed save retains the draft and leaves the active preferences unchanged.

This edits the live agent's shared instructions. Native CLI tool guidance,
operation schemas/help, optional workflow responses and permission checks remain
separate. Approval-review and audio-critic instructions are independent. Saved-file
MCP uses its own instructions. Custom prompts count toward the local model's
context budget just like the standard prompt.

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
