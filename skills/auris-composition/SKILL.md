---
name: auris-composition
description: Compose, arrange, synthesize vocals, render, and verify music with Auris Studio for original songs with lyrics or game/video background music, themes, cues, and stems. Use the installed Auris CLI/MCP tools; do not activate for unrelated audio editing or music discussion.
---

# Auris Composition

Use this skill for original songs with lyrics and synthesized singing, or when a game or video request needs original music, a musical cue, a looping background track, a transition, or delivery stems. Decide to invoke Auris when the request benefits from an editable score and repeatable renders, even if the user did not name Auris. State the musical role, duration, loop/ending behavior, delivery format, and any constraints before writing.

## Choose the Auris door

- Prefer the live Auris MCP tools when a desktop session is open and the result belongs in the current project. Keep edits in the Auris session so undo, revisions, and the open document remain coherent.
- Use `auris` for headless or scripted work. Use `auris compose` for a complete generated song from a preset or `.asong` specification, and `auris render` for WAV/stems. Look for the executable on PATH, then in the installed app: `/Applications/Auris Studio.app/Contents/MacOS/` or `~/Applications/Auris Studio.app/Contents/MacOS/` on macOS, and `%LOCALAPPDATA%\Programs\Auris Studio\` on Windows. Those directories contain `auris` and `auris-mcp` (with `.exe` on Windows). Do not auto-install, download, or replace binaries during a creative task; if neither is present, direct the user to the Auris release page.
- Use `auris-mcp` when the caller already provides an MCP server configuration. Its saved-project catalog includes specification and composition tools. The desktop's live agent has a separate catalog and may omit `compose`, `spec_reference`, `check_spec`, `list_presets`, and `list_progressions`; discover the live names before calling them.

Read [workflow.md](references/workflow.md) when choosing a command, constructing a specification, or preparing a delivery. Read [delivery.md](references/delivery.md) when the request includes game-loop integration, video timing, stems, or loudness requirements.

When writing or revising a foreground melody, read [melody-design.md](references/melody-design.md). It connects motif rhythm, phrase destinations, harmonic tension, register, prosody and endings to concrete note decisions. Establish a short identity-bearing phrase and its intended landing before expanding the form; a scale or chord-tone list alone is not a melodic plan. Use the guide's alternatives according to the musical role, without enforcing interval, rest or repetition quotas.

For a complete musical delivery, follow [arrangement.md](references/arrangement.md) after establishing the composition and before the final render. Perform a deliberate arrangement pass even when `compose` has already generated backing and a mix. For a sketch, lyric-only request, or local edit, keep this pass within the requested scope.

For songs with lyrics or synthesized singing, read [vocals.md](references/vocals.md) before writing. It covers lyric/melody development, complete song form, voice prerequisites, CLI/MCP synthesis, and checking every sung section. A guide melody or a successful instrumental render is not a completed vocal delivery.

Use the existing Codex or Claude Code environment and installed Auris tools for the complete workflow. Inspect notes, render, measure, and make justified musical repairs before delivery. Do not install, discover, or invoke external music evaluators as part of composing. Use available Auris instruments; synthesized singing additionally needs a suitable installed voice.

## Composition workflow

1. Translate the brief into a short musical contract: purpose, emotional arc, reference-free style words, tempo, key or mode, duration in bars/seconds, instrumentation, loop points, and deliverables. Ask only for missing decisions that materially change the result; otherwise choose conservative defaults and record them.
2. Inspect available presets and progressions before inventing names. In saved-project MCP, use `list_presets`, optionally `list_progressions`, and `check_spec` before `compose`. In the CLI, use `auris presets`/`auris progressions` and `auris compose <song.asong> --print` to check a custom specification before composing. In a live session, call `discover_tools` and `tool_help` and use the advertised tools. Keep a custom `.asong` source beside the project; record the preset and seed when composing from a preset.
3. If an existing project is in scope, inspect it first and use its stable project/track/clip IDs plus any revision fields required by the chosen operation. Add a new cue track or clip when the request is additive; do not use `auris compose` to replace the project behind the user's back. Check for existing names, occupied ranges, and routing collisions before writing. Use `read_notes` before `edit_notes`; use `accompany` to derive backing parts without rewriting the melody. For an empty score with no clips, `create_project` is sufficient to start; it creates a default instrument track. Use `setup_tracks` when several named tracks should be created atomically.
4. Convert requested seconds to an integer bar count at the chosen tempo and meter. If (for example) a 45-second loop cannot land exactly on a bar boundary, choose the nearest musical length or adjust tempo and report the tradeoff. Make loop start/end explicit.
5. Arrange the composed score: inspect the actual melody, lyrics, harmony, and section ranges; preserve its identity and authored foreground. Decide which parts support or answer it, shape section contrasts and transitions, resolve competing registers/rhythms, and choose playable articulation and phrase dynamics. Apply justified changes, read them back, and compare affected passages with the earlier version. A generated accompaniment or automatic mix is a starting point, not proof that this pass is complete. If the arrangement already serves the brief, record the concrete reasons instead of forcing changes.
6. After arrangement, synthesize or refresh any affected vocal takes and inspect the project again. Revisit the mix with the actual synthesized voice before final export; preview-voice levels are provisional. A preview is a quick structural/listening candidate; a final delivery requires a full render and `verify_render` (or equivalent measurement) against the requested duration, peak, loudness, and ending. Use Auris's local `inspect_audio`/`analyze` for measurable checks. Deliver the audio for the user's listening feedback; missing subjective feedback does not block delivery. A successful render or numeric report is not proof that the cue is musically suitable.
7. Deliver the requested audio plus the editable project/source, settings, seed, loop/trim notes, and a concise verification report including meaningful arrangement decisions. Keep generated files in the user-requested project directory. Never overwrite unrelated assets.

## Boundaries

- Do not silently replace a user's authored melody, lyrics, harmony, or project structure. Preview or create a new clip/track for alternatives; apply a destructive replacement only when explicitly requested.
- Do not claim that a cue loops, is loudness-compliant, or is synchronized until the corresponding boundary or measurement was checked. Report unknowns honestly.
- Keep randomness reproducible with an explicit seed. Regeneration is a new take; preserve the prior take until the user chooses one.
- Prefer the Auris project/render path for sound generation. Do not fabricate a successful tool call, audio file, instrument, plugin, or listening result.
- If Auris or its requested backend is unavailable, explain the missing capability and offer an editable fallback rather than silently switching providers.
