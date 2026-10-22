# Agent WAV clipping: reproduction and output headroom

Date: 2026-10-22.
Implementation: `d37db398`.

## Cause and deterministic reproduction

The [argument-contract rerun](agent-argument-errors.md) produced a 16-bar WAV with
two full-scale samples. The agent had normalized the original four-bar sketch to
a -1 dBTP ceiling, then extended the arrangement with a louder chorus. Those earlier
measurements described the sketch. The final export encoded the extended waveform
directly into integer PCM without measuring headroom before quantization.

The retained `arrange_and_undo/project-04.json` snapshot reproduces the original
24-bit WAV byte for byte: SHA-256
`df55e9dc5d7d83ed6a3e66398b8d58395c33db677010ce6f3cfedde5e436aa2d`.
It has 1,645,714 stereo frames at 48 kHz, 34.285708 seconds, and two full-scale
samples. A regression asserting safe headroom fails on the original implementation.
An independently overloaded FM fixture also fails before the change, with 88,106
full-scale samples in its 16-bit export.

The original floating-point render peaks at +0.058665 dBTP. Integer clamping loses
the over-range samples; measuring or attenuating the already clipped file cannot
restore them. The fix therefore acts before integer encoding. It does not rely on
the model remembering to repeat a document-level mix adjustment after every edit.

## Delivery contract

The existing `render` tool now accepts `ceiling_db`, default -1 dBTP, with a range
of -12..-0.1. It captures the requested range once per output and retains private
32-bit floating-point stages, preserving over-range values. The measured waveform
determines one attenuation-only gain; all stems share that gain to preserve their
relative levels. Final encoding reserves a few precision steps for quantization,
floating-point rounding and the peak interpolator. The encoded files are measured
again before publication; an unsafe result is refused.

The response reports `source_true_peak_db`, `attenuation_db`, `ceiling_db` and the
actual encoded measurements. Quiet exports are unchanged. This is uniform output
gain, not limiting or loudness normalization. It does not edit notes, performance,
project faders or duration, and cannot promise an incompatible LUFS target. Existing
file refusal, snapshot ownership, stale-result checks and cancellation are retained.
Preview uses the same staging path. All model backends and saved-project adapters
receive the shared command contract; no new tool was added.

Only one duration-sized audio buffer is retained at a time, with the existing
512 MiB buffer/inspection limit. Floating-point stages keep stems out of memory
and avoid rerendering potentially stateful plugins after measuring them. This is
worker-side delivery logic; no realtime processing, instrument, effect or score
writer algorithm changed. True peak remains the existing 4x interpolation estimate.

## Retained-project result

The same project now exports with:

| Measurement | Before | After |
| --- | ---: | ---: |
| Encoded peak / estimated true peak | 0 dB | -1.000004 dB |
| Full-scale samples | 2 | 0 |
| Frames | 1,645,714 | 1,645,714 |
| Integrated loudness | -16.643501 LUFS | -17.702171 LUFS |
| Output-only attenuation | None | -1.058670 dB |

The complete project remained identical. A separate byte-level PCM check confirmed
the full-scale counts and frame count. The new WAV hash is
`05c8e0d0998c2a397257b0b52bc641ac2eedf8c25114b16819d6c779d811a5af`.
The preserved fixed file is `target/eval/clipping-fix-2026-10-22/frozen-project.wav`.

Numeric regressions cover overloaded 16/24/32-bit exports, sample-by-sample linear
attenuation rather than hard clipping, exact preservation of a quiet floating-point
export, common stem attenuation, explicit ceilings and invalid ceiling refusal.
Existing tests cover protected preview, cancellation, stale results, races and
document preservation.

## Built-in Codex rerun

The unchanged [four-scenario, thirteen-prompt manifest](../tools/eval/references/codex-composition-scenarios.json)
was rerun on fresh unsaved fixtures through the real built-in `gpt-6.1-sol` Codex
host, using Bypass permissions and the configured `gemma4:e2b` critic. The
[measurement record](../tools/eval/references/agent-export-clipping-validation-2026-10-22.json)
separates these trials from the deterministic retained-project regression.

| Scenario | WAV duration | Estimated true peak | Full-scale samples | File verification |
| --- | ---: | ---: | ---: | --- |
| Eight-bar BGM | 19.2 s | -3.382780 dBTP | 0 | PASS |
| Existing 6/8 melody and accompaniment | 16 s | -16.430162 dBTP | 0 | PASS |
| Sketch, 16-bar form, chorus drums and Undo | 34.285708 s | -1.241621 dBTP | 0 | PASS |
| Generated phrases and replacement lead | 20 s | -12.924158 dBTP | 0 | PASS |

All files are 48 kHz/24-bit stereo, with requested duration within one sample.
Independent PCM byte checks confirmed zero full-scale samples, frame counts and
the exact hashes measured by the application. All four `verify_render` results
were PASS. Every export left the open project identical and unsaved. MIDI exports
reimported with 180, 142, 674 and 120 notes respectively.

The original melody including CC1 remained exact. The guitar's authored data was
preserved while expression changed, and Codex removed the ineffective strum after
reading its measurements. The arrangement retained its first four-bar clips,
changed only chorus drums, and restored the complete pre-drum-edit document with
guarded history Undo. Regeneration reduced the lead from 28 to 24 notes and raised
mean duration from 917.5 to 1206.46 ticks while preserving accompaniment and recipe.

These fresh mixes already had enough headroom, so their reported output attenuation
was 0 dB. They establish the native controller and export workflow; the retained
project and numeric overload regressions establish that needed attenuation fixes
clipping without relying on a different random composition. No statistical speed,
model-reliability or musical-quality claim follows from these one-run trials.

There was one unrelated, recovered recipe argument refusal: `gate:1.2` exceeds
`0..1`. Codex corrected it and completed regeneration. The refusal and the false
`no_argument_or_tool_errors` check remain in the record; this is not presented as
an argument-error-free run. No clipping or delivery verification failures remained.

Full transcripts, snapshots and audio/MIDI files remain under
`target/eval/clipping-fix-2026-10-22/`. Existing user projects and saved agent
preferences were not edited.

## Validation

- `cargo test --workspace --exclude auris-gpui`: 3,222 passed, 19 ignored.
- GPUI `ui::agent_chat` tests: 103 passed, six opt-in trials ignored.
- Retained-project clipping regression passed; all four native Codex processes
  exited successfully and their encoded-file checks passed independently.
- Formatting, diff checks, workspace/all-target Clippy and desktop/MCP builds passed.

The complete GPUI suite was not rerun for this worker-side change; the affected
agent host subset and real multi-turn controller trials were exercised.
