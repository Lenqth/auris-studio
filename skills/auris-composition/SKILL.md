---
name: auris-composition
description: Compose, arrange, render, and verify music for games and videos with Auris Studio when a project needs original background music, themes, cues, or stems. Use the installed Auris CLI/MCP tools; do not activate for unrelated audio editing or music discussion.
---

# Auris Composition

Use this skill when a game or video request needs original music, a musical cue, a looping background track, a transition, or delivery stems. Decide to invoke Auris when the request benefits from an editable score and repeatable renders, even if the user did not name Auris. State the musical role, duration, loop/ending behavior, delivery format, and any constraints before writing.

## Choose the Auris door

- Prefer the live Auris MCP tools when a desktop session is open and the result belongs in the current project. Keep edits in the Auris session so undo, revisions, and the open document remain coherent.
- Use `auris` for headless or scripted work. Use `auris compose` for a complete generated song from a preset or `.asong` specification, and `auris render` for WAV/stems. Resolve the installed executable with the host's normal command discovery; do not download or replace binaries during a creative task.
- Use `auris-mcp` when the caller already provides an MCP server configuration. The saved-project tools and the live agent expose the same toolbox vocabulary.

Read [workflow.md](references/workflow.md) when choosing a command, constructing a specification, or preparing a delivery. Read [delivery.md](references/delivery.md) when the request includes game-loop integration, video timing, stems, or loudness requirements.

## Composition workflow

1. Translate the brief into a short musical contract: purpose, emotional arc, reference-free style words, tempo, key or mode, duration in bars/seconds, instrumentation, loop points, and deliverables. Ask only for missing decisions that materially change the result; otherwise choose conservative defaults and record them.
2. Inspect available presets and progressions before inventing names. For a generated song, call `list_presets`, optionally `list_progressions`, then `check_spec`; fix every diagnostic before `compose`. Keep the `.asong` source beside the project so the intent remains editable.
3. If an existing project is in scope, inspect it first and use its stable project/track/clip IDs and revisions. Add a new cue track or clip when the request is additive; do not use `auris compose` to replace the project behind the user's back. Check for existing names, occupied ranges, and routing collisions before writing. Use `read_notes` before `edit_notes`; use `accompany` to derive backing parts without rewriting the melody. For a blank project, create tracks atomically with `setup_tracks` or use `create_project` followed by explicit track creation.
4. Convert requested seconds to an integer bar count at the chosen tempo and meter. If (for example) a 45-second loop cannot land exactly on a bar boundary, choose the nearest musical length or adjust tempo and report the tradeoff. Make loop start/end explicit.
5. After a write, inspect the project again. A preview is a quick structural/listening candidate; a final delivery requires a full render and `verify_render` (or equivalent measurement) against the requested duration, peak, loudness, and ending. Use `inspect_audio`/`analyze` for measurable checks and `listen` only when an actual listening critic is available. A successful render or numeric report is not proof that the cue is musically suitable.
6. Deliver the requested audio plus the editable project/source, settings, seed, loop/trim notes, and a concise verification report. Keep generated files in the user-requested project directory. Never overwrite unrelated assets.

## Boundaries

- Do not silently replace a user's authored melody, lyrics, harmony, or project structure. Preview or create a new clip/track for alternatives; apply a destructive replacement only when explicitly requested.
- Do not claim that a cue loops, is loudness-compliant, or is synchronized until the corresponding boundary or measurement was checked. Report unknowns honestly.
- Keep randomness reproducible with an explicit seed. Regeneration is a new take; preserve the prior take until the user chooses one.
- Prefer the Auris project/render path for sound generation. Do not fabricate a successful tool call, audio file, instrument, plugin, or listening result.
- If Auris or its requested backend is unavailable, explain the missing capability and offer an editable fallback rather than silently switching providers.
