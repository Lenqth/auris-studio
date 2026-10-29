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

## Cluster names, icons and library colors

After a scan, measured default instruments and melodic SoundFont presets use the map's cluster
color and name in the library tree and search results. Unmeasured choices retain their role
colors. Identity uses the instrument id and exact font/bank/patch, so equal display names do not
share annotations accidentally. Closing the map keeps these annotations available in the library.

Each cluster initially takes the name of the measured source closest to its centroid in the full
feature space. The legend shows the source count; its tooltip lists up to eight central examples.
These names describe exploratory groups rather than fixed instrument categories.

With an agent model configured in Settings, **Name and classify with AI** requests short labels in
the interface language and selects icons from fourteen built-in monochrome SVG presets. It sends
cluster sizes, bounded representative sound names and family counts from every cluster member to the
configured backend, without audio, project contents or conversation history. Suggestions are
display labels inferred from metadata, not a listening assessment. The request cannot use tools
or edit the project. Every cluster must receive one distinct valid name and an allowed icon before
any suggestions change.
Cancel, closing the map, rescanning or replacing the document invalidates a pending request;
failure preserves the current labels. **Restore Representative Names** reverts the suggestions.
Colors and names remain in memory for this scan; rescanning or replacing the document clears them.

Library and map rows choose an icon for each sound: a manual correction takes priority, followed
by the sound's own instrument-family name, then the cluster's AI icon or a family shared by at
least two thirds of its members. This keeps a clearly named guitar recognizable inside a mixed
acoustic group. The classifier recognizes common abbreviations and Japanese family names,
ignores a map entry's library prefix, and leaves ambiguous names generic. It does not assume that
arbitrary SoundFont program numbers follow General MIDI.

Select a sound in the map and open **Correct sound category…** to choose a labeled SVG preset.
Tab and Space reach and select each choice. **Use automatic category** removes the correction.
Corrections are saved immediately in application preferences and survive rescans, document changes
and restarts. Future AI naming requests include these corrections in their family counts.
SoundFonts use resolved file path, bank and patch so equal display names and
document-local font IDs do not share corrections accidentally. A failed save keeps the previous
choice and reports the error. Cluster labels and colors keep their acoustic meaning.

The preset families are piano, organ, guitar, bass, strings, brass, woodwinds, drums,
bells/mallets, voice, synth, pad, plucked strings and other/mixed. All assets use the existing
24×24 grid and two-pixel monochrome strokes. Metadata regression tests cover standard named GM
families, mixed clusters, library-name leakage and ambiguous substrings; they do not establish
accuracy on unnamed sounds or listening quality. Agent transport is tested with mock responses;
classification quality still depends on the configured model and the supplied metadata.

The opt-in live fixture uses synthetic metadata for thirteen named families, one unknown group
and one mixed group. On 2026-10-29 the configured Codex backend (`gpt-6.1-terra`) selected the
expected icon in all 15 cases, including generic icons for the unknown and mixed groups.
This is a small metadata fixture, not an estimate of accuracy on an audio library. Run it with
`cargo test -p auris-agent live_configured_model_selects_icons_from_metadata -- --ignored --nocapture`;
it sends those synthetic names and counts to the configured backend.

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
zero. Euclidean distance in this space drives nearest-neighbor search and deterministic k-means.
The initial group count is the rounded square root of the sound count, bounded to 1–12. Groups
larger than twice the average initial population (with a floor of twelve sounds) are candidates
for recursive binary refinement, up to 24 total groups. Each split uses farthest-first and eight
fixed-seed D²-weighted initializations, retaining the smallest within-group squared error. Both
children need at least three sounds and the split must reduce the parent's error by at least 5%.
Smaller existing groups retain their membership. Duplicate or weakly separable groups can remain
above the population target. These are exploratory groups whose usefulness should be checked by
listening; a finer partition does not establish distinct instrument classes.

Refinement changes group membership only. The global feature normalization, PCA coordinates,
retained variance, audition recordings and nearest-neighbor distances stay the same. Cluster
names and colors are assigned after refinement, including optional agent naming of all groups.

A local comparison using 171 measured built-in and Fluid R3 GM sounds produced 16 groups instead
of 12, with the largest group falling from 87 to 27 sounds. The former 87-sound group split into
groups represented by Halo Pad (27), Strings (26), Trumpet (20) and Saw Wave (14). Total within-group
squared error fell from 59,514 to 53,206 (10.6%). Mean silhouette fell from 0.106 to 0.048: these
finer groups have overlapping boundaries, so the result demonstrates browsing granularity and
lower internal error, not improved global separation or listening quality.

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
