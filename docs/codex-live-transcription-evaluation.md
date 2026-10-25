# Codex transcription through the new live APIs

[日本語](codex-live-transcription-evaluation.ja.md)

The 2026-10-21 application evaluation completed original-audio export, local analysis,
generic note import, note readback and rendered-audio inspection with real Codex
inference. The new APIs preserve recognition results from an existing frozen
program. They do not make an agent-generated recognizer accurate: a newly generated
DSP program produced substantially more false notes than Basic Pitch.

The later [audio-only workflow evaluation](codex-autonomous-cover-evaluation.md)
tests the repaired review-evidence path without supplying a frozen program and
also verifies section-aware composition.

The [measurement record](../tools/eval/references/codex-live-transcription-2026-10-21.json)
retains the programs, original drafts, actual imported notes, recovered reference,
scores, conversation text, acceptance observations and the artifact-recovery limits.

## What ran

The opt-in `auris-gpui` window harness opened the real application view and session,
attached a WAV through `agent_attach_files`, and submitted the panel's composer.
`drain_agent` processed the actual worker's events, including live edits, permission
checks and independent automatic reviews. The worker used signed-in Codex CLI
0.163.0, `gpt-6.1-sol`, high effort and the normal authenticated live MCP server.
There was no scripted model peer and no separate `codex exec` transcription run.

The harness uses a test window with no display, GPU or audio device. This is an
application-behavior test, not a claim of completed OS drag-and-drop or visual UI
validation. Native desktop exploration was attempted first, but the automation
tool could not drag a file between separate windows. Rendered samples were checked
numerically; neither controller nor application agent claimed listening.

Each test window used an isolated in-memory **AutoReview** policy. Persistent user
preferences were unchanged, no pending request was manually approved, and no deny
rule or bypass mode was used. Ordinary Edit mode can require manual native-command
consent; these results do not establish unattended operation with default settings.

