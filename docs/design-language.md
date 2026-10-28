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

Every window's title bar is composed by `titlebar::composed` the way Atelier's `TitleBar` is:
the title sits on the window's centre line with a short subtitle beside it, Back (or, in the
editor, Undo and Redo) leads at the left, and frequent commands sit at the trailing edge. The two
sides grow from bases that make up for the native traffic lights and caption buttons, so the
title stays centred on the window rather than on whatever space the controls leave.

The chooser is built from Atelier's own components, as in the upstream `auris_screens` example:
a `Sidebar` of `ListItem`s (new, recent, and the setup guide at the foot), `OptionCard`s for how
to start, an `EmptyState` when there are no recent projects, and a toolbar-height footer holding
the one primary commit. The panel floats over `MetalBackdrop`, as in the liquid-metal variant of
the upstream `ambient` example, and occludes pointer events so its controls do not also
manipulate the metal beneath it. A card selects a starting method; Create or Enter commits it,
and double-click also opens it. The setup guide is the panel's third page rather than a window
of its own; a first run opens the chooser on it, and Settings brings it forward in whichever
window shows the chooser. Inactive windows hold a still frame of the metal, and Atelier's motion
policy stills it entirely.

Choosing "Create with AI" hands the panel over to the song workshop: for one `motion.normal` the
metal stays mounted while a panel-coloured shape grows from the chooser panel to the whole body
and the workshop fades in over it; Back plays the same hand-over in reverse. Only the shape
moves — the arriving screen is laid out at its final size from the first frame — so nothing
reflows. The workshop follows the upstream compose screen: one centred column of transcript and
candidate cards (elevated, accent-bordered when chosen, agent-washed only while being written),
the Atelier composer with the send button as its one primary, and a row of workshop options
beneath it with "Use my preferences" as a `Switch`.

Toolbar icon buttons are Atelier ghost buttons: no fill or hairline until hovered, and filled
with their signal colour when latched.

On Windows, `gpu-allocator` 0.27 must resolve to `windows` 0.58, matching Atelier's `wgpu-hal` 25.
The committed lockfile includes that resolution; after regenerating it, check `cargo tree -p
gpu-allocator@0.27.0` and use `cargo update -p gpu-allocator@0.27.0` if it selects 0.57 instead.
