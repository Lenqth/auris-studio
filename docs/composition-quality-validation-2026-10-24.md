# Composition harness validation — 2026-10-24

[日本語](composition-quality-validation-2026-10-24.ja.md)

This report covers production records, immutable composition candidates, scoped
phrase/cadence variation, task-specific prompts and cross-backend evaluation.
It separates engineering correctness, delivery and musical preference. The
candidate catalog and modular prompt remain experimental; no human preference
ratings have been collected.

## Conditions and retained evidence

Three experiments serve different purposes:

- The runtime smoke asks for one exact pitch change, preview without editing,
  adoption in the next turn, Undo/Redo, and 16-second 24-bit WAV/MIDI delivery.
  Codex uses `gpt-6.1-sol`, Claude Code uses `sonnet`, and Rig uses local
  `qwen3.8:27b`. These different models are not a backend quality ranking.
- The prompt ablation uses one development case (`loop_title`), three trials per
  condition, one frozen binary and identical tool catalogs. It compares legacy
  instructions with the initial modular profile. The later pending-candidate
  instruction repair is assessed separately, not substituted into this cohort.
- The holdout uses all four held-out cases, three trials per condition, the
  unchanged v1 manifest and Codex `gpt-6.1-sol`. The baseline is the frozen phase-3
  runtime; the new condition includes phases 4–6 with the legacy prompt. The
  four new-condition case shards run concurrently. No trial is removed from the
  cohort because it failed, and wall time is not a performance benchmark.
  The later repair changes only modular instructions, leaving the evaluated
  legacy condition unchanged.

Every suite retains source provenance, executable/DLL hashes, configuration,
transcript, document snapshots, encoded measurements and artifact hashes under
`target/quality-final`. Reusing a binary verifies its original suite's runtime
hashes instead of attributing it to the current checkout. The local evidence
directory is ignored by Git; the
[numerical report](../tools/eval/references/composition-quality-validation-2026-10-24.json)
retains suite hashes, executable hashes, project snapshot hashes and the audit
summary. Runtime-manifest hashes use sorted compact JSON; the original suites
retain every DLL hash.

## Holdout delivery and preservation

| Task | Before WAV/MIDI | After WAV/MIDI | Before failed calls | After failed calls |
| --- | ---: | ---: | ---: | ---: |
| waltz | 3/3 | 3/3 | 7 | 5 |
| foreground-preserving folk backing | 3/3 | 3/3 | 4 | 7 |
| rhythm-first loop | 3/3 | 3/3 | 9 | 15 |
| theme, contrast, return | 3/3 | 3/3 | 10 | 10 |

All 24 deliveries match the requested duration within one sample and have zero
full-scale samples. All final documents match the requested tempo and meter.
All six folk trials retain the complete original melody clip,
including controller data, and the original tempo/meter maps. There were no
external critic calls. The failed-call total increased from 30 to 37, although
all trials recovered and delivered. This does not demonstrate a reliability or
musical-preference improvement. No holdout output was used to tune the evaluated
legacy condition.

`target/quality-final/holdout-listening/listening.html` contains all 12 anonymous
before/after pairs. Musical ratings remain uncollected; note counts, delivered
files and preserved foreground data cannot fill those missing ratings.

## Prompt ablation

| Profile | Completed with WAV/MIDI | Tool calls per trial | Failed calls per trial | Wall time per trial |
| --- | ---: | --- | --- | --- |
| legacy | 3/3 | 116, 102, 125 | 7, 2, 3 | 384, 288, 376 s |
| initial modular | 3/3 | 156, 220, 147 | 0, 4, 3 | 408, 522, 436 s |

All six encoded WAVs are 19.2 seconds with zero full-scale samples. Failed calls
were recovered before delivery. Modular instructions produced fewer failed calls
in this small sample but more total calls (523 versus 343). Builds and concurrent
inference confound elapsed time. Backend usage counters were unavailable and
reported zero; those zeros are not measurements of token consumption. No token
cost or musical-quality improvement can be inferred.

`target/quality-final/prompt-listening/listening.html` contains three anonymous
pairs. Keep its separate `private-key.json` hidden while collecting ratings.

## Exact-edit runtime checks

