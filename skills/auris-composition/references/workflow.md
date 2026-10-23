# Auris workflow reference

This reference records the concrete interfaces the `auris-composition` skill may use. Discover exact fields with MCP `discover_tools`/`tool_help` when the installed build differs.

## Headless CLI

```text
auris presets
auris progressions
auris compose --preset <name> -o <output.auris> [--seed <integer>]
auris compose <song.asong> --print
auris compose <song.asong> -o <output.auris> [--seed <integer>]
auris info <project.auris>
auris render <project.auris> -o <preview.wav> [--stems <directory>]
```

The CLI's exact flags are authoritative for the installed version. `--print` parses a specification and prints normalized TOML without writing a project; use it as the headless syntax check. Run `auris help` when a command rejects an argument. A Save As path may become a nested `<name>/<name>.auris` project; use the path printed by the command or `auris info` rather than guessing.

## MCP/live tool sequence

The shared toolbox names are the stable vocabulary:

1. `create_project` or `open_project`.
2. `inspect_project` to obtain stable track and clip IDs.
3. `list_presets`, `list_progressions`, `spec_reference`, and `check_spec` for generated scores.
4. `compose` to write a complete specification, or `edit_notes`/`generate_phrase` for targeted material.
5. `accompany` for backing parts under an existing melody.
6. `inspect_project`, `inspect_audio`/`analyze`, and `preview_composition` to verify.
7. `render` and `verify_render` for delivery files.

Every saved edit must use the current project or clip revision returned by inspection. Read the clip again after writing. Use `discover_tools` and `tool_help` instead of guessing schemas; argument errors are feedback, not permission to retry unrelated calls.

## Minimal `.asong` approach

Use `spec_reference` for the current syntax. A generated specification should name its title, key/mode, tempo, sections, and parts, and should use a quoted built-in progression or explicit chord chart. Run `check_spec` before `compose`; diagnostics may include instrument availability, capacity, lyric fit, or unsupported source controls.

For a loop, make the musical boundary explicit in the score and render an unambiguous first/last bar. Verify that the end does not contain an uncontrolled tail before claiming seamless looping.
