# Agent production audit: composition and piano covers

Follow-up: live harmony editing and the complete voice-selection/lyrics/synthesis workflow
are now implemented; see [Live vocal production](agent-workflows.md#live-vocal-production).
The findings below record revision `a5564e38`; the other issues remain separate work items.

On 2026-10-21, the public tools could complete an original instrumental piece and a
MIDI-based piano cover, but the live agent could neither write the document's chord progression
nor complete singer selection, lyric entry and voice synthesis.
Audio transcription produced useful piano drafts and substantially less complete drafts
from mixed instruments. Export and note readback also exposed reproducible defects.

This is a workflow evaluation of revision `a5564e38`, not an implementation of the fixes.
The controller was Codex making real saved-project MCP calls. No additional language model
was evaluated for its tendency to choose particular tools. The live cover's inference and
acceptance were exercised through the same session backend as `create_piano_cover`, using
the evaluation example below. That experiment bypasses attachment selection and permission
UI; existing agent host tests cover those boundaries separately. No subjective listening
or aesthetic-quality claim is made. The audio critic services were unavailable.

## Completed experiments

| Experiment | Actual result | Independent checks |
| --- | --- | --- |
| Authored original | D major, 96 BPM, 16 musical bars, two piano parts, 61 melody and 259 accompaniment notes | Harmony written using MCP `edit_harmony`; saved projects reopened; WAV/MIDI exported |
| Generated original | `check_spec` and `compose`, seed 20261021, 16-bar verse plus an ending bar, 206 notes | `describe` verified generated clips, instruments, routing, and ending |
| MIDI piano cover | Beethoven's *Ode to Joy*, 16 bars, 100 BPM, 238 notes on two hand tracks | Independent MIDI decoding compared source and export: pitches and onset/offset times preserved within 2 ms |
| Piano audio to piano | The 38.4-second cover rendered as piano, then transcribed with bundled Basic Pitch | 273 estimated notes against the 238-note source; existing music/tempo retained and one-step Undo verified |
| Mixed audio to piano | The same independent score rendered with Physical Reeds and Physical Strings, then transcribed | 154 estimated notes; identical reference and matching criteria as the piano control |

The reference is Mutopia's SATB arrangement, not a commercial recording:
[piece and provenance](https://www.ibiblio.org/mutopia/cgibin/piece-info.cgi?id=528),
[MIDI](https://www.ibiblio.org/mutopia/ftp/BeethovenLv/ode/ode.mid), and
[LilyPond score](https://www.ibiblio.org/mutopia/ftp/BeethovenLv/ode/ode.ly).
This controlled synthetic test has known ground truth but does not establish performance on
real recordings, vocals, percussion, effects, or dense commercial mixes. The MIDI cover is
a piano realization of that arrangement, not a newly simplified two-hand arrangement.

The manual workflow successfully used sound search, atomic track setup, named clips,
file-backed note replacement, harmony, faders/pan, measurement, normalization, rendering,
MIDI export, and encoded-file verification. Retrying the 259-note replacement returned
`Unchanged` without another checkpoint. A note outside the clip was rejected and the saved
document's SHA-256 stayed unchanged.

Final original and MIDI cover exports both met -23 LUFS, a -1 dBTP ceiling, zero full-scale
samples, and a final-half-second RMS below -60 dBFS after the tail workaround described below.
The original is 45.0 seconds, true peak -9.1 dBTP; the cover is 43.2 seconds, -9.4 dBTP.
The audio-derived drafts were also exported, with their transcription errors retained.

## High priority: live composition cannot write harmony metadata

The user's observation that the agent does not write chord progressions is consistent with
an actual missing live operation. Saved-project MCP offers `edit_harmony`; it successfully
wrote the original's D-major key and 16-bar progression. The live `Command` enum has no
equivalent. `live_agent::definitions()` excludes the advanced whole-project `Compose`
operation, and `live_agent::command("edit_harmony", ...)` resolves to `None`.

The live preamble says to choose harmony and write music with `add_notes`. That can produce
audible chords without changing `project.harmony`, leaving its initial C-major key and
empty chord map. A model cannot correct this through better tool selection alone. This
explains the live path; whether an external model ignores the available saved-file MCP
operation remains a separate model-behavior experiment.

Expose a bounded, undoable live harmony operation and verify both the notes and the saved
key/chord map in composition acceptance checks. Do not regenerate existing notes implicitly.
Relevant code: [live commands](../crates/auris-session/src/live_agent.rs),
[live tool translation](../crates/auris-toolbox/src/live_agent.rs), and
[live instructions](../crates/auris-agent/src/lib.rs).

## High priority: mixed audio transcription is incomplete

Pitch/onset matches use maximum one-to-one bipartite matching, exact MIDI pitch, and the
stated onset tolerance. An estimated event cannot satisfy several reference notes.
These are onset metrics; they do not certify accurate releases, dynamics, or playability.

| Input | Estimates | Matches at 50 ms | Precision | Recall | F1 | F1 at 100 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Piano | 273 | 238 | 87.2% | 100.0% | 93.2% | 93.2% |
| Reeds + strings | 154 | 94 | 61.0% | 39.5% | 48.0% | 67.3% |

The piano control added 35 unmatched events. At the more forgiving 100 ms tolerance, the
mixed source matched 132/238 notes, still missing 106 reference events and containing 22
unmatched estimates. Median duration error among these mixed-source matches was 68.7 ms;
matching on duration was not required. Measured inference times were about 0.39 and 0.37
seconds respectively on this machine, excluding build and project application.

The implementation applies all pitched estimates to one piano track. It does not identify
the main melody, separate instruments/drums, remove duplicates, constrain hand spans, or
choose which accompaniment voices to retain. A technically successful transcription is
therefore not a finished piano arrangement. Retain the explicit draft wording, add quality
evaluation on independent recordings, and provide an actionable review/edit workflow before
presenting this as faithful cover production.

## High priority: live singing stops after creating the track

The user additionally reported that the live agent can create a singer track but cannot
select a singer, enter lyrics, or synthesize the voice. The exported command contract
confirms this gap: `add_track` accepts `kind:"singer"`, and note clips/notes can be added,
but there is no live voice-selection, lyric-writing, or singing operation.
`set_instrument` only changes instrument/drum sounds and is not singer selection.

Saved-project MCP has `write_lyrics` and `sing`, but these are not live editing operations.
The live agent deliberately receives only read-only saved-file references alongside live
tools, so their existence elsewhere does not make a live vocal workflow possible. The
current live surface can leave the singer playing its temporary guide voice instead of
producing the requested sung take.

Add the complete workflow to the repair backlog: discover installed voices and speakers,
select and read back the chosen voice, distribute/edit lyrics and phonemes, synthesize on
a cancellable worker, and report missing models, failed synthesis and stale takes. Apply
the resulting take through the live permission/revision boundary and make the document
edits undoable. An acceptance test must reach an actual vocal take, rather than treating
successful singer-track creation as successful song production. No vocal synthesis was
attempted in this audit; the blocker is the missing public live contract.

## High priority: the 25 MiB attachment gate blocks normal song-length WAVs

Follow-up: live audio references now use streamed original snapshots with a 512 MiB
per-file and 1 GiB per-turn/cache budget. Native API uploads have a separate 128 MiB
prepared-payload budget, with full-length 16 kHz PCM16 conversion above 25 MiB.
CLI local audio no longer consumes the image/text upload budget. The observations
below describe the audited build; see [agent backends](agent-backends.md#reference-attachments)
for the current limits and conversion policy.

The user reported WAV rejection at 25 MiB. Both the per-file and per-turn raw-byte ceilings
are `25 * 1024 * 1024`; the conversation audio-reference cache shares the total ceiling.
The file-size check precedes decoding and local transcription. The CLI reference path also
uses the bounded reader, even when it supplies local measurements rather than uploading the
audio to a provider. These are Auris limits, not a measurement of any provider's limit.

| Uncompressed stereo WAV | Approximate maximum duration at 25 MiB |
| --- | ---: |
| 44.1 kHz, 16 bit | 148.6 s |
| 48 kHz, 16 bit | 136.5 s |
| 48 kHz, 24 bit | 91.0 s |
| 48 kHz, 32 bit float | 68.3 s |

These estimates exclude the small WAV header. Two individually acceptable attachments can
also exceed the aggregate limit. A ten-minute 48 kHz/24-bit/stereo WAV is about 164.8 MiB,
although the local Basic Pitch backend supports ten minutes. Thus the public attachment
path cannot reach the backend's documented duration for this ordinary input format.

Separate local audio references from provider-upload payloads. Bound decoded duration,
channels and working memory, prepare an appropriate local snapshot off the UI thread, and
apply provider-specific upload constraints only where an upload occurs. Merely raising the
shared raw-byte constant would retain the mismatch and amplify base64/history costs.
Relevant code: [attachment ceilings and reader](../crates/auris-agent/src/lib.rs) and
[reference preparation and retention](../crates/auris-agent/src/attachments.rs).

## High priority: instrument decay is cut at the arrangement boundary

With no effects, whole-song `render` defaults to `include_tail:true`, but both piano files
ended exactly at the clip boundary. The original ended at 40.0 s with final-half-second RMS
-53.0 dBFS; the MIDI cover ended at 38.4 s with -38.7 dBFS. Both failed an explicit
`verify_render` ending requirement of -60 dBFS, despite the render's generic `PASS` message.
That generic result did not include the stricter ending requirement; it was not a claim
that the requirement passed.

The root cause is specific: `OfflineRender` appends `graph.tail_frames()`, whose strip
calculation sums enabled effects' tails without an instrument-release contribution.
Changing the final clip end to bar 19, without changing its notes or faders, added two empty
bars. Both resulting files then passed the same loudness, peak, and ending checks. The audio
drafts needed the same padding policy.

Account for source decay at export, or offer an explicit post-roll duration independent of
musical clip length. Keep `verify_render` as a separate acceptance check. Relevant code:
[offline renderer](../crates/auris-engine/src/offline.rs),
[graph tail](../crates/auris-engine/src/graph/mod.rs), and
[effect-only strip tail](../crates/auris-engine/src/graph/strip.rs).

## Medium priority: MCP note readback loses editing precision

An actual `replace_notes` request stored a C4 at beat `1 + 1/960`, duration `1/960`, velocity
`0.751`, and an E4 at beat `1 + 3/960`, duration `1/3`, velocity `0.754`.
`notes` returned:

```text
[1] bar 1 beat 1 — C4, 0 beats, vel 0.75
[2] bar 1 beat 1 — E4, 0.33 beats, vel 0.75
```

The underlying MIDI export retained the nonzero timings. The loss occurs in
[`notes` and `trimmed`](../crates/auris-toolbox/src/lib.rs), which convert to `f32` and format
two decimal places. Reusing the displayed duration can make a valid note invalid; reusing
other displayed values changes timing and dynamics. This also affects longer, ordinary
triplet and rubato notes. Live `read_notes` already returns integer ticks and avoids this
particular defect. Provide machine-readable tick positions/durations and unrounded velocity
alongside human-readable labels in MCP.

## Other workflow gaps

* **Different cover tools on different transports.** Live `create_piano_cover` uses bundled
  polyphonic Basic Pitch. Saved MCP has no such callable: an actual request returned
  `tool not found`. Its `transcribe_audio` is explicitly monophonic YIN; it returned 71
  events for the polyphonic 238-note piano control, which is outside its advertised scope.
  The optional mixture tool needs a separately prepared model and explicit noncommercial
  acknowledgement. Expose the existing bundled cover backend to saved-project agents.
* **Piano performance controls are incomplete.** Imported MIDI controllers can survive
  interchange and the physical piano supports sustain, but the exposed catalogs contain
  no note-clip CC64/pedal editor. Parameter automation is not MIDI-controller editing.
  `discover_tools` found no pedal operation. Pedal, expressive timing and voicing review
  should be part of a cover workflow, rather than requiring external MIDI reconstruction.
* **A builtin-only search still pays for broad discovery.** One fresh-process search with
  `source:"builtin"` took 24.44 s and reported 62 scan errors from the machine's broader
  library. Later scans differed with the process environment. The catalog is populated
  before `SoundFilter` is applied. This timing is environment-specific; the result filter
  does not currently isolate discovery cost. Consider source-scoped discovery.
* **Schema bounds sometimes contradict field descriptions.** For example, `preview` bar
  fields and `edit_clip.clip` advertise minimum 0 while describing 1-based values. An actual
  `preview` with `start_bar:0` was refused correctly at runtime. Align the schemas so an
  agent can avoid the failed call.

The default MCP `tools/list` contained 66 tools and 74,825 UTF-8 bytes of compact JSON in
this run. Existing tool groups and on-demand help are useful context controls. This audit
does not establish a model-context failure from catalog size.

## Reproduction and evidence

Run the surface example to inspect the exported definitions, without a model or UI:

```powershell
cargo run -p auris-toolbox --example evaluate_agent_surface
```

To repeat the cover inference and live session acceptance/Undo checks on a known recording:

```powershell
cargo run -p auris-session --example evaluate_agent_cover -- source.wav NewCover.auris
```

The helper creates a sentinel track, applies the exact draft at beat 4 with a 100 BPM tempo,
checks preservation and full one-step Undo, reapplies it, and saves a new project. It prints
the complete source-second draft and measured inference time. For a scored comparison, use
the original MIDI tempo map and match exact pitches with 50/100 ms onset tolerances as above.
The 100 BPM helper clock matches the reference; it is not an inferred tempo.

The checked-in [measurement snapshot](../tools/eval/references/agent-production-audit-2026-10-21.json)
contains hashes and detailed metrics. Local artifacts are in `target/agent-production-audit/`:
`wire.jsonl` and `probe-wire.jsonl` hold requests/results; `catalog.json`,
`original-melody.json`, `original-accompaniment.json`, `generated.asong`, `metrics.json`,
and `basic-pitch-{piano,mixed}.json` hold inputs and measurements. The `Original`, `OdeCover`,
`AudioCover`, and `MixedAudioCover` folders contain inspectable saved documents. Listen to
`original-finished.wav`, `ode-cover-finished.wav`, `audio-piano-draft.wav`, and
`audio-mixed-draft.wav` to judge the results independently. Generated media are not committed.

Validation completed with `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets`, and `cargo test --workspace`
(4,245 passed, zero failed, 18 ignored). Both evaluation examples were also run successfully.
The workspace run included the real attachment-limit checks, retained-audio/permission host
test, and desktop tests; it did not involve a real model deciding which tools to call.
