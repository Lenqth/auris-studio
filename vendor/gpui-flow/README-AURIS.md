# GPUI Flow in Auris Studio

MIT-licensed source from [pacifio/gpui-flow](https://github.com/pacifio/gpui-flow),
commit `cfa578758ee251657f15f281d893a08b85aedbff`.

The dependency uses the gpui-pre snapshot Auris builds against (0.3.10), under the
crate name `gpui`. `ModifiersExt::is_secondary` reads the platform's command modifier at
compile time, since gpui-pre's own `Modifiers::secondary` asks the running `App` and the
graph reads it inside closures that already hold it. The application supplies its theme
and custom node renderer.

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