The source was the same [Mutopia SATB *Ode to Joy* score](https://www.ibiblio.org/mutopia/cgibin/piece-info.cgi?id=528)
used in the earlier audit: 238 reference events, rendered as 38.4-second piano and
reeds/strings recordings. Filenames were anonymous. Reference notes, MIDI, baseline
predictions and correctness feedback were withheld from the application agents.
Normal application tempo/chord metadata remained available. This is one composition
in two controlled synthetic timbres, not two independent works or acoustic recordings.

## Accuracy of the actual imported scores

Matching requires exact MIDI pitch and onset within 50 ms, with maximum one-to-one
matching. F1 combines precision and recall. A separate measure also checks offsets
within the larger of 50 ms and 20% of reference duration.

| Recording | Newly generated Codex DSP | Previous frozen Codex DSP | Basic Pitch |
| --- | ---: | ---: | ---: |
| Piano, onset F1 | 16.00% | 78.12% | 95.97% |
| Reeds/strings, onset F1 | 16.77% | 59.40% | 64.74% |
| Piano, onset + offset F1 | 13.60% | 70.12% | 92.74% |
| Reeds/strings, onset + offset F1 | 12.83% | 58.86% | 45.26% |

| Recording / method | Estimated notes | Matched | Extra notes (FP) | Missing notes (FN) |
| --- | ---: | ---: | ---: | ---: |
| Piano / new Codex | 1,850 | 167 | 1,683 | 71 |
| Piano / previous Codex | 187 | 166 | 21 | 72 |
| Piano / Basic Pitch | 258 | 238 | 20 | 0 |
| Reeds/strings / new Codex | 931 | 98 | 833 | 140 |
| Reeds/strings / previous Codex | 129 | 109 | 20 | 129 |
| Reeds/strings / Basic Pitch | 142 | 123 | 19 | 115 |

The new program's dominant measured failure is excessive detection. On piano it
matches only one more reference event than the previous program while adding 1,662
more false events. It uses nonnegative fitting of analytical harmonic profiles to
interpolated spectral peaks, followed by activity segmentation. The previous
program uses a different spectral decomposition and explicit harmonic suppression.
Both are untrained DSP written by Codex, not the model directly hearing audio.

The previous program's results reproduce the earlier experiment, including its
better offset score on the mixture. Basic Pitch still wins onset F1 in both cases,
but misses 115 of 238 reference events in the mixture. These data support keeping
recognition quality evaluation separate from successful API delivery; they do not
support replacing Basic Pitch with arbitrary freshly generated analysis code.

## Acceptance and assistance

All four completed Codex runs used `export_audio_attachment` and
`import_transcription`, without `create_piano_cover`. The original byte hash and
actual program hash were checked independently against the saved draft. The actual
document notes were scored. Existing tracks and tempo were retained, changes were
unsaved before host acceptance, and one Undo restored the original project; Redo
restored the imported project. Only the host then saved the result for delivery.

The new program's SHA-256 is
`8f5478708e0538737a5b8d1d8965cd5a6b8b7efc1d79307017d40c4de67fa1ba`.
The old program remains
`1ad425dd3758b046d23c3e3a11bee0ce2815402fda64fc69f77a9cdd873c08b5`.
The old program's unchanged events received explicitly disclosed uniform strength
0.65 through a schema adapter. The new program estimates relative fitted amplitude;
neither supplies recovered original MIDI dynamics.

The host mechanically assembled the new program's final proposed append after
that native command stopped for review. It did not alter recognition logic or
retune using scores. Later requests supplied complete source as review evidence.
The four completed runs therefore demonstrate **assisted, fixed-method application
workflows**, not autonomous completion from an unqualified WAV request. Their
84–111 second times exclude earlier code development and blocked attempts.
All four frozen outputs were also reproduced exactly outside the agent after the
runs, without program edits or additional inference.

## Workflow problems found

1. **Large native commands exceed automatic-review capacity.** The first attempt
   stopped after 222.52 seconds because the serialized review request exceeded
   64 KiB. The native item alone was about 23.7 KiB; repeated command representations
   and JSON escaping also consume the review budget. No program or notes were
   imported in that attempt. Smaller, meaningful code-writing steps passed review.
2. **The reviewer lacks script and prior-call evidence.** A split-code attempt stopped
   after 383.85 seconds because the reviewer could not inspect the existing script
   before execution. Supplying the full source resolved that case. Another run
   stopped after 58.73 seconds because a separately completed hash verification was
   not available to the next review. Putting a fail-closed hash check in the same
   execution command resolved that case. Approval evidence needs a bounded way to
   carry referenced file contents and relevant verified results; raising permissions
   or automatically accepting failed reviews is not the remedy.
3. **The agent sometimes previews before import.** Two attempts called `inspect_audio`
   on the empty arrangement and received the correct range error. Inspection succeeded
   after import in all completed runs. Tool-order guidance still needs improvement.
4. **Method generation is unstable.** The previous and new programs deliver very
   different precision on identical recordings. Code and thresholds must be frozen
   before comparison, with recognition errors reported separately from API success.

Three blocked trials are retained as observations alongside the four completed
trials. Neither source/hash evidence nor retries provided musical correctness
feedback or changed the frozen recognizers.

## Artifact recovery and reproduction

During packaging, the transient `target/` directory disappeared. The generated
projects, programs, note JSONs, source WAVs and persisted conversations survived in
their separate evaluation workspace and were copied to
`.local-evaluations/codex-live-transcription-2026-10-21/`, which is ignored by Git
and survives a normal `cargo clean`. Raw volatile tool traces and initial log files
were lost. The record distinguishes previously observed harness assertions from
surviving artifacts; it does not reconstruct fictitious MCP or approval events.

The independent reference was regenerated from the public MIDI. Exact bytes of the
old annotation JSON are not claimed. Recovered reference events reproduce all three
historical fixed-Codex matching measures and every initially observed new-program
count. Actual imported projects and original draft hashes remain available. Basic
Pitch inference and delivery exports were rebuilt in the protected location.

The local `delivery/` folder there contains `Cover.auris`, `cover.wav` and
`inspection.txt` for `fresh-piano`, `fresh-mixture`, `previous-piano`,
`previous-mixture`, `basic-piano` and `basic-mixture`. All six PCM24 stereo
48 kHz exports passed encoded-file checks with zero full-scale samples. The
measurement record retains their hashes. Basic Pitch's inference-only runtime is
not comparable to an agent turn that also exports, reviews commands, imports and
inspects the result.

For a new application run, create a new evaluation config with absolute `workspace`,
`output`, `audio` and `prompt` paths, `model`, `effort`, and `timeout_seconds`.
Use output paths outside `target/`. Prepare a uv environment with numpy, scipy,
soundfile and matplotlib. Supply its interpreter explicitly in the prompt. The
test consumes signed-in Codex usage and independent review usage.

```powershell
$env:AURIS_CODEX_TRANSCRIPTION_EVAL = 'C:/absolute/evaluation/config.json'
cargo test -p auris-gpui --bins -- --ignored --exact ui::agent_chat::transcription_eval::installed_codex_transcribes_via_new_apis_in_the_window --nocapture --test-threads=1
```

With complete original harness reports, score against a matching Basic Pitch
baseline using the strict tool-result and artifact checks:

```sh
uv run --with numpy --with scipy tools/eval/live_transcription.py manifest.json basic-pitch-score.json --case ode-piano=/absolute/piano/result.json --case ode-mixture=/absolute/mixture/result.json --json comparison.json
uv run --with numpy --with scipy --with pytest pytest tools/eval/test_live_transcription.py tools/eval/test_piano_transcription.py
uv run --with ruff ruff check tools/eval/live_transcription.py tools/eval/test_live_transcription.py
```

The normal workspace tests passed 4,272 cases with 19 opt-in/resource skips; Clippy
passed. The Python measurement tests passed 17 cases, including changed audio,
programs, annotations, note counts, existing music and Undo evidence. None of these
results establishes two-hand playability or subjective cover quality.
