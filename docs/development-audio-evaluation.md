# Audio evaluator development experiments

This guide is for explicitly requested evaluator research and regression experiments in a source checkout. It is not part of the distributed composition skill or the normal Codex/Claude Code composition workflow. The evaluation service and models are managed separately by the developer.

For local SongEval scoring and bounded refinement, use [composition refinement](composition-refinement.md). Preserve before/after audio and provenance; do not turn a model score into an automatic musical winner.

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

When review fails, distinguish no configured reviewer, a connection failure and no advertised audio-input model. After cancellation, do not retry without a new request. When a structured assessment is used, require successful schema validation; never recover a positive judgment from raw text after validation failed. Keep actual audio hashes, ranges, model identity, validation state, numeric measurements and human feedback separate.
