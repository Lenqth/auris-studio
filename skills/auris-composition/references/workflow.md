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

## Saved-project MCP sequence

The saved-project server exposes specification and composition tools:

1. `create_project` or `open_project`.
2. `inspect_project` to obtain stable track and clip IDs.
3. `list_presets`, `list_progressions`, `spec_reference`, and `check_spec` for generated scores.
4. `compose` to write a complete specification, or `edit_notes`/`generate_phrase` for targeted material.
5. `accompany` for backing parts under an existing melody.
6. `inspect_project`, `inspect_audio`/`analyze`, and `preview_composition` to verify.
7. `render` and `verify_render` for delivery files.

`compose` and `accompany` carry their own input contracts and do not take a clip revision field. `setup_tracks` uses its request ID/fingerprint contract. For operations that explicitly require an `expected_revision` or current revision, obtain it from the relevant inspection/read call and pass it unchanged. Read the affected project or clip again after writing. Use `tool_help` instead of guessing schemas; argument errors are feedback, not permission to retry unrelated calls.

## Live agent sequence

The desktop live agent has a separate, smaller catalog. Start with `discover_tools` and fetch `tool_help` for the returned candidates; do not assume that saved-only names such as `compose`, `spec_reference`, `check_spec`, `list_presets`, or `list_progressions` are available. Inspect the open document, read the target clip, then use the advertised live note/phrase/harmony tools and read back the result. Apply revision checks only to operations whose schema declares them.

## Minimal `.asong` approach

Use `spec_reference` for the current syntax. A generated specification should name its title, key/mode, tempo, sections, and parts, and should use a quoted built-in progression or explicit chord chart. Run `check_spec` before `compose`; diagnostics may include instrument availability, capacity, lyric fit, or unsupported source controls.

For a loop, make the musical boundary explicit in the score and render an unambiguous first/last bar. Verify that the end does not contain an uncontrolled tail before claiming seamless looping.
