# Agent composition quality trials

[日本語](composition-quality.ja.md)

For phrase design, reference-MIDI limitations and arrangement-specific listening
questions, use the separate [melody domain trials](melody-domain-evaluation.md).
Keep their versioned cohort separate from the original quality trials below.

`tools/eval/composition_quality.py` runs the real agent-panel host in an isolated,
unsaved fixture. The versioned manifest contains eight development tasks and four
held-out tasks. Freeze it before collecting a baseline; do not tune against holdout
outputs. Use the same controller model, backend, manifest and repetition count for
both conditions. The runner supports Codex CLI, Claude Code and Rig/Ollama through
the same live application host (`--backend codex|claude|rig`, `--model <name>`).

Build the GPUI **test binary**, locate its executable in Cargo's JSON output, and
pass that executable to the runner (not `auris-studio.exe`). The runner copies the
binary and runtime DLLs before execution. A fresh output directory is required.

```powershell
cargo test -p auris-gpui --bins --no-run --message-format=json > build.jsonl
uv run tools/eval/composition_quality.py run --executable <test-executable> --output target/quality-before --repetitions 3
# Rebuild after the implementation change, then collect the matching condition.
uv run tools/eval/composition_quality.py run --executable <test-executable> --output target/quality-after --repetitions 3
uv run tools/eval/composition_quality.py blind --before target/quality-before --after target/quality-after --output target/quality-listening
```

Use `--case authored_folk` for a targeted experiment, preserving the same cohort on
both sides. Use `--split holdout` only after development decisions are fixed.
CLI trials require the corresponding authenticated CLI; Rig trials require a
running local Ollama server and an installed tool-capable model. They can consume
substantial inference time. Each turn has a bounded timeout in the GPUI harness.
`--context-tokens` controls the local model's context budget (default 65536); it
does not change CLI model limits or the application's default preferences.

The manifest asks for built-in instruments and disables the external audio critic
in these composition trials. Critic calibration is a separate experiment, so its
availability and suggestions cannot confound the first composition comparison.

Each run records executable/DLL hashes, the manifest, source commit and Rust diff,
new Rust source files, per-trial configuration hash, transcript, project snapshots, WAV/MIDI artifacts,
encoded-file measurements, tool failures and elapsed time. Source provenance is
captured from the current checkout: build and start the runner before changing
that source. When reusing a frozen runtime, pass `--source-suite <suite.json>`:
the runner verifies identical runtime hashes and retains the original source
provenance rather than attributing the binary to today's checkout.
It freezes the exact reference bytes in `source-suite.json`, so a still-running
source suite cannot invalidate the recorded reference hash by appending results.
Controller services may change behind the same model name; retain
timestamps and compare under the closest available service conditions.

`transport_pass` means that every requested turn completed. `delivery_present`
means both requested output files exist. Neither establishes musical quality or
even correct musical constraints. Read the request, score and measurements as
well as listening. Failed trials remain in the comparison cohort; report their
frequency instead of dropping them from a musical success rate.

Open `listening.html` and keep `private-key.json` away from the listener until
ratings are complete. The page randomizes pair order and A/B identities and
exports ratings plus evidence. The five axes are request fit, melody, form,
ensemble interaction and sound. A tie and an inability to judge are distinct.
The outputs are independent model trials, not identical musical seeds. Original
levels are preserved; loudness differences must be considered explicitly.

Report per-case results and failure counts before an aggregate preference rate.
Three repetitions expose variability but do not by themselves demonstrate a
statistically reliable preference improvement. The symbolic and learned rulers
in [evaluation.md](evaluation.md) are supporting measurements, not substitutes
for human listening or permission to optimize against the holdout set.

The separate `composition-runtime-smoke-v2.json` manifest checks exact local
variation, preview, candidate adoption, Undo/Redo and delivery. Its versioned
`melody_six_eight_eight_bars` fixture uses native 960 ticks per quarter and spans
eight 6/8 bars at 90 BPM (16 seconds). The original `melody_six_eight` fixture in
quality manifest v1 is a **four-bar seed** (11520 ticks, 8 seconds), despite its
eight construction iterations. Keep those original bytes for frozen v1 paired
comparisons: an eight-bar accompaniment request must extend that shorter seed.
Do not silently substitute the corrected fixture into one comparison condition.

Runtime smoke v1 incorrectly requested the current eight bars from that four-bar
seed. Its runs remain diagnostic evidence, not valid eight-bar delivery tests.
Use v2 for new runtime checks. Neither runtime smoke version measures preference.

```sh
uv run --with pytest pytest tools/eval/test_composition_quality.py
uv run --with ruff ruff check tools/eval/composition_quality.py tools/eval/test_composition_quality.py
```

## 2026-10-24 implementation pilot

The [retained numerical report](../tools/eval/references/composition-quality-pilot-2026-10-24.json)
compares baseline `406277cc` with implementation `da89dc96`, using Codex CLI 0.163.0 and
`gpt-6.1-sol`. This exploratory pilot selected the first two development tasks while the
larger run was in progress; it is not a preregistered efficacy study. Both selected tasks
have three trials per condition, with no selected trial omitted. The four holdout tasks
were unevaluated in that pilot. The complete manifest remains available for broader development runs.

| Condition | Task | Completed with WAV/MIDI | Failed tool calls, total | Wall time range | Trials reporting changed accompaniment |
| --- | --- | ---: | ---: | ---: | ---: |
| before | loop_title | 3/3 | 10 | 294–334 s | not reported |
| before | authored_folk | 3/3 | 9 | 286–345 s | not reported |
| after | loop_title | 3/3 | 10 | 198–266 s | 1 |
| after | authored_folk | 3/3 | 6 | 236–296 s | 2 |

Failed tool calls include arguments that the agent corrected before completion. Wall times
overlapped builds and other measurements; do not interpret them as a performance improvement.
12/12 deliveries have the requested duration (19.2 seconds for the title loop,
16 seconds for the folk arrangement); 12/12 have zero full-scale samples.
External critic calls: 0. 6/6 folk trials retained the original melody clip
in full, including notes and curves.
The new adaptation reports distinguish changed backing from explicit-rhythm, sustained-texture,
role and empty-foreground bypasses. A changed accompaniment is not a preference score.

The complete workspace passed 4,499 Rust tests including doctests; Clippy and formatting passed.
The evaluator scripts passed ten pytest cases and Ruff. The symbolic ruler was byte-identical.
One fixed pop-band render was also byte-identical; its TuneJury and MuQ-MuLan deltas were zero.
These are regression checks, not evidence that the new compositions are better.

Foreground selection is request-local. This pilot covers explicit `accompaniment` generation
and regeneration, not generic GUI regeneration of those clips. For music assessment,
`target/quality-evidence/listening/listening.html` contains six anonymous A/B pairs and a
rating form; the separate key reveals their identities. Original levels are retained.
No human preference ratings have been collected, so musical improvement remains unproven.
The [critic calibration](audio-critic-calibration.md) likewise did not validate the current
audio model for automatic musical selection.

## Follow-up validation of production records and candidates

The [follow-up report](composition-quality-validation-2026-10-24.md) covers
conversation records, immutable candidates, scoped variation, modular prompts
and real backend trials. It retains all 24 holdout deliveries and the six-trial
development prompt ablation, including recovered tool errors and runtime-smoke
failures. Human preference remains unscored. The candidate catalog and modular
prompt stay opt-in; see [candidate tools](composition-candidates.md) and
[workflow configuration](composition-workflows.md).
