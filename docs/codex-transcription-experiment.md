# Codex-led transcription experiment

[日本語](codex-transcription-experiment.ja.md)

The 2026-10-21 experiment used the installed Codex CLI 0.163.0 with the user's
configured `gpt-6.1-sol`, high reasoning effort, and ChatGPT sign-in. Codex produced
usable note drafts by writing and running its own signal-analysis program without
Basic Pitch. Its aggregate accuracy remained below the current recognizer.

The [measurement record](../tools/eval/references/codex-transcription-2026-10-21.json)
retains prompts, model metadata, command traces, original notes, the frozen generated
program, audio/reference hashes, replay checks, metrics and delivery acceptance.
This adds development tooling and evidence; production transcription is unchanged.

## Direct audio versus a tool-using agent

The installed app-server schema includes `audio` and `localAudio` input types.
We sent actual `localAudio` inputs to three fresh, tool-free Codex threads using
silence, six isolated notes and two three-note chords. All three replies returned
empty arrays with an explicit statement that the attached audio was inaccessible.
The model catalog advertised text and image input for the selected model, and no
audio modality for any of its seven entries. These are input-capability failures,
not measurements of musical recognition accuracy. The
[official app-server documentation](https://learn.chatgpt.com/docs/app-server)
explains the model catalog's `inputModalities` field.

For the second route, Codex received a separate workspace containing only 15 WAV
snapshots, anonymous filenames and durations: the previous experiment's same 12
composer recordings plus the three controls. Scores, identities, BPM, chords,
MIDI and Basic Pitch estimates were withheld. Codex selected its own approach,
wrote Python, inspected sample measurements and plots, and refined its code.
The provided uv environment contained numpy, scipy, soundfile and matplotlib.
Pretrained transcription models, external recognition services, internet research
and delegation were excluded. No correctness feedback was returned to Codex.

The first CLI attempt could not execute native commands under the never-approve
configuration and processed no files. The measured attempt used the documented
`--approve-for-me` workspace mode. User config, hooks, plugins, apps and inherited
MCP connections were excluded; the configured model/effort were passed explicitly.
Its command trace contains no original case identities or annotation accesses.
These are experiment controls, not an adversarial filesystem-isolation guarantee.

## What Codex built

The completed agent took 535.75 seconds and wrote 846 note events across all 15
files. It explicitly reported that it analyzed samples and plots rather than
hearing playback. Its final program uses:

* Stereo averaging, resampling to 22,050 Hz and a centered Fourier analysis with a
  4,096-sample Hann window, 220-sample hop and 8,192-point FFT.
* Broadband-floor removal and frequency whitening.
* Eight analytical harmonic profiles per piano key, covering MIDI 21 through 108,
  followed by sparse nonnegative spectral fitting and fundamental-peak support.
* Activity thresholds, repeated-attack detection, boundary refinement and a
  heuristic that suppresses synchronous upper harmonics.

These profiles are mathematical templates, not downloaded or pretrained weights.
The complete program is stored in the record. Its final SHA-256 is
`1ad425dd3758b046d23c3e3a11bee0ce2815402fda64fc69f77a9cdd873c08b5`.
All 15 original drafts were reproduced exactly by the frozen program before
publishing results. No model or program edits followed reference scoring.

## Accuracy on the same inputs

The corpus consists of the same 12 held-out synthetic recordings from the
[previous evaluation](../tools/eval/references/piano-transcription-2026-10-21.json): three presets, two seeds,
and piano/mixture versions. Coincident unisons merge into 924 reference events.
The independent scorer verifies the frozen project annotations and recording hashes.
Matching requires exact MIDI pitch and onset within 50 ms, with maximum one-to-one
matching. F1 combines event precision and recall; it does not measure arranging quality.

| Input cohort | Codex-generated method | Current Basic Pitch |
| --- | ---: | ---: |
| All 12 recordings, onset F1 | 54.95% | 67.48% |
| Six piano recordings | 60.37% | 71.19% |
| Six mixtures | 49.11% | 62.71% |
| Matched events | 483 | 637 |
| Extra events (FP) | 351 | 327 |
| Missing events (FN) | 441 | 287 |

The isolated-note and chord controls match all 12 events, including release checks;
the silent control correctly produces zero events. An all-empty case's numerical
F1 is undefined by intuition and stored as zero by the scorer; silence acceptance
is assessed separately, not described as a failed transcription.

Results vary: the Codex method improves the two city-pop piano cases by 5.4 and 1.8
percentage points, but loses on the other ten recordings. At 100 ms tolerance its
aggregate onset F1 is 58.36%, and its onset-plus-offset F1 is 42.32%. This is an agent
with coding tools, not an otherwise controlled model-quality comparison with the
earlier one-shot Gemma audio requests.

After freezing the method, the host applied it unchanged to the independent Mutopia
SATB *Ode to Joy* recordings. No additional Codex inference or tuning was involved.
These are controlled existing-score syntheses, not acoustic performances.

| Existing-score input | Codex onset F1 | Basic Pitch onset F1 | Codex FP / FN |
| --- | ---: | ---: | ---: |
| Piano | 78.12% | 95.97% | 21 / 72 |
| Reeds and strings | 59.40% | 64.74% | 20 / 129 |

For the reeds/strings source, onset-plus-offset F1 improves from 45.26% to 58.86%,
although onset recall falls. The full record retains that improvement and the
missed notes. Harmonic suppression can remove real octave/harmonic voicings;
quiet notes, attacks during sustain and broadband interference remain difficult.
Neither method was scored for two-hand playability or subjective listening quality.

## Application boundary and an editable cover

Two integration limits matter if this route is adopted:

1. The current Auris Codex attachment route supplies bounded tempo/chord measurements
   and an opaque attachment ID. It does not expose the original samples as a general
   analysis input. This prototype instead received WAV snapshots in its own workspace.
2. `PianoTranscription::validate` requires the exact bundled Basic Pitch model hash.
   The normal piano-cover session command therefore rejects this different recognizer's
   draft. We recorded the rejection without substituting a false model hash.

For delivery validation, the host converted the frozen reeds/strings draft to MIDI
and called the real saved-project `import_midi` and `render` tools. This was a host
acceptance check, not a Codex model tool call. All 129 notes survived import into a
physical-piano project; the largest timing change was 0.260 ms from MIDI rounding.
Velocity was uniformly 83 for audition, not inferred dynamics. The 38.39-second WAV
passed encoded-file checks with no full-scale samples. The record retains its hash.

The experiment supports tool-assisted Codex transcription as a working prototype,
but not replacing the current recognizer with this algorithm across all recordings.
An agent still needs a reliable audio-analysis implementation and suitable access
to the source, along with a backend-independent note-draft import boundary.

The API limitations above describe the experiment revision. The subsequent
[transcription API](transcription-api.md) provides original-attachment export and
recognizer-independent draft import; it does not change these recognition scores.

## Reproduce

Use a uv environment with numpy/scipy for the harness and scorer. Sign in through
the official Codex CLI; the harness never reads subscription credential files.
It uses the configured model and reasoning preference. New output paths are required.

```powershell
$codexExe = (Get-ChildItem "$env:APPDATA/npm/node_modules/@openai/codex" -Recurse -Filter codex.exe | Select-Object -First 1 -ExpandProperty FullName)
uv run --with numpy --with scipy tools/eval/codex_transcription.py target/agent-direct-transcription-controls/manifest.json target/codex-direct-new --codex $codexExe --split control
uv run --with numpy --with scipy tools/eval/codex_transcription.py target/piano-transcription/corpus-v1/manifest.json target/codex-tools-new --codex $codexExe --mode tools --controls target/agent-direct-transcription-controls/manifest.json
uv run --with numpy --with scipy --with pytest pytest tools/eval/test_codex_transcription.py tools/eval/test_direct_audio_transcription.py tools/eval/test_piano_transcription.py
uv run --with ruff ruff check tools/eval/codex_transcription.py tools/eval/test_codex_transcription.py
```

The tool-using mode prepares a separate uv environment and may install the listed
scientific packages. It consumes the signed-in Codex account's usage. A new agent
run can produce a different program; use the recorded frozen source and outputs
to audit this historical result. Replay timings include diagnostic plot generation
and are not comparable to a pure inference benchmark.
