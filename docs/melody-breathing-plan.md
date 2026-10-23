# Melody breathing: implementation plan

An agent-composed lead was reported to play continuously, with too little space between
phrases. Treat this as an arrangement requirement alongside instrument performance,
section orchestration, automation and SFX. The reported project has not yet been
reproduced, so the cause remains open.

## Establish which path loses the space

The generated melody writer already calls `melody_rhythm::close_phrase` and `breath`
at phrase endings (`crates/auris-compose/src/parts/melody.rs`). This does not establish
that an agent's manually authored notes, explicit rhythm, performance transforms or
rendered instrument preserve an audible breath. Trace the actual tool transcript and
project through these paths before changing the writer.

Use three fixtures: a manual lead whose notes join continuously, a generated lead with
phrase endings, and a lead with notated gaps obscured by sustain, overlapping layers
or effect tails. Keep the original score and a render of the same bars as baselines.

## Diagnose phrases and continuity

Extend read-only arrangement inspection with bounded measurements of authored notes
and the first performed pass. Merge overlapping note intervals before measuring note
coverage; note counts and onset density alone cannot show whether a melody rests.
Report the longest continuous run, gaps and their positions, and phrase-ending space
in both ticks/beats and seconds through the tempo map. Distinguish short articulation
gaps from phrase rests with an explicit measurement threshold, reported in the result.
Include joins across clips and loop boundaries, with completeness and truncation flags.

Assign or confirm the foreground role and phrase spans before issuing suggestions.
A pad, drone or deliberately continuous ostinato must not inherit a lead's breathing
requirements. Suggested phrase lengths and rest sizes depend on style, meter, tempo,
vocal text and the section's intensity; avoid a universal silence quota or a forced
rest every four bars. Report suspected sustain/controller, layering and release-tail
causes separately from note occupancy. Symbolic gaps are not proof of audible silence.

## Write space into new music

For direct agent composition, plan calls, continuations, answers and breathing points
before writing notes. Give phrase endings and occasional foreground dropouts an explicit
duration. Let accompaniment answer selected gaps without filling every gap or making
the whole ensemble stop. Vary the spacing between verse, chorus and bridge while
preserving recognizable motifs. Inspect the written and performed result after edits.

For generated composition, verify existing phrase-boundary handling over short and odd
section lengths, explicit rhythms, meters, repeated clips and performance settings.
If a writer defect is reproduced, fix the responsible path with seeded regression cases.
Run both symbolic and learned evaluation baselines before and after any writer change,
as required by the repository's evaluation contract.

## Repair an existing lead explicitly

Add an opt-in proposal that names the target clips, phrase boundaries and intended
rests. Preview exact note-duration changes or omitted pickups before application.
Prefer releasing a phrase's ending or leaving a response gap to uniformly shortening
every note. Preserve pitch order, recognizable motif, harmony, lyrics and total song
length; protect tied notes, intentional legato and lyric slots. Any necessary score
change must be explicit in the proposal, rather than hidden in an instrumentation edit.
Use the arrangement revision, validation, one-Undo application and retry contracts.

If the notes already rest, inspect pedal resets, gate/release, overlapping foreground
layers and effect sends. Propose the responsible scoped change instead of deleting
more notes. Keep an intentional reverb tail when it still lets the next phrase read
clearly; the target is contrast, not digital silence.

## Acceptance and evaluation

- Unit fixtures distinguish continuous leads, short detached notes without phrase
  space, meaningful rests, overlapping voices, clip joins, muted clips and tempo changes.
- Repairs preserve protected score data, are scoped to selected phrases, and support
  exact retry plus one Undo/Redo. Invalid or stale proposals leave the project untouched.
- Real-agent trials cover manual writing and generated composition, then require an
  independent readback and rendered comparison of the same bars. Compare the foreground
  stem and full mix so accompaniment and tails cannot conceal the cause.
- Measure note occupancy and continuous-run changes, level balance and phrase timing;
  also listen for natural phrasing, motif identity and unwanted staccato. Numeric
  improvements alone do not establish musical improvement. Record unresolved failures
  and bound local repair attempts to two.

Implement in this order: reproducible fixtures and read-only diagnosis, composition
guidance, explicit repair proposals, then real-agent and listening evaluation. The
current arrangement harness supplies inspection, scoped performance/automation edits,
prepared proposals and verification plumbing; this plan adds phrase-space diagnosis
and deliberate score repair to that workflow.
