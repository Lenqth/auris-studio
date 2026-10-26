# Local composition refinement

`tools/eval/composition_refine.py` runs the real panel composition harness, renders
its saved score, evaluates the actual WAV with SongEval, and asks the agent for a
bounded revision. It preserves the initial composition and every candidate. This
is optional development tooling; the application does not load these model weights.

The default decision is **retain the baseline for review**, even if every predicted
axis rises. There is no sum, optimization target, automatic winner, repeated search
for a score threshold, or automatic replacement of the user's original project.

## Setup

Use a Python 3.11 uv environment with the tested
[`songeval-requirements.txt`](../tools/eval/songeval-requirements.txt), a local
[SongEval checkout](https://github.com/ASLP-lab/SongEval) and checkpoint, and a local
[MuQ snapshot](https://huggingface.co/OpenMuQ/MuQ-large-msd-iter).
The initial download requires network access; scoring subsequently forces offline
mode. Keep these optional assets outside the release tree.

```sh
uv venv --python 3.11.15 .local-evaluations/songeval/.venv
uv pip install --python .local-evaluations/songeval/.venv/Scripts/python.exe -r tools/eval/songeval-requirements.txt
```

On macOS/Linux the interpreter is `.venv/bin/python`. The tested source revision is
`848fb2ff3a2a9d64dcb20d46f07238c28abd7add`; the tested MuQ snapshot is
`0562a57814f6f8bbd9fdea0a25921a2fce1a841a`. A configuration names existing local files:

```json
{
  "upstream": "C:/models/SongEval",
  "upstream_revision": "848fb2ff3a2a9d64dcb20d46f07238c28abd7add",
  "checkpoint": "C:/models/SongEval/ckpt/model.safetensors",
  "muq": "C:/models/MuQ-local-snapshot",
  "threads": 4
}
```

The adapter checks the actual source revision and records source/configuration,
head and MuQ weight hashes, package versions and preprocessing. SongEval's README
states CC-BY-NC-SA 4.0 while its LICENSE file says Apache 2.0 at that revision.
This integration uses the separately acquired model for local development research
and does not redistribute it or assert commercial-use clearance.

## Score existing audio

```sh
<songeval-python> tools/eval/songeval.py --config songeval-config.json --output scores.json before.wav after.wav
```

Alternatively pass `--manifest inputs.json` containing an array of `{id, path}`
objects. Optional `group` and `condition` (`before`/`after`) fields produce per-axis
paired deltas. Repeated basenames receive separate identities. Duplicate conditions
within a comparison are rejected. A failed item remains in the report and makes
the command unsuccessful. Whole recordings are decoded to mono 24 kHz with librosa,
without gain normalization, and passed through MuQ hidden layer 6 and the SongEval
head. The default length cap is 120 seconds; use a fixed, documented excerpt policy
for longer tracks, or explicitly raise `--max-seconds` with sufficient memory.

Outputs are Coherence, Musicality, Memorability, Clarity and Naturalness on the
model's 1–5 scale. Naturalness refers to vocal breathing and phrasing; Clarity to
song structure. There is no separate rhythm score or timestamped repair diagnosis.
Memorability does not measure a listener's later recall. Do not substitute a
minimum target score for the musical brief, or interpret tiny differences as a
validated preference margin. Instrumental scores, mastered recordings and synthetic
vocals have different domain limitations. The [SongEval paper](https://arxiv.org/abs/2505.10793)
describes a full-song benchmark of English and Chinese songs. Japanese synthesized
singing and short instrumental excerpts are not a demonstrated preference benchmark
for this head; treat the local listening evidence as a separate calibration.

## Run the automatic loop

Build the ignored real-backend test target as described in
[composition trials](composition-quality.md), and supply that test executable, a
renderer and an authenticated agent backend. SongEval inference is local; the
composition agent still uses its configured provider/account and normal usage.
For a reused executable, pass `--source-suite` from the run which captured that
exact runtime rather than attributing it to today's source checkout.

```sh
uv run tools/eval/composition_refine.py --recipe tools/eval/references/composition-refine-pop-v1.json --output .local-evaluations/my-refinement --executable <auris-gpui-test-executable> --auris <auris-cli> --evaluator-python <songeval-python> --songeval-config songeval-config.json --seed-project <Song/Song.auris> --voice <voice.voicevox.json> --speaker <speaker-name>
```

The example recipe uses the existing controlled pop-vocal seed: 24 bars at 112 BPM,
with a `Vocal` track and clip IDs 5–12. For another song, write its own brief, focus
track, permitted fields and clip IDs. Omit `--seed-project` to compose from a blank
fixture; the agent is instructed to create the named monophonic foreground track.
Omit voice arguments for instrumental music. A supplied seed is copied with its
project-local Audio folder, and checked for changes afterwards. Existing external
library references still require the same available assets on the machine.

A recipe declares one to three interventions before any scores are seen. Each
intervention has an `instruction`, allowed existing-note `fields` drawn from
`pitch`, `start`, `length`, optional `clip_ids`, and `max_changed_notes` (1–128).
All revisions branch independently from the initial composition, so a poor prior
candidate cannot become the new default. Prior per-axis deltas are disclosed as
uncertain context, never as a target. The agent must justify a specific musical
edit from the score and brief; no change is a valid outcome.

The hard checks preserve note count/order, written lyrics, accompaniment, form,
tempo, sounds, gain and effects. They reject overlapping/out-of-bounds foreground
notes, off-scope edits, excessive note changes, silent/clipped/nonfinite output,
changed duration or changed backing render, and changed evaluator conditions.
These checks concern experimental validity, not musical merit. A failed step
stops the run and retains its logs and artifacts; do not hide it with a successful
rerun. The foreground restriction currently supports monophonic singer/instrument
tracks, not arbitrary arrangement or effect optimization.

The output contains `report.json`, exact recipes/prompts, frozen runtime/source
evidence, agent transcripts and score snapshots, per-stage native projects, full
mix/foreground/backing WAVs, edit details and SongEval reports. `listening.html`
compares the retained WAVs at original gain. Foreground/backing are separate renders
through the full master chain, not additive stems. Labels are disclosed: this is
an audit/listening page, not a blinded preference study. Retain an untouched cohort
if using these findings to tune instructions further.

## Checks

```sh
uv run --with pytest --with numpy --with soundfile pytest tools/eval/test_songeval.py tools/eval/test_composition_refine.py tools/eval/test_composition_quality.py
uv run --with ruff ruff check tools/eval/songeval.py tools/eval/composition_refine.py tools/eval/test_songeval.py tools/eval/test_composition_refine.py
```

The [initial local evaluator trial](local-music-evaluators-2026-10-26.ja.md) records
same-input repeatability, silence controls and disagreement with human ratings.
These calibration limits still apply inside an automatic loop.

The [first complete loop](songeval-refinement-2026-10-26.ja.md) retains a fresh
pop melody and a one-note rhythm revision with identical backing. The
[reference-song calibration](songeval-reference-2026-10-26.ja.md) includes the
user's existing NAS collection, one fresh official-source download and a
within-song excerpt-sensitivity check. Neither study establishes a selection threshold.
