# Model training integration

The saved-project MCP and live agents use the same session commands. Dataset
storage, labels, source-work splits, inference and replay policy belong to the
collector. Retrieve `capabilities` and MCP `tools/list` at the start of a run;
`tool_schemas` contains per-tool groups and SHA-256 schema hashes, with a catalog
hash for the enabled tool set. Preserve the exact schemas alongside the run.

## Exact analysis output

`render` accepts an optional `analysis` object. This uses the existing immutable
snapshot, cancellation, permission and no-replace publication gates:

```json
{
  "destination": {"kind": "mix", "path": "C:/Dataset/example.wav"},
  "analysis": {
    "sample_rate": 48000,
    "range": {"unit": "frames", "start": 0, "end": 14400000},
    "manifest_path": "C:/Dataset/example.manifest.json"
  }
}
```

Output is stereo float32 WAV, with no gain change, clipping, normalization or
dither. Finite samples outside full scale are retained. Each range is at most
300 seconds; seconds can be used as `{"unit":"seconds","start":1.025,"end":2.5}`.
Seconds round to the nearest output frame. Frame indices use the requested output
rate and the end is exclusive. The output is not subject to `listen`'s short bar
limits. All requested frames are emitted, including silence beyond the score.
Do not also supply the ordinary bar `range`, `include_tail`, or a delivery ceiling.

`track` selects an existing source or bus through its solo routing, inserts,
fader, sends and master. A stem destination exports each source through that same
routing; summing independently rendered wet stems need not equal the full mix.
`dry:true` requires one source track and bypasses its mixer, inserts, sends, bus
routing and master processing. Instrument controls, note performance and clip
processing remain active. It does not mean an acoustic instrument's internal dry
microphone signal. Use separate explicit renders for vocal and accompaniment parts.

`context_from_start:true` processes from timeline zero and discards frames before
the requested start, retaining causal effect context. `looping:true` instead warms
up the requested cycle using the engine's cycle/tail rules. The two are mutually
exclusive. No extra tail is appended: include the desired release in the explicit
range. The manifest records these choices and the exact source-frame mapping.

Audio rendering streams processing blocks. Encoded loudness inspection and input
validation have bounded whole-waveform allocations; a request is not an arbitrary
length streaming protocol. Existing decoder and sample allocation limits apply.

The manifest and WAVs are persistent outputs, independent of audition caches.
Publication refuses stale document/plugin states or changed resolved dependencies.
A multi-file publication error identifies files already published. Copy outputs
into the collector's own durable storage before removing their chosen directory.

The manifest binds all files to document and render-snapshot hashes, session and
document identities, build/binary identity, tempo/meter maps, automation, source
range, signal route and encoded byte hashes. Source and encoded measurements are
separate. Ordinary protected `render` output also reports its source measurement
and actual attenuation. Render snapshots capture current hosted patch states and
parameters without changing the source document. Plugin metadata includes format,
vendor/version when available and state identity. Source files, SoundFonts, plugin
files/bundles and singer model/connection references are hashed without copying
their contents into the manifest. `loaded_audio` fingerprints decoded buffers
actually available to the render, separately from files on disk. Missing/unreadable
references and guide/stale vocals are reported; unavailable render processors fail
analysis export. A hash of a connection file identifies that configuration, not the
remote engine's model files. Determinism remains explicitly uncertified.

## Agreed audio input

Configure `AURIS_AUDIO_INPUT_CONTRACT` with a JSON agreement for the selected native
OpenAI-compatible audio model and reviewer. For example:

```json
{
  "sample_rates": [48000],
  "channel_order": ["left", "right"],
  "sample_formats": ["float32", "pcm24"],
  "max_seconds": 300,
  "max_bytes": 134217728,
  "max_total_bytes": 268435456,
  "max_inputs": 2,
  "simultaneous_comparison": true
}
```

Set this only after agreeing with the model server about its decoder and context
handling. A client-side agreement does not prove that a server or model uses both
channels. Verify decoded values at that server's model boundary. The bundled
16 kHz mono review models retain their own preprocessing; they are different input
contracts from a custom 48 kHz stereo model.

Under an agreement, complete original WAV bytes are validated and passed unchanged
in one request. Mono/stereo order, sample representation, duration and byte limits
are checked. Incompatibility fails with no resampling, downmixing, clipping or
truncation. Up to 512 MiB combined container bytes are allowed, including enough
space for two 300-second 48 kHz stereo float32 WAVs. Base64/JSON adds memory and
transport overhead. Review results include transfer hashes, actual format,
per-input frames, zero gain, conversion flags and simultaneous versus independent
comparison status. Explicit agreements fix reviewer selection to the configured
endpoint/model. Existing network and model-loading permission gates still apply.

Native audio attachments use the same contract. Reference-only CLI backends keep
their existing local-analysis path. An external controller can feed a published
render to the next inference step using its manifest path/hash and the agreed input
route. File access and network delivery remain subject to that controller's
existing permission gates. `review_audio_with_contract` also accepts an agreement
explicitly through the session library.

## External multitrack drafts

Use `import_transcription` with `multitrack`, or `path` to a bounded local JSON file.
For example, the multitrack value is:

```json
{
  "schema_version": 1,
  "source": {
    "audio_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "start_seconds": 10.0,
    "end_seconds": 12.0
  },
  "recognizer": {"name": "external", "model": "my-model"},
  "tracks": [{
    "name": "Bass", "instrument": null, "program": 32,
    "role": "bass", "drums": false,
    "notes": [{"pitch": 24, "start": 10.25, "end": 11.5,
               "strength": 0.7, "confidence": 0.8}]
  }]
}
```

Pass `preview:true` first, then supply its `revision` as `expected_revision` when
accepting to reject intervening edits. `at_tick` maps `source.start_seconds` onto the destination
timeline, retaining its tempo/meter maps, overlap and velocity. `sound_id` on each
part explicitly maps a playback sound from instrument search. Unknown instrument
labels remain unknown; an omitted sound uses a reported guide. All notes are
validated before any edit, including tick-resolution collapse. Any mapping failure
rolls back the whole import. Acceptance is one Undo step. The declared draft,
confidence, model hashes, constraints and provenance persist in the project's
`transcription_provenance`, keyed by its first imported clip. They establish neither
recognizer authenticity nor musical accuracy. Unknown fields are rejected. Legacy
piano drafts remain accepted through their existing boundary.

## Transition evidence

The live, session-scoped `execution_trace` uses `operation:{"kind":"start"}` to explicitly enable collection
through the host permission gate. `read` accepts an optional `after` sequence; `stop`
disables collection. The session retains 1024 events and reports evictions and its
last sequence, so a collector can detect gaps and persist its own journal.

Events contain timestamps, origin, operation, before/after state and numeric
revision, successful/failed calls and human/frontend edit boundaries. The desktop
host supplies exact tool call IDs, results and permission outcomes. Published audio
references and byte hashes are recorded after publication. Other hosts can use
`record_execution_call` and `record_execution_outcome` for their own IDs and outcomes.
Arguments, response bodies, prompts, credentials and unrelated file contents are
excluded. The collector owns authorized exact call payloads needed for replay,
human preference attribution and dataset policy. Replay must compare current schema
hashes and stop/report incompatibility before executing recorded calls.
