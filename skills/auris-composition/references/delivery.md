# Delivery reference

## Games

Ask whether the cue is a loop, one-shot, adaptive layer, or transition. Keep loopable cues bar-aligned and provide a loop start/end in bars or samples. Export a full mix and, when requested, stems whose names describe their role (`music_drums.wav`, `music_harmony.wav`, `music_melody.wav`). Preserve the project and seed so a later cue can be regenerated consistently.

## Video

Confirm frame rate, start time, duration, and whether the editor needs a two-pop or handle. Render a full-length WAV at the requested sample rate and provide stems only when they improve the edit. Check the final file's duration, peak, and loudness with Auris analysis/verification before reporting it as ready.

## Verification report

Include:

- source project and `.asong` path;
- seed, tempo, key/mode, duration, and loop boundaries;
- rendered file paths and format;
- measurements actually performed (duration, peak/true peak, LUFS, and render status);
- unresolved musical or integration assumptions.

Do not turn a preview into a delivery claim. If the platform's loudness target, sample rate, or loop behavior was not supplied, state the chosen default and make it easy to revise.
