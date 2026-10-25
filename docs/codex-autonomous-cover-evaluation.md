# Audio-only Codex cover and section-aware composition

[日本語](codex-autonomous-cover-evaluation.ja.md)

The 2026-10-21 follow-up completed an editable cover from an attached WAV and a
short request, without supplying analysis code, an interpreter path, reference
notes or manual approvals. The ordinary request selected bundled Basic Pitch;
the independent request generated and ran its own DSP, then used
`export_audio_attachment` and `import_transcription`.

The [measurement record](../tools/eval/references/codex-autonomous-cover-2026-10-21.json)
retains prompts, observed tool/review outcomes, actual imported notes, frozen
programs, hashes, packages and exports. Full tool traces and project snapshots
remain in `.local-evaluations/codex-autonomous-2026-10-21/`.

## Review fixes and the initial blocked trial

The earlier [assisted evaluation](codex-live-transcription-evaluation.md) found
duplicate command representations exceeding the review input budget, and missing
script/prior-call evidence. The new [review evidence path](agent-review-evidence.md)
uses lossless shared text, bounded current script snapshots and recent completed
native results. Operation text is preserved; current paths/hashes are checked
again after an approving review. Ordinary permission rules remain authoritative.
Long-input regression tests exercise the exact previously failing size pattern;
the real independent run exercises script creation, execution and successive
refinement without pasting source into a user request.

An initial independent attempt stopped after 39.17 seconds. The reviewer asked
about an unnecessary enumeration of the home directory, including hidden folders,
while the agent searched for Python. That command was not executed. Native
guidance now directs runtime discovery to dedicated executable/uv queries and
project-local environments. A fresh retry with the same user prompt completed.
The blocked attempt is retained; this is not a claim that every review can or
should approve unattended. No denial was overridden and no permanent rule changed.

## Conditions

These are real application-worker tests: the GPUI window harness submits the panel,
drains actual worker events and executes live session operations through the
authenticated MCP server. Codex CLI 0.163.0 used `gpt-6.1-sol`, high effort, with
isolated in-memory AutoReview preferences. The harness has no display, GPU or audio
device; it does not establish OS drag-and-drop or visual desktop validation.

Each case started in a separate project outside the repository, without prior
conversation or analysis files. The controller supplied only the WAV and the
short request. The independent agent discovered Python through uv and installed
numpy/scipy/soundfile in its own project environment. It inspected and refined
its analysis using the source waveform, without correctness feedback. Scoring
started only after completion and preservation of its final code and draft.

The recording is the same 38.4-second synthetic piano rendition of the public
Mutopia SATB *Ode to Joy* score used in the earlier evaluation, with 238 reference
events. Its byte hash is
`049e881f2939744ad9643c69d1d20cdb790b95654ac716080678d820b2e81410`.
The MIDI, reference, title and previous estimates were withheld. Ordinary project
metadata was available. This is one controlled recording, not a cohort of real
recordings or a demonstration of reliable recognition across instruments.

## Cover results

| Request | End-to-end time | Imported notes | Matched | Extra | Missing | Onset F1 | Onset + offset F1 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Ordinary; agent selected Basic Pitch | 36.07 s | 258 | 238 | 20 | 0 | 95.97% | 92.74% |
| Independent DSP, after runtime guidance | 384.64 s | 222 | 217 | 5 | 21 | 94.35% | 76.09% |

Matching uses exact pitch, maximum one-to-one matching and a 50 ms onset
tolerance; offsets additionally allow max(50 ms, 20% of reference duration).
The scorer reads actual saved note ticks through the actual project tempo,
not the agent's completion prose or rounded tool display. Both completed runs
read notes and inspected rendered audio after import. The ordinary result
reproduces the frozen Basic Pitch baseline on all three matching measures.

The independent program uses multi-envelope harmonic spectral fitting,
nonnegative fitting, waveform onset alignment and high-octave rejection. It uses
no trained transcription model or Basic Pitch. Its provenance hash is
`29dbab4f7857dbe436f5c7d0315e6fac5e9480fe3495e23c0fb6892f9e7b9aaa`,
verified as SHA256 of the original `transcribe.py` bytes followed by `refine.py`
bytes. Both individual files and their hashes are retained. It recovers relative
spectral strengths, not original MIDI velocity, and discloses uncertain weak
notes, octaves, releases and missing pedal reconstruction.

These are end-to-end observations, including inference, reviews, environment setup,
analysis and readback. They are not benchmarked CPU runtimes. The much better
independent score than the previous newly generated program does not establish
stable method generation; the requests, assistance and generated code differ.

Both workflows performed several undoable edits, including mix adjustments.
Consequently one Undo of the entire completed workflow does not restore the initial
document. Redo restored the complete final state. The single-transaction guarantee
of each note import remains covered by session tests. Live work was unsaved until
the harness accepted and saved the final document. WAV exports below were made by
the controller afterwards; the live agent did not claim a saved WAV delivery or
subjective listening.

## New composition includes structure

The request was simply to create a bright 16-bar pop piece with melody and
accompaniment. It did not name sections. In 58.21 seconds the agent produced four
tracks, 540 notes, 128 BPM and the following real document section points:

| Section | Bars |
| --- | --- |
| Aメロ | 1–8 |
| サビ | 9–14 |
| エンディング | 15–16 |

All clips end at the exclusive start of bar 17, and the section lane becomes
unnamed there. These labels survive the saved project. Composition guidance now
requires appropriate named form as well as key/chord metadata. Live `edit_harmony`
accepts `section` alone or with harmony; bounded range edits preserve notes,
replace interior section changes, restore the previous label after the range and
undo together. Intro/bridge sections remain musical choices, not mandatory labels
for every short piece. Existing song-spec composition already saves its form.

## Remaining tasks and local outputs

At the time of this evaluation, leading/trailing padding and BPM/beat estimation
remained open. Both covers kept the project's default 120 BPM, while the source score
is 100 BPM. Source-second timing is preserved, but that does not align its music with
the bar grid or infer the source tempo. Padding removal must account for pickups, quiet attacks,
releases
and the resulting source-to-score offset. These tasks are not fixed by better
approval evidence or a successful import.

The local `delivery/ordinary`, `delivery/independent-2` and `delivery/composition`
folders each contain `Cover.auris`, `cover.wav`, `inspection.txt` and `export.json`.
All three PCM24 stereo 48 kHz WAVs passed encoded-file checks with zero full-scale
samples. This check does not prove a natural release ending, two-hand playability
or subjective musical quality.

Repeat the opt-in application tests using the configuration described in the
[previous evaluation](codex-live-transcription-evaluation.md#artifact-recovery-and-reproduction).
For natural workflows, set `natural_workflow:true`; this records the full sequence's
single-Undo observation without treating several legitimate edits as one import.
Set `bundled_allowed:true` for the ordinary request. The named-section test is
`ui::agent_chat::transcription_eval::installed_codex_composes_named_sections_in_the_window`;
omit `audio` from its configuration. Real runs consume signed-in model and review usage.

Validation: `cargo test --workspace` passed 4,279 tests with 20 opt-in/resource
skips; `cargo clippy --workspace --all-targets` and formatting passed. The final
review-specific run passed eight tests, desktop permission controls passed 13,
and the shared Python measurement tests passed 17. The three successful real
model trials are additional opt-in runs; the initial blocked trial remains a
recorded failure.
