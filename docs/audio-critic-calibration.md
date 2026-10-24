# Audio critic calibration

`listen` separates single-excerpt diagnosis from anonymous comparison. Diagnostic `focus`
may ask a descriptive question. Comparison requires `compare_to` plus one typed `aspect`
(`balance`, `phrasing`, `ensemble`, `sound`, `musical_coherence`) and rejects `focus`.
Only the criterion and randomly ordered A/B audio reach the critic; neither project metadata
nor the controller's explanation of its edit is supplied. The caller receives the A/B mapping
after the request. Both WAVs retain a common protective attenuation.

The production response parser validates JSON shape, mode, excerpt labels, up to eight
observations and evidence times within the supplied duration. Preferences require evidence.
A diagnostic inability to judge cannot contain an edit suggestion. `assessment_error` reports
invalid responses separately from `cannot_judge` or `tie`; delivery and raw response remain
available for debugging. Timestamp validation checks bounds, not whether the described event
really happened. Model confidence is a self-report, never a calibrated probability.

The development worker uses this exact production path:

```powershell
cargo build -p auris-session --example critic_calibration
uv run tools/eval/audio_critic_calibration.py --worker target/debug/examples/critic_calibration.exe --output target/critic-calibration
```

The configured audio server/model must already be available. Connection settings use the
same `AURIS_AUDIO_*` variables as `listen`. Use `--limit 2` for an initial delivery smoke test;
it is not a completed calibration. Requests have the production five-minute network bound.

The frozen synthetic cohort contains three short tonal phrases and four controls per phrase:
identical audio, silence, hard clipping and disordered pitches. Every pair runs in both
orders, giving 24 judgments. The clipping and pitch variants match the reference's RMS;
no limiter or independent peak normalization obscures their difference. WAV hashes, worker
hash, raw replies, validation failures and outcomes are retained.

Identity pairs must not elicit a preference. `cannot_judge`, invalid responses and transport
failures are separate outcomes, not calibration successes. Compare the two orderings before
interpreting preferences on impairment controls. Those controls are screening hypotheses:
silence, distortion and dissonance can be intentional music, so a preference on this synthetic
cohort does not prove general musical judgment. Inspect observations as well as verdicts.

The [2026-10-24 pilot](../tools/eval/references/audio-critic-calibration-2026-10-24.json)
used local `gemma4:e2b` with the recorded model digest. All 24 requests delivered audio:
15 responses were `cannot_judge`, eight were ties and one omitted required fields.
No impairment preference was demonstrated. Valid structured delivery therefore does not
validate this model for selecting musical edits. The raw replies and exact WAV hashes are
retained in the reference report; the WAVs can be regenerated with the script.

Until a model demonstrates reliable discrimination and human agreement on representative
music, its observations are advisory. The controller must not automatically select edits
from critic preference alone. Use the blinded human comparisons in
[composition-quality.md](composition-quality.md) for actual musical improvement.

```sh
uv run --with pytest pytest tools/eval/test_audio_critic_calibration.py
uv run --with ruff ruff check tools/eval/audio_critic_calibration.py tools/eval/test_audio_critic_calibration.py
```
