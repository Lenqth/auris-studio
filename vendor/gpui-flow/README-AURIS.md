# GPUI Flow in Auris Studio

MIT-licensed source from [pacifio/gpui-flow](https://github.com/pacifio/gpui-flow),
commit `cfa578758ee251657f15f281d893a08b85aedbff`.

The dependency uses Auris's released GPUI 0.2.2. `PixelsExt` bridges the pixel
conversion API. The application supplies its theme and custom node renderer.

An externally managed flow queues completed connection gestures for the session.
Only a successful session command changes the displayed edges. The session owns
undo, redo, deletion and cycle validation; moving a visual node only changes its
presentation. Allowed connection pairs come from that same session validation.

Embedding requires translating edge geometry and handle hit testing by the dock
canvas's window origin. Handle release checks direction, and release outside the
canvas cancels a gesture. Keyboard modifiers use GPUI's secondary modifier so the
same interaction works on macOS and Windows.

The application window tests in `auris-gpui::ui::mixer_graph` exercise the source
and target handles with actual press, move and release events. Session tests
exercise output/send/sidechain cycles, stable graph identities and undo.
