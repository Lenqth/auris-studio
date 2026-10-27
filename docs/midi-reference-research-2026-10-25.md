# Symbolic melody reference study (2026-10-25)

This note records a bounded, reproducible reference study for Auris composition.
It is evidence for descriptive priors and workflow questions, not a claim that a
statistical average is a recipe for a good song.

## Sources and provenance

The primary source is [POP909](https://github.com/music-x-lab/POP909-Dataset) at
git commit `d83e6edba6872a704f5d3b8b32f5cb540088dae6`,
the dataset accompanying the ISMIR paper [POP909: A Pop-song Dataset for Music
Arrangement Generation](https://archives.ismir.net/ismir2020/paper/000089.pdf).
The paper describes 909 professionally reviewed popular-song arrangements,
with aligned MIDI tracks named MELODY (vocal lead), BRIDGE (secondary/lead
instrument) and PIANO (accompaniment), plus beat, tempo, key and chord
annotations. It reports approximately 60 hours and songs spanning the 1950s
through the 2010s. The repository is MIT licensed; the original songs and
recordings remain third-party material.

No copyrighted MIDI is checked into this repository. For the run below, a
shallow sparse checkout fetched folders `001` through `030` into the operating
system temporary directory. The output contains paths and aggregate-compatible
per-file metrics, but no note sequences. In a new analysis directory, reproduce
the source selection and run the pinned script environment:

```powershell
git clone --filter=blob:none --no-checkout https://github.com/music-x-lab/POP909-Dataset.git target/pop909-reference
git -C target/pop909-reference sparse-checkout init --cone
$songFolders = 1..30 | ForEach-Object { 'POP909/{0:D3}' -f $_ }
git -C target/pop909-reference sparse-checkout set -- $songFolders
git -C target/pop909-reference checkout d83e6edba6872a704f5d3b8b32f5cb540088dae6
uv run tools/eval/midi_reference_analysis.py target/pop909-reference --pattern 'POP909/*/*.mid' --limit 30 --output target/pop909-reference-analysis.json
```

The JSON records actual Git origin/HEAD, working-tree status, the exact selected
paths, MIDI/index/analyzer hashes and library versions. A different source tree
is never labelled with a hardcoded POP909 origin. The retained result is
[melody-reference-analysis-2026-10-25.json](../tools/eval/references/melody-reference-analysis-2026-10-25.json).

The paper's dataset description is on pp. 1–3 (dataset size and date span on
p. 3; track roles and annotations on p. 3). The checked source index begins
with “123我爱你”, “123木头人”, “9420”, “Dear Friend” and “I Believe”. These
are dataset entries, not current popularity or chart rankings. The paper and
repository are useful because the melody track is explicitly
identified. Lakh MIDI is much larger, but its tracks are not marked as lead
melody or accompaniment, so treating its highest notes as a melody would mix
instrumental lines and chords. The [music21 corpus documentation](https://music21.org/music21docs/moduleReference/moduleCorpus.html)
likewise warns that its freely distributable corpus has varied formats and
licensing; it is a useful classical/folk comparison, not a pop-hit sample.

## Method

`tools/eval/midi_reference_analysis.py` parses closed `note_on`/`note_off`
events from exactly one case-insensitive `MELODY` track. It measures pitch
range and median register, adjacent melodic interval size, nonzero small-motion
fraction (one or two semitones), repeated-pitch fraction, note duration in
quarter-note MIDI gate units, and gaps between the union of note intervals. A gap of at
least one quarter is reported as a *silence proxy*, not a phrase or breath
annotation. The parser never infers a melody from the highest note, rejects
type-2/SMPTE files, records unmatched/unclosed events and overlaps, and marks
interval metrics unavailable when notes share an onset.

The test fixture in `tools/eval/test_midi_reference_analysis.py` checks explicit
track selection, beat conversion, malformed events, ambiguity and rejection of
missing labels, ascending/descending/unison separation, and union gaps under
overlap. Run it with `uv run --with pytest --with mido==1.3.3 --with openpyxl==3.1.5
pytest tools/eval/test_midi_reference_analysis.py`.

Time units are not seconds or compound-meter beats: they are ticks divided by
PPQ. Tempo changes therefore do not change these symbolic durations. Intervals
are chronological note-on differences; the small-motion numerator is absolute
one/two-semitone movement (not a key-aware diatonic-step classifier), divided by
all adjacent pairs, including unisons. The non-unison mean excludes zero
intervals. Summaries weight files equally, not notes. Union gaps exclude leading
and trailing silence, ignore sustain controllers and instrument release, and
divide by positive gaps only. A missing denominator produces null, not zero.
Small overlaps may be legato; their maximum voice count is reported, while the
explicit track label is still an assumption about melodic role, not proof of
monophonic performance. Suspect rows remain in the output and are excluded from
all summary fields.

For repeated overlapping note-ons of the same channel/pitch, note-offs are
paired in FIFO order and the overlap is reported; that is a parsing convention,
not proof of the transcriber's intended articulation. No such overlap occurs
in the retained 30-file sample. The script does not analyze motifs, chord-tone
targets or section boundaries automatically; those remain score/listening tasks.

## Observed descriptive statistics

The 30 final `001.mid`–`030.mid` files yielded 30 valid melody tracks. The
summary below is generated by the analyzer and stored in the JSON artifact;
interval fields exclude suspect or ambiguous files. Across files, the mean
(with median in parentheses) was:

| measure | mean (file median) | range across files |
| --- | ---: | ---: |
| notes per file | 329 (342) | 156–468 |
| pitch range | 17.77 (17) semitones | 9–32 |
| mean absolute non-unison interval | 2.95 (3.02) semitones | 2.08–3.68 |
| small-motion fraction (1–2 semitones, ascents and descents) | 0.468 (0.452) | 0.268–0.673 |
| repeated-pitch fraction | 0.221 (0.195) | 0.055–0.467 |
| median note duration | 0.313 (0.313) quarter notes | 0.133–0.558 |
| union gaps at least one quarter | 0.035 (0.019) of positive union gaps | 0–0.160 |

These numbers suggest questions for review: permit repeated pitches as a
rhythmic/lyric anchor; inspect larger leaps as possible targets or climaxes;
and do not equate silence with a phrase boundary. In this sample, union gaps
of a quarter or more are uncommon, so the data does not support a mandated
rest after every bar or identify where a phrase should breathe. It does *not*
establish universal thresholds, genre norms, or an ideal vocal range. The
melody tracks are transcriptions and may include quantization and editorial
choices.

## Practical use in generation and review

1. Ask for a named foreground melody and inspect it as a single line before
   generating backing. Preserve a small motif's rhythm/contour while varying
   its ending or register across the next phrase.
2. Use the statistics as diagnostics, not gates. A high leap rate may be right
   for an anthem; a low range may be intentional for an intimate verse. Flag
   an outlier for review and explain the musical reason before changing it.
3. Place breathing/rests around phrase boundaries and lyric syntax. Check that
   the preceding arrival has enough release for its instrument, and that the
   next onset is a deliberate pickup or downbeat.
4. Keep BRIDGE/countermelody and PIANO accompaniment separate from the lead.
   POP909's track design reinforces the workflow boundary: support the lead
   with contrasting rhythm/register and inspect actual notes rather than
   assuming an accompaniment recipe is musically finished.

## Limits and follow-up

### Recent transcription discovery

A bounded follow-up search also examined recent-hit transcriptions. None entered
the measured corpus; the following are discovery results, not note analyses:

| Candidate | Observed limitation on 2026-10-25 |
| --- | --- |
| [YOASOBI, Idol (MidiShow)](https://www.midishow.com/en/midi/idol-yoasobi-yoasobi-oshi-no-ko-op-midi-download-254046) | Piano-hand tracks, without an explicit main-melody label; automatic highest-note extraction would require a separate validated annotation step |
| [Billie Eilish, Birds of a Feather (MidiShow)](https://www.midishow.com/en/midi/birds-of-a-feather-billie-eilish-midi-download-205091) | File retrieval encountered a browser verification challenge; no MIDI bytes were analyzed |
| [Sabrina Carpenter, Espresso (Nonstop2k)](https://www.nonstop2k.com/midi-files/23423-sabrina-carpenter-espresso-midi.html) | Parts are described, but the download requires credits/membership; no purchase or download was performed |
| [YOASOBI, Idol (Note Block World)](https://noteblock.world/song/unKOsprJMl) | Download required login, and a standard MIDI file with labelled melody was not established |

These access and annotation observations do not show that usable recent-song
MIDIs do not exist. Fan-arrangement labels or a site's license badge would not
make the notes an official transcription of the recording. A future study can
add accessible files with explicit provenance and checked melody annotations,
retaining their transcription uncertainty.

### Scope of the numerical evidence

The sample is consecutive early-index material from a dataset published in 2020
and is not a current-chart survey. No reliable, openly redistributable
set of recent hit transcriptions was added; commercial/user-uploaded MIDI
files have uncertain rights and inconsistent track labels. Do not describe
these results as trends in 2024–2026 releases. A later study can stratify a
rights-cleared corpus by genre, meter and song section, and use the supplied
`chord_midi.txt`/beat annotations to test target-note and phrase-boundary
relationships. Such a study should retain track labels and report selection
and transcription uncertainty.
