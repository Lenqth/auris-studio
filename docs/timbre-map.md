# Timbre map

[日本語](timbre-map.ja.md)

Open **Timbre Map** at the top of the library. Auris measures the built-in melodic instruments
and melodic presets of loaded SoundFonts in the background. The counter shows completed sounds;
Cancel, Close and Escape stop a running scan. Use **Rescan Library** after loading another font.
The prototype accepts up to 512 sounds per scan and keeps the results in memory until the
document is replaced or a new scan starts.

Each point is a sound. Hover for its name or click to hear its reference note. The **All Sounds**
list also reaches coincident points. Selecting a sound highlights its eight nearest neighbors
and lists them under **Similar Sounds**, ordered by feature-space distance (smaller is closer).
Color denotes an acoustic k-means group, not a named instrument class. **Use on Selected Track**
adopts that sound as one undoable edit; browsing and audition do not change the document.
Adopting a built-in sound restores its default patch and clears its instrument automation,
including when the selected track already uses that instrument. Mixer settings remain in place.

Audition uses the selected playable track, or the first playable track. Its mute, fader, inserts
and routing apply, so choose an unmuted dry track for comparisons. Previews are matched to a
common RMS target with a peak ceiling; closing the map stops the last preview.

## Cluster names and library colors

After a scan, measured default instruments and melodic SoundFont presets use the map's cluster
color and name in the library tree and search results. Unmeasured choices retain their role
colors. Identity uses the instrument id and exact font/bank/patch, so equal display names do not
share annotations accidentally. Closing the map keeps these annotations available in the library.

Each cluster initially takes the name of the measured source closest to its centroid in the full
feature space. The legend shows the source count; its tooltip lists up to eight central examples.
These names describe exploratory groups rather than fixed instrument categories.

With an agent model configured in Settings, **Name Clusters with AI** requests short labels in
the interface language. It sends only cluster sizes and bounded representative sound names to the
configured backend, without audio, project contents or conversation history. Suggestions are
display labels inferred from metadata, not a listening assessment. The request cannot use tools
or edit the project. Every cluster must receive one distinct valid name before any names change.
Cancel, closing the map, rescanning or replacing the document invalidates a pending request;
failure preserves the current labels. **Restore Representative Names** reverts the suggestions.
Colors and names remain in memory for this scan; rescanning or replacing the document clears them.

## Measurement and coordinates

Every source uses fresh instances for six matched triggers: MIDI 48, 60 and 72 at velocities
0.45 and 0.85, a 600 ms hold, and 400 ms after note-off. The middle-C, higher-velocity recording
is the preview. A source silent under any reference condition is omitted and counted explicitly.
Analysis always measures the raw instrument output before the project mixer.

Each trigger contributes 93 deterministic DSP features:

* Twelve MFCCs (excluding coefficient zero), each with mean and standard deviation, separately
  over attack, held body and release: 72 values. The 32 mel filters cover 40 Hz to the lesser of
  10 kHz and Nyquist. Channel powers are summed before calculating normalized log energies.
* Existing drum-analysis spectra over onset-relative attack, body and tail: low/body/high energy
  fractions, log centroid, flatness and peak concentration, for 18 values.
* Onset delay, time to 90% of captured energy, and sustained-energy ratio: three values.

The six feature blocks are concatenated, preserving corresponding pitch/velocity conditions,
then each of the 558 dimensions is standardized over this catalogue. Constant dimensions become
zero. Euclidean distance in this space drives nearest-neighbor search and deterministic k-means
(rounded square root of the sound count, bounded to 1–12 requested groups). These are exploratory
groups whose usefulness should be checked by listening.

The map displays three principal components in an Atelier `Plot3d` scatter plot, with the fraction
of standardized variance retained. Drag to rotate, use the rotation buttons from the keyboard,
or choose 3D, Top, Front or Side. A cross marks the selected sound. The sound lists remain
available for coincident or hidden points, and the group legend identifies the point colors.
Axes are statistical directions without fixed labels such as brightness or decay. Displayed
distance is only an approximation; similarity ordering always uses all 558 dimensions. Rescanning
a changed catalogue can change both coordinates and groups. The computation needs no pretrained
weights, Python runtime or network service. Plot rendering uses Atelier's GPU path when available
and its CPU fallback otherwise.

Implementation: `auris_dsp::timbre` extracts and projects features; `Session::timbre_map_job`
snapshots sources for a worker, `TimbreMap::nearest` answers similarity queries, and
`Session::use_timbre_sound` adopts a result. Numeric tests cover level/polarity invariance,
silence and invalid PCM, PCA distance preservation for rank-two and rank-three fixtures, deterministic grouping,
and degenerate catalogues. A desktop harness test checks read-only browsing and undoable adoption.
