# Melody and arrangement domain trials

`tools/eval/references/melody-domain-v1.json` adds a focused development cohort
to the [composition quality harness](composition-quality.md). It keeps the
existing five preference axes and adds explicit listening questions. The runner
shows those questions to the listener; it does not send them to the composing
agent or turn them into numerical composition targets.

The six development tasks cover a singable question/answer, a narrow repeated
hook, an instrumental interlude/return, protected 6/8 accompaniment, a held
arrival, and a sustained ambient scene. The last two cases are reserved for
holdout evaluation after development decisions are fixed. The continuous and
narrow-range cases matter: a melody guide must not force rests, big climaxes,
wide ranges or new material into every musical role.

## Collect evidence

Build the GPUI test executable and follow the isolation and provenance procedure
in [composition-quality.md](composition-quality.md), passing this manifest:

```powershell
uv run tools/eval/composition_quality.py run --executable <gpui-test-executable> --manifest tools/eval/references/melody-domain-v1.json --output target/melody-domain-before --repetitions 3
# Build the changed prompt from its own checkout before collecting the after condition.
uv run tools/eval/composition_quality.py run --executable <gpui-test-executable> --manifest tools/eval/references/melody-domain-v1.json --output target/melody-domain-after --repetitions 3
uv run tools/eval/composition_quality.py blind --before target/melody-domain-before --after target/melody-domain-after --output target/melody-domain-listening
```

Keep the backend/model, tool catalog, prompt profile, manifest, repetitions and
delivery settings equal. `legacy` versus `modular` is a separate experiment:
both profiles load the shared composition guidance. Existing user replacement
system prompts retain their own text. Run `--split holdout` only after the
guidance is frozen. Do not overwrite the older evaluation manifests or compare
their runs directly with this new task cohort.

For each pair, retain a bar/timestamp and reason for a preference, tie or
inability to judge. Listen to the whole request before isolated excerpts. For
the interlude, include both joins; for a loop, audition a contiguous wrap. A
foreground-only render can diagnose a part, but preference needs the full
arrangement. The generated page preserves original levels, so note loudness
differences explicitly; it does not secretly loudness-normalize the files.

## Keep three kinds of evidence separate

| Evidence | What to record | What it cannot establish |
| --- | --- | --- |
| Score/contract | Named foreground, section length, protected-note equality, recurring rhythm/contour, actual chord at a target, note/lyric assignment, planned release | Memorability, phrasing naturalness or audible masking |
| Render measurement | Actual duration, peak, silence/tail, current vocal coverage when applicable | Emotional effect, arrangement quality or preference |
| Listening | Request fit and the manifest's questions, with locations and stated uncertainty | A population-wide benefit from a few favourable examples |

Treat a non-chord note, repeated pitch, wide leap, overlap or sustained phrase as
something to interpret in context. Record a concrete musical problem before
changing it. No minimum rest percentage, required number of transformations,
maximum leap rate or corpus-average range is a pass condition.

The [MIDI reference study](midi-reference-research-2026-10-25.md) supplies
descriptive observations and measurement limitations. Its gap/interval metrics
are not preference labels. Research-informed guidance is in the skill's
[melody reference](../skills/auris-composition/references/melody-design.md) and
[arrangement reference](../skills/auris-composition/references/arrangement-design.md).

This manifest and rubric are evaluation infrastructure. No before/after human
preference result is implied by adding them or passing their software tests.
