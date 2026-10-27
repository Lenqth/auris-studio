# Design language integration

Auris Studio uses the Atelier design language from `uthree/design-language`, pinned in
`crates/auris-gpui/Cargo.toml` to revision
`ae5b23ed61b23e6e0366c9aeaaae134c009da0c3`. The application selects Atelier's Glass direction and
adapts its existing `Theme` through the `Theme::atelier` field, `Theme::install`, and
`Theme::sync_atelier`. Existing Auris palette identifiers remain available while shared controls,
agent surfaces, and the application shell read the corresponding Atelier tokens.

At startup, `atelier::Assets::with_fallback` serves Atelier's embedded icon set first and then
`ui::provider_logo::ProviderAssets`. The fallback contains the provider marks and Auris-specific
24px SVGs for volume, mute, punch, notes, keyboard, and knob controls. The application icon files
in `crates/auris-gpui/assets/` are generated Atelier element-cell exports for Auris Studio (`Au`),
including the editable, size-specific, symbolic, PNG, ICO, and ICNS forms. The redistributed
canonical icon files carry the upstream Apache-2.0 text in `assets/ATELIER-LICENSE`.

The editor transcript and song workshop share `ui::agent_components`: user messages, assistant
messages, status and error notices, thinking indicators, the composer frame, its wrapping control
lane, and keyboard-operable send/stop commands. The adapter keeps
the existing Auris session state, editable fields, attachments, and permission handling in their
own views while giving both workflows the same Atelier agent presentation primitives.

Glass Dark is the first-run default; Glass Light is available in Appearance. Previously saved
schemes and custom palettes retain their identifiers and document colours. Interface surfaces
follow Atelier for the two Glass presets, while custom schemes supply their own colours to the
same components. Native glass backdrops are applied when a project window opens and when its
theme changes; the editor canvas remains opaque.

The screen compositions follow the upstream `showcase` arrangement and `auris_screens` examples:
selected track headers use the accent fill, clips use rounded coloured bodies and hairline
outlines, and the trailing agent dock opens at 380 px. Both conversation entry points share the
same introductory state, message forms and inset composer. Saved dock widths remain intact.

The chooser places the navigation, descriptive start cards and commit footer on an opaque panel
over `AmbientBackdrop` with `Ambience::Aurora`, as in the upstream `ambient` example. A card
selects a starting method; Create or Enter commits it, and double-click also opens it. Background
motion can be paused in the title bar; the choice persists as `animate_welcome` in
`appearance.json`. Inactive windows hold a still frame. The backdrop is unmounted on entering
the editor or workshop, respects Atelier's motion policy and uses its static CPU fallback when
hardware rendering is unavailable.

On Windows, `gpu-allocator` 0.27 must resolve to `windows` 0.58, matching Atelier's `wgpu-hal` 25.
The committed lockfile includes that resolution; after regenerating it, check `cargo tree -p
gpu-allocator@0.27.0` and use `cargo update -p gpu-allocator@0.27.0` if it selects 0.57 instead.
