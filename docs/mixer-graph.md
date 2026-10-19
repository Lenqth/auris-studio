# Mixer signal flow

The Mixer opens on **Signal flow**. Each track or bus has one node containing
its name, fader, pan, mute, solo and meters. Effects remain individual nodes in
processing order. Fader controls still act after all inserts. Bus and master nodes sum their incoming signals.
Arrows show the direction audio travels. Bypassed effects remain in the chain.

Drag a track's right port, or its last effect's right port, to a bus's left input port. **Output** changes the
strip's destinations by adding a unity post-fader branch; it keeps every existing
output. **Sends** adds a branch with its own level and pre/post-fader tap. The master
and several buses can receive the same strip at once. The console's **Output** menu
checks each connected destination; choosing one toggles that branch. Add a bus from the toolbar
to create a shared return or another processing branch.

An effect that accepts a sidechain has a port on its top edge. Drag another
strip's output port there to key the effect from that strip. Output, send and
sidechain connections all participate in cycle validation. A connection that
would lead back to its source is refused before the document or undo history
changes.

Wire roles use themed colors, labels and distinct line styles:

| Connection | Line | Label |
| --- | --- | --- |
| Output | Solid | Out |
| Send | Dashed | Send ID, level, and pre-fader mark when applicable |
| Sidechain | Dotted | Sidechain |

The legend below the canvas shows the current colors. Wires sharing the same
source and destination use separate curved lanes, including multiple sends to
one bus. Their labels follow their lanes and identify each send independently.

Click a wire or its label to select it. Selection adds a thicker accent outline
while retaining the role color and line style. Press Delete or Backspace, or right-click the wire or label and choose
**Delete connections**. Hold Ctrl on Windows or Command on macOS while clicking
to select several wires; their deletion is one Undo step. Undo and Redo restore
the actual routing, including removed send automation. A disconnection removes
only the selected branch. Deleting every output leaves the strip disconnected;
it does not create a new master route. Thin insert-chain arrows follow effect
order; use the effect menu to change that order.

Use **Add effect** on a track node. Open an effect by its name, toggle
**Enabled** to bypass it, or use its **Effect** menu to reorder, remove or choose
a sidechain. Gain, pan, mute, solo, meters and send levels are on the track node.
A send's context menu changes its pre/post-fader tap or removes it.

Drag nodes to arrange the workspace, or drag/scroll the background to pan.
**Arrange nodes** places dependencies from left to right. **Console** provides
the channel strips; **Signal flow** returns to the graph. Both views edit the
same project and share undo/redo. Keyboard focus moves through the controls with
Tab; routing and effect menus also provide keyboard access to connections.

The saved project contains the routing, effect order, parameters and send tap
points. Existing projects open with the same processing paths. The system's
architecture is documented in `auris_session::guide`.

## MCP and live agents

MCP exposes `mixer_graph` with a project handle/path and an `operation` object.
Start with `{"action":"read"}`. Nodes use stable `track:N`, `track:master`
and `effect:N` IDs, independent of names and effect positions. Read pages are
in processing order; `next_offset` continues the node list and each page includes
its nodes' outgoing edges. A track's `output_node` identifies its terminal insert
or the track itself. Both the track ID and its terminal effect ID can address an
outgoing connection. Send edges include a stable `send_id` and pre/post-fader tap.

For example, after reading the actual IDs:

```json
{"project":"/absolute/path/Song/Song.auris","operation":{"action":"connect","source":"track:1","target":"track:2","kind":"send"}}
```

The rig agent uses flat tools against the open document:

| Task | Live tool | MCP operation action |
| --- | --- | --- |
| Read graph | `read_mixer_graph` | `read` |
| Fader / pan / mute / solo | `set_mixer_track` | `set_strip` |
| Connect / disconnect | `connect_mixer` / `disconnect_mixer` | `connect` / `disconnect` |
| Add / remove inserts | `add_mixer_effect` / `remove_mixer_effect` | `add_effect` / `remove_effect` |
| Reorder / bypass | `move_mixer_effect` / `enable_mixer_effect` | `move_effect` / `enable_effect` |
| Send level / tap | `set_mixer_send` | `set_send` |
| Read / set effect parameters | `mixer_effect_parameters` / `set_mixer_effect_parameter` | `parameters` / `set_parameter` |

Both transports use the same session operations and cycle checks. Invalid and
stale edges are refused before editing. Connecting an output adds a destination
without replacing the others. Disconnecting removes only the named branch;
an empty output list stays disconnected. Removing a send or effect also removes its automation. MCP saves
actual changes with a checkpoint; reads and unchanged connection retries do not
write. Live edits remain unsaved and undoable. Read-only and Plan modes permit
graph inspection but deny edits; ordinary Edit mode asks before disconnecting or
removing an effect, according to the existing permission policy.
