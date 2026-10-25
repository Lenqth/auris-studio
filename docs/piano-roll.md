# Piano-roll snapping

[日本語](piano-roll.ja.md)

The piano roll has its own Snap menu. It opens from the editor header or the
**Piano Roll Snap…** command in the command palette; the command can also be
assigned a keyboard shortcut. Preferences are remembered between runs and do
not modify the song or its undo history.

**Smart** adjusts the snap interval with horizontal zoom. **Bar** and **Beat**
follow the time signature at the edit position. **Division** follows the
project's division setting. Fixed note values run from whole notes to 64th
notes, including triplets. **Free** allows one-tick editing. Fixed grids keep
their interval when zooming; dense lines are thinned for readability while
remaining aligned with the editing grid.

**Absolute** is the default: moving or copying a note aligns its start with the
song grid. Resizing aligns the grabbed end with that grid. **Relative** preserves
the original start or end offset, moving it by multiples of the chosen interval.
Other selected notes receive the same delta, preserving their timing and length
differences. A vertical-only drag preserves the notes' time positions.

New notes are placed on the song grid in either mode. Their initial duration
comes from the last edited note, or one grid interval. The **Snap ends** toggle
controls snapping while resizing and drawing a note's right edge.

During a drag, **Control-Shift** temporarily switches to ticks. On macOS,
pressing **Control** after starting the drag refines it to the project division
when that is finer. The existing free-edit gesture, **Command** on macOS or
**Ctrl** on Windows, also remains available. A modifier used to create a note
does not disable snapping for the drawing gesture.

## Time navigation

Drag the horizontal scrollbar below the arrangement, piano roll or drum grid to move
through time with an ordinary mouse. The arrangement and note editors share their time
position. **Shift + mouse wheel** scrolls horizontally over the grids; the ordinary
wheel also moves through time when the pointer is over a horizontal time scrollbar.
These gestures change the view without editing the song.
