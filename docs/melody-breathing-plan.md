# Melody breathing

The arrangement harness measures phrase space and prepares explicit foreground note
repairs. Enable it with `AURIS_ARRANGEMENT_HARNESS=1`. Shared composition guidance
also asks agents to plan calls, continuations, answers and breathing points when writing
new music, through either manual note tools or generated composition.

The original report concerned an agent-composed lead playing continuously. Its project
has not been reproduced. The generated writer already closes phrases; it was not changed.
Tests distinguish directly authored continuous notes, detached notes without a phrase
rest, existing generated phrase endings, and notated gaps with sustain.

## Diagnose before editing

Read `inspect_project` and `read_notes` for stable IDs, original note indices and revision.
Confirm the foreground role rather than guessing from a track name:

```json
{
  "operation": {
    "kind": "inspect_breathing",
    "track": 2,
    "role": "lead",
    "start_tick": 0,
    "end_tick": 30720,
    "min_rest_ticks": 480,
    "phrase_ends": [15360, 30720]
  }
}
```

The result merges overlapping intervals before measuring authored and performed note
occupancy, exact continuous runs and `longest_without_phrase_rest`. The latter bridges
articulation gaps shorter than `min_rest_ticks`; uniformly detaching notes need not
create a phrase breath. Durations are ticks, quarter-note units and seconds through the
full tempo map. Phrase endings report their preceding space. Gaps at range boundaries
are included, and all positions are absolute ticks.

`role:"texture"` keeps the same measurements without lead-specific suggestions. Choose
the threshold and phrase endings for meter, tempo, style, section and vocal text.
There is no universal silence quota or compulsory rest every four bars. Missing space
at an explicit lead ending is a suggestion, not an artistic failure.

Clip joins and all bounded seeded loop passes use the playback performer. Muted clips
and muted tracks contribute no note occupancy. Limits are 128 quarter notes per request,
32 intersecting clips, 2,048 authored notes per clip, 4,096 potential performed instances
per clip, 128 loop passes, 32 performance stages and 8,192 intervals per representation.
Unsupported or excessive work sets `complete:false` and suppresses breathing suggestions;
partial numbers are not a verified result. At most 64 gap details are returned, alongside
their full count and a separate truncation flag. Large reports use `read_report` pages.

`audibility` separately flags sustain data and effect/send paths. Those are possible
causes, not proof of an audible tail. Instrument release, pedal state, other tracks,
solo/routing and bus effects are outside note occupancy. Compare the foreground stem
and full mix; a notated rest is not proof of audible silence.

## Prepare an explicit rest

Use `arrangement` validation with its current revision. Every edit includes one short
purpose, at most 256 UTF-8 bytes (keep Japanese purposes to a brief clause):

```json
{
  "operation": {
    "kind": "validate",
    "expected_revision": "COPY_FROM_CURRENT_INSPECTION",
    "changes": [{
      "purpose": "Let the lead release before the next phrase",
      "edit": {
        "kind": "breath",
        "clips": [12],
        "rests": [{"start_tick": 14880, "end_tick": 15360}],
        "min_note_ticks": 120,
        "protected_notes": [{"clip": 12, "index": 0}],
        "omit_notes": []
      }
    }]
  }
}
```

A note crossing a rest's start is shortened there, retaining at least `min_note_ticks`.
An attack inside a rest is refused unless its exact original `{clip,index}` is in
`omit_notes`. An omission removes only that explicitly named note; retained pitches,
attacks, velocities and order stay unchanged. Validation previews every changed original
index and its before/after duration, or the complete omitted note. All omissions must
fall inside a requested rest. Protected indices, vocal data/ornaments and touching
same-pitch continuations (possible ties, including across clips) are refused. Notes are
not split or uniformly gated.

Select 1–16 unmuted clips on one foreground instrument track and 1–16 ordered,
nonoverlapping rest windows. The selected passage is bounded to 128 quarter notes.
Singer/drum tracks are protected. A repeated clip requires explicit windows covering
the same relative rest in every audible pass; a proposal affecting an unrequested pass
is refused. Use independent clips for different phrasing between repetitions. Existing
recipes, authored curves, transforms, clip lengths, harmony and timeline maps stay intact.
Explicit recipe regeneration may later replace repaired notes, as with other score edits.

The candidate is checked against actual performed notes from the whole target track.
Another clip or a performance stage filling a rest refuses the proposal. Supported or
unknown active sustain during a rest also refuses it. Inspect/reset CC64 explicitly
before altering more notes; an instrument known to ignore CC64 does not gain pedal
support merely because a lane exists. Reverb and release remain audio-review conditions.

Review the prepared `note_changes`, then apply its `plan_id` with the original revision.
The existing arrangement contract supplies stale rejection, cancellation, exact retry
and one Undo/Redo. Failed validation changes no live score or history. An already empty
rest with no other edits adds no Undo. Independently reread notes and diagnose the same
phrase, then compare the same rendered bars. Limit local repairs to two attempts.

## Verification

Session tests cover occupancy unions, articulation versus phrase space, tempo changes,
clip/loop joins, muted clips, explicit roles, bounded work, indexed pickup omissions,
protected/vocal notes, possible ties, pedal state, overlapping clips, exact preservation
and one-Undo retries. A real PCM test measures reduced foreground energy in the repaired
rest while an added sustained texture keeps the full mix sounding. Existing generated
phrase endings are inspected without regeneration.

The ignored window test
`installed_agent_repairs_lead_breathing_and_preserves_generated_phrases` uses the
`AURIS_TOOL_SURFACE_EVAL` configuration described in
[agent tool validation](agent-tool-validation.md). A real agent diagnoses manual and
generated leads plus an intentional texture, repairs only two named lead endings in one
proposal, independently rereads the result, and numerically inspects the same foreground
stem and mix before and after. Exact state, recipe preservation and one Undo/Redo are
checked. Numeric verification alone does not establish natural phrasing or musical
improvement; use a separate listening comparison when an audio critic is available.

The [2026-10-23 verification record](../tools/eval/references/agent-breathing-2026-10-23.json)
includes real-agent and separate audio-model outcomes. The agent repaired the two exact
endings and preserved the generated reference and texture. The critic received both
stem and mix comparisons, but called the new ending abrupt/staccato and was uncertain
about intent. Measured note and PCM space increased, while natural phrasing was not established.
That is not a musical pass: consider a held arrival or an explicitly scoped dynamic taper
and compare again, rather than treating lower occupancy as a quality score.

The opt-in session test `installed_audio_critic_compares_breathing_stem_and_mix` writes
four 24 kHz WAV excerpts and the review to a fresh absolute directory supplied through
`AURIS_BREATHING_REVIEW_OUTPUT`. It uses `AURIS_AUDIO_*` settings and sends actual audio;
ordinary tests do not contact a critic. Raw local evidence remains outside Cargo cleanup.
