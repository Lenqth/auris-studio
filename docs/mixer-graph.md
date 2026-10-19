# Mixer signal flow

The Mixer opens on **Signal flow**. Each strip has an input, its effects in
processing order, and a fader. Bus and master inputs sum their incoming signals.
Arrows show the direction audio travels. Bypassed effects remain in the chain.

Drag a fader's right port to a bus's left input port. **Output** changes the
strip's main destination; **Sends** adds a parallel branch and keeps its existing
output. The master input can receive a main output. Add a bus from the toolbar
to create a shared return or another processing branch.

An effect that accepts a sidechain has a port on its top edge. Drag another
strip's fader port there to key the effect from that strip. Output, send and
sidechain connections all participate in cycle validation. A connection that
would lead back to its source is refused before the document or undo history
changes.

Use **Add effect** on an input node. Open an effect by its name, toggle
**Enabled** to bypass it, or use its **Effect** menu to reorder, remove or choose
a sidechain. Gain, pan, mute, solo, meters and send levels are on the fader node.
A send's context menu changes its pre/post-fader tap or removes it.

Drag nodes to arrange the workspace, or drag/scroll the background to pan.
**Arrange nodes** places dependencies from left to right. **Console** provides
the channel strips; **Signal flow** returns to the graph. Both views edit the
same project and share undo/redo. Keyboard focus moves through the controls with
Tab; routing and effect menus also provide keyboard access to connections.

The saved project contains the routing, effect order, parameters and send tap
points. Existing projects open with the same processing paths. The system's
architecture is documented in `auris_session::guide`.
