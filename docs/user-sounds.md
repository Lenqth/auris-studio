# Reusable sounds

Select an instrument or drum track, adjust its instrument and insert effects, then choose
**Save as user sound…** in the track inspector. Enter a new name. Saved sounds appear in the
library's **User sounds** section and in library search, and remain available after restarting.
Names must be nonempty and unique; an existing sound is never overwritten implicitly.

Choose a saved sound with a destination instrument track selected. The command replaces its
instrument settings and complete insert chain, including effect order, parameters and bypass
switches. Undo restores the previous sound and its automation in one step. Notes, track level,
pan, sends and routing stay in the song. Existing instrument and insert automation is removed
when applying a sound; sidechain sources are not carried between tracks.

SoundFont sounds remember the library file and bank/program. Hosted CLAP and VST3 sounds also
remember their binary paths and opaque plugin state. Those libraries must remain installed.
Applied settings are embedded in the project, so moving or deleting the user sound library
does not change an existing project.

The physical bowed-string instrument appears as **Strings**, with **Violin**, **Viola**,
**Cello** and **Contrabass** starting patches beneath it. These are editable parameter variants
of the existing bowed-string model. They do not transpose the score; write notes in the
instrument's intended register. Each can be edited, combined with effects and saved as a new
user sound.
