# Agent composition quality trials

`tools/eval/composition_quality.py` runs the real agent-panel host in an isolated,
unsaved fixture. The versioned manifest contains eight development tasks and four
held-out tasks. Freeze it before collecting a baseline; do not tune against holdout
outputs. Use the same controller model, backend, manifest and repetition count for
both conditions. The initial implementation exercises the Codex backend.

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
Trials require an authenticated Codex CLI and can consume substantial inference
time. Each turn has a bounded timeout in the existing GPUI harness.

The manifest asks for built-in instruments and disables the external audio critic
in these composition trials. Critic calibration is a separate experiment, so its
availability and suggestions cannot confound the first composition comparison.

Each run records executable/DLL hashes, the manifest, source commit and Rust diff,
new Rust source files, per-trial configuration hash, transcript, project snapshots, WAV/MIDI artifacts,
encoded-file measurements, tool failures and elapsed time. Source provenance is
captured from the current checkout: build and start the runner before changing
that source. Controller services may change behind the same model name; retain
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

```sh
uv run --with pytest pytest tools/eval/test_composition_quality.py
uv run --with ruff ruff check tools/eval/composition_quality.py tools/eval/test_composition_quality.py
```
