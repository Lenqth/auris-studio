# Agent argument errors: cause and validation

[日本語](agent-argument-errors.ja.md)

Date: 2026-10-22.
Implementation: `f233bdc5`.

The preceding composition rerun completed all four
scenarios but recovered from four argument refusals. This follow-up addresses
those refusals without changing the composition algorithms or tool count.

## Causes and changes

Two `read_notes` calls copied track addresses `1,2,3,4` instead of the actual
clip addresses `5,6,7,8`. Creation results already distinguished `track` and `clip`,
but `inspect_project` labeled both nested values `id`. The shared instructions
and tool description also demonstrated clip numbers `1,2` without establishing
their origin. One failing call immediately followed an inspection containing the
correct nested values: having the information available did not prevent the mistake.

Model-facing project inspections now name numeric addresses `track` and `clip`,
matching creation results and tool arguments. The composition inspector uses the
same vocabulary. Primary instructions identify `tracks[].clips[].clip` as the
source for note operations and remove fixed-number examples from the preamble.
An invalid note-read address identifies a mistaken track ID and returns at most
eight valid track/clip pairs. It never guesses a clip from a track or changes the
document. Persisted project IDs and the file format are unchanged.

The other calls supplied subdivision `quarter` and rhythm `h h` to `edit_recipe`.
Both fields were unbounded strings in the transmitted schema. Their restrictions
existed only in field descriptions, which compact schemas intentionally remove.
The controller therefore had no machine-readable choice list or rhythm alphabet.
These are grid controls: a subdivision selects step spacing, and a rhythm is a
hit/rest pattern; neither accepts arbitrary held-note duration names.

Recipe subdivisions now deserialize into the existing core enum, with its four
canonical names retained in compact JSON Schema. Rhythm exposes its actual parser
alphabet and length bound in that schema. The primary tool description explains
the step syntax and points to density/gate for note count and held length; this
guidance survives description compaction. Runtime errors explain the rhythm
notation and include a valid example. No extra tool or fallback alias was added.

Regression tests cover missing wire constraints, canonical enum round trips,
valid step patterns, clearing a pattern, invalid rhythm refusal without partial
edits, unambiguous inspected addresses and actionable wrong-track errors without
document mutation. The wire-constraint regression failed on the original schema
because the subdivision enum was absent.

## Same-scenario rerun

The unchanged four-scenario, thirteen-prompt
[manifest](../tools/eval/references/codex-composition-scenarios.json) was executed
through the built-in `gpt-6.1-sol` Codex backend and real GPUI host on fresh unsaved
fixtures, using Bypass permissions and the configured `gemma4:e2b` audio critic.
The [measurement record](../tools/eval/references/agent-argument-errors-validation-2026-10-22.json)
contains per-turn counts, objective checks, file measurements and the remaining
audio-delivery failure. Transcripts, project snapshots and WAV/MIDI files remain
under `target/eval/argument-errors-2026-10-22/`.

| Scenario | Previous argument errors → current | Calls / note reads | Independent result |
| --- | ---: | ---: | --- |
| Eight-bar BGM | 1 → 0 | 47 / 2 | Four populated parts and eight-bar loop; 19.2 s WAV, 200 MIDI notes |
| Existing 6/8 melody and accompaniment | 0 → 0 | 50 / 6 | Original clip including CC1 exact; guitar authored data unchanged; effective expression and explicit strum limitation; 16 s WAV |
| Sketch, 16-bar form, chorus drums and Undo | 1 → 0 | 116 / 13 | Original clips exact; only chorus drums changed; Undo restored the entire document; export detected clipping |
| Generated phrases and replacement lead | 2 → 0 | 52 / 3 | Lead 38 → 30 notes, mean duration 644.21 → 960 ticks; recipe retained; accompaniment exact; 20 s WAV |

All thirteen turns completed with **zero argument/tool errors**, compared with
four in the preceding run. Total calls were 305 → 265; note reads 35 → 24.
The generated music differs between runs, so these counts are descriptive rather
than a speed or reliability benchmark. The regenerate follow-up used density,
gate and a seed directly, without unsupported duration names. During initial
composition, it also successfully used `eighth` and `x.x.x.x.` recipe controls.
The arrangement's first note batch used the actual interleaved clip addresses
`2,4,6,8`, distinct from their tracks `1,3,5,7`.

All four WAV files were 48 kHz/24-bit with requested duration within one sample.
All MIDI files reimported with notes. Export turns left each document unchanged
and unsaved. The guitar's single-note arpeggio had no eligible chord groups;
Codex correctly reported that the stored strum spread did not create strokes.
The Undo turn used project inspection, history read, guarded history undo and
project inspection, without reconstructing note arrays.

### Remaining audio-delivery failure

The 16-bar arrangement WAV reached 0 dBTP and contained two full-scale samples.
`verify_render` returned `verdict:FAIL`, `peak_ok:false`; Codex explicitly reported
the failed clipping check and the need to render again. It did not claim a clean
delivery. The file is 34.285708 seconds with 587 MIDI notes, and the score/Undo
checks passed, but this audio-delivery criterion failed. The other three WAVs
were non-silent and had no full-scale samples. This result is retained rather
than rerunning a different random composition until it passes.

This is a separate observed mixing/export outcome, not a recurrence of any of the
four argument mistakes. It remains unresolved by this argument-contract change.
No musical-quality or universal model-reliability claim follows from this trial.

The WAV clipping was subsequently reproduced from the retained project and fixed
in the output-headroom follow-up. Its separate same-prompt
Codex rerun passed all four encoded-WAV delivery checks.

## Code validation

- `cargo test --workspace --exclude auris-gpui`: 3,220 passed, 18 ignored.
- GPUI `ui::agent_chat` tests: 103 passed, six opt-in trials ignored.
- Four real Codex scenario processes: exit zero; semantic checks remain separate
  from transport completion, including the clipping failure above.
- `cargo fmt --all --check`, `git diff --check`, workspace/all-target Clippy and
  desktop/MCP builds passed.

The complete GPUI suite was not rerun for these model-contract changes; its agent
subset exercised the affected host path. Code tests and real trials used the same
implementation. Existing user projects and saved agent preferences were not edited.