The corrected-fixture Claude trial passed the complete contract without tool
errors. The first Codex trial completed transport and wrote files but failed the
edit contract because it cancelled the pending round. After the instruction
repair, a separate Codex trial passed with zero tool errors. Both successful
deliveries have exactly the requested document change and the same encoded WAV
SHA-256 (`481fcfd4d386180f27c4c7a85b9683d7cbae9a649acea75e507efc614db2b7db`).
This establishes an exact edit, not a preference improvement or a success rate
for arbitrary requests.

The local Rig trial also passed the complete contract at 131,072 configured
context tokens, with three recovered schema errors: a missing operation tag,
an untagged render destination and an unsupported production-record operation.
Its final document and WAV hash match the successful CLI trials. It took 2,287
seconds on this machine, with Ollama reporting partial CPU/GPU offload. The final
reported input contexts were 25,319, 27,624 and 27,949 tokens across the three
turns; reported conversation output totaled 16,629 tokens. These last-context
counts are not peak or cumulative input usage. The 64k guard failure was an
**estimate**, not proof that the model actually needed more than 64k tokens.
Profile that conservative guard before treating a larger allocation as a fix.

The corrected Codex run made 347 tool calls, including 261 controller-curve reads;
Claude made 30 calls. Exhaustively re-reading individual controller lanes is an
observed efficiency problem. A future batch preservation report should reduce
that cost without weakening exact-content checks. Model names and backend
conditions differ, so these counts do not establish a general model ranking.

## Findings that changed the implementation

The first Codex smoke used dotted production-record keys. The validator rejected
them although they are safe identifiers, and the schema did not expose the
restriction. Dots are now accepted; the schema declares the pattern and length.
A regression test covers replacement, persistence and atomic rejection of a
slash-containing key. The original two failures remain in the diagnostic trial.

Source-reference hashes initially named a suite that could still append trial
results. The runner now reads the reference bytes once and saves an immutable
`source-suite.json`; its regression test updates the original file and verifies
that the snapshot still matches the recorded hash. For these already completed
runs, the exact original reference snapshots were recovered from the unchanged
provenance and trial prefixes, and every recovered byte sequence was required to
match its previously recorded SHA-256. No source hashes or trial results were
rewritten to make them match.

Smoke v1 also exposed an evaluation-fixture mistake: its seed used 480 ticks per
quarter while Auris uses 960, making it four bars rather than the requested eight.
Codex requested clarification instead of exporting; Claude exported the actual
eight-second seed and disclosed the mismatch. Neither is a valid eight-bar
delivery test. Smoke v2 uses a versioned eight-bar fixture with a regression test.
The original v1 seed remains unchanged on both sides of the holdout comparison.

In the first corrected-fixture Codex trial, the modular instruction to adopt or
cancel led the agent to cancel a candidate that the user wanted to adopt on the
next turn. The host correctly rejected the cancelled round, preserving the
document. File delivery still succeeded, demonstrating why transport and file
existence cannot substitute for checking the requested edit. The common prompt
now explicitly retains deferred candidates and forbids cancelling merely to
finish a turn. A separate trial evaluates that repair; the failed trial remains.

The initial local Rig trial reached its 65,536-token budget after candidate
creation and inspection. The budget guard stopped it without changing the score.
A separate 131,072-token experiment uses the model's supported context range;
application defaults are unchanged. Larger context alone is not a quality claim.

## Validation and adoption gate

The workspace passed 4,516 Rust tests including doctests before the final two
regressions; both added regressions also passed. Workspace Clippy and formatting
passed after the changes. The evaluator and critic scripts passed 12 pytest
tests and Ruff. Candidate tests cover exact application, one Undo, idempotent
retry, protected content, cancellation, stale/foreign/expired rounds, budgets,
preview publication and measurements of the actual encoded preview files.

The production record persists attributed context separately from tool-owned
observations. Its tests cover resume, reset races, missing IDs, bounded output and
revision invalidation, including edit-then-Undo. The saved-project candidate
round trip and live worker contracts are covered independently of real inference.

These checks do not establish better music. Human blind ratings, request fit and
per-case failures remain necessary before changing the default prompt. Local
context exhaustion and unnecessary verification calls are operational limitations
to address in subsequent development, without tuning against holdout outputs.
