# Agent tool improvements and Codex rerun

Date: 2026-10-22. Implementation: `8f8457e8`. This implements the four changes
identified by the [composition workflow audit](codex-composition-audit.md).
The [measurement record](../tools/eval/references/agent-tool-improvements-validation-2026-10-22.json)
contains per-turn calls, errors, encoded-file measurements and state comparisons.

## Implemented contracts

`inspect_performance` and `set_performance` now return `note_effects`, computed
with the actual playback performer. Each stage reports changed onsets, durations,
velocities and pitches, additions/removals, simultaneous chord groups and warnings.
The measurements cover the selected scope's first loop pass, with explicit limits
and truncation. Pitch curves, timbre and volume contours are outside note-event
measurements. Nested drum scopes require selecting their writer. A separate safety
test verifies that sparse long clips cannot trigger unbounded ghost-note grid scans.

The live host groups one user request's edits in the existing Session Undo stack.
Commands and worker publication are tagged only while they execute; playback is
immediate and no human gesture stays open while the model thinks. One live-only
`history` tool reads bounded entries or reverses the latest uninterrupted group,
using a numeric revision guard. Human edits and open gestures are boundaries.
History uses the ordinary Undo/Redo machinery and adds no persistent snapshot store.
The saved-project adapter retains its separate checkpoint workflow.

`edit_notes` uses one typed input:

```json
{
  "expected_revision": "<content revision>",
  "input": {
    "kind": "inline",
    "edits": [{
      "kind": "patch", "clip": 2,
      "add": [{"pitch": 60, "start_tick": 0, "length_ticks": 960, "velocity": 0.75}]
    }]
  }
}
```

File input is `input:{"kind":"file","source_path":"/absolute/notes.json"}`.
The tagged enum makes the alternatives exclusive without custom schema rewriting.
Every branch retains its full object and item types. Velocity remains strictly
normalized `0..1`; invalid values are refused rather than guessed or converted.

`read_notes` accepts `clips:[{"clip":2},{"clip":3,"offset":128}]`: up to eight
distinct clip pages with one shared revision, up to 128 notes each, original storage
indices and per-clip `next_offset`/`truncated`. It replaces repeated single-clip
calls without adding another inspector. Current examples and limits are in
[agent workflows](agent-workflows.md).

The base live catalog has 49 tools; saved-project MCP remains at 69 with all groups.
The optional internet-search tool still depends on configuration. Claude, Codex and
the rig worker share these contracts; this rerun specifically exercised Codex.

## Same natural-language scenarios

The exact [four scenarios and 13 prompts](../tools/eval/references/codex-composition-scenarios.json)
were rerun with the built-in `gpt-6.1-sol` Codex backend, fresh unsaved fixtures and
the real GPUI host. Prompts contain no tool names or document IDs. The configured
`gemma4:e2b` critic received actual rendered audio; its observations remain fallible.

| Scenario | Tool calls before → after | Note reads before → after | Independent checks |
| --- | ---: | ---: | --- |
| Eight-bar title-screen BGM | 52 → 45 | 10 → 3 | Four populated parts, 100 BPM, eight-bar loop; 19.2 s WAV and 193 MIDI notes |
| Existing 6/8 melody and folk accompaniment | 99 → 70 | 13 → 7 | Original clip including CC1 identical; guitar authored notes unchanged; effective humanization and explicit stroke limitation; 16.0 s WAV |
| Four-bar sketch, 16-bar form, chorus drums and reversal | 161 → 114 | 64 → 16 | Original clips retained; only chorus drum clips edited; Undo restored the entire pre-edit project; 34.285708 s WAV |
| Generated jazz phrases and a different lead | 60 → 76 | 12 → 9 | Lead changed from 22 to 16 notes with longer mean duration; recipe retained; accompaniment and other project fields identical; 20.0 s WAV |

All delivery files were independently inspected: 48 kHz, 24-bit WAV, non-silent,
no full-scale samples and requested duration within one sample. MIDI files were
reimported into fresh sessions. Export requests left the open document unchanged
and unsaved. Full transcripts, per-turn project snapshots and WAV/MIDI artifacts
are under `target/eval/agent-improvements-final-2026-10-22/`.

In the folk follow-up, Codex correctly reported that the single-note arpeggio had
no effective stroke spread, while applying and measuring subtle expression. It
preserved all authored guitar and melody data. This addresses the earlier false
claim that storing a strum setting necessarily produced a stroke.

The drum reversal used `history` read and undo, bracketed by two project reads:
four calls instead of seven, with no note array reconstruction. The complete
result matched the snapshot before the drum request.

Total calls fell from 372 to 305; note reads fell from 99 to 35. Successful note-read
payloads totaled 340,103 → 336,921 UTF-8 bytes. The largest individual read increased
to 38,981 bytes because it contained multiple clips. This is fewer round trips,
not a claim of proportionally smaller context use. Songs differed between runs;
concurrent model/build load also makes these wall times unsuitable as a speed benchmark.

There were **zero note-input shape or velocity errors** in the final rerun.
Four other refusals were recovered: two incorrect clip IDs and two unsupported
recipe values (`quarter` subdivision and `h h` rhythm). These did not corrupt the
document. The recipe case took more calls than before; the result does not establish
that every composition request is faster or error-free.

An intermediate validation exposed ambiguous partial root input-alternative schemas:
all three controllers initially encoded edit objects as JSON strings. The final
tagged input uses the same explicit discriminator pattern as other tools and
removed that failure in all four final scenarios. Intermediate evidence remains
under `target/eval/agent-improvements-2026-10-22/`.

## Validation and limits

The non-UI workspace run passed 3,216 tests. The complete GPUI suite passed 1,123
tests in serial mode. The final agent UI subset passed 103 tests, and ten targeted
performance tests passed, including the additional long-grid safety regression.
Formatting, workspace/all-target Clippy and desktop/MCP builds passed.

The first parallel workspace attempt failed one existing appearance-save UI test
when its saved custom-theme list was empty. It passed alone, and the complete GPUI
suite subsequently passed serially. No appearance implementation was changed.
The logs and conditions are recorded rather than treating that first run as a pass.

The long-grid inspection cap was checked separately after the model runs; every
scenario clip is at most 64 quarter notes, so that cap does not change these workflows.
Model tests used Bypass mode on generated fixtures, not interactive approval clicks.
Permission and history tests cover Read-only refusal, stale revisions, human-edit
boundaries, open gestures, empty requests and exact Undo/Redo restoration. The model
trials did not edit existing user projects or saved agent configuration. These are one-run integration
checks, not a statistical model-reliability or musical-quality evaluation.
