# Hosted plugin playback performance

The CLAP host keeps active parameter displays in an atomic snapshot. Automation written by
Auris and global parameter-value events emitted by the plugin update that snapshot on the
audio side without allocating or taking a lock. Preset values are read before activation;
explicit value-rescan and dirty notifications refresh the snapshot on the session thread.
Drawing a parameter panel does not query an active plugin's values. Saving still reads the
plugin's authoritative state.

This matters for plugins that protect their DSP and parameter getters with the same lock.
A main-thread query can otherwise wait for a process block, and a plugin using a try-lock
in its audio callback can instead produce a silent block when the getter owns that lock.

## Reproduce and measure

Build both Auris and the selected plugin in Release mode. Stop builds and other heavy work
before recording a result. The profiling example uses a paced audio thread and concurrent
parameter-panel reads, without an audio device:

```sh
cargo build --release -p auris-clap --example profile_playback
target/release/examples/profile_playback /path/to/AmiSynth.clap 128 10 mixed
target/release/examples/profile_playback /path/to/AmiSynth.clap 512 10 mixed
```

On Windows, use the `.exe` file. The arguments are the selected CLAP file, block size,
duration in seconds, and either copies of each instrument (1–16) or `mixed` to play one of
each instrument in the library together. Each instrument receives the same four held notes.
The three conditions are `closed` (no panel queries), `direct` (the previous full-table
getter pattern), and `cached` (the active display snapshot).

The output reports process-block p99 and maximum time, blocks exceeding the audio deadline,
panel-query p99 and maximum time, silent blocks, total output energy, and a PCM checksum.
For deterministic presets, compare the checksum against `closed` as well as energy.
Timing covers instrument processing and excludes checksum and energy calculations. At 48 kHz, the
deadline is about 2.67 ms for 128 frames and 10.67 ms for 512 frames. Compare aggregate block
time when several instruments share a callback. CPU percentage alone does not show whether
a single callback meets its deadline.

Silent blocks can also be a natural end to a decaying preset, especially drums. Compare the
same phrase and output energy against `closed`; do not interpret every silent block as an
underrun. The tool is a focused host/parameter-contention probe, not a replacement for a real
project, device-driver, thermal, or power-mode test. It does not recreate the whole desktop.

## AmiSynth investigation

The initial investigation used AmiSynth commit
[`857f796`](https://github.com/Lenqth/AmiSynth/tree/857f796af43683d4a0a2507d359806c6c5c2bf1d)
built with MSVC in Release mode. Its CLAP process function uses a try-lock and its parameter
getter takes the same mutex. A direct-query run reproduced skipped Synth audio blocks;
cached reads avoid entering that getter during playback. The original report describes
multiple instruments, short UI freezes and audio interruptions on a Ryzen 5800H laptop.
Measurements on the development machine do not establish that laptop's supported track
count. Recheck the actual project, preset versions, sample rate, buffer size, power mode,
and device driver before attributing remaining interruptions to CPU capacity.
