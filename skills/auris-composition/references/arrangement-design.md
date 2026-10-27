# Arrangement design: decisions, operations, and checks

Use this reference after the score has a foreground idea and a usable form. It is a
decision aid, not a recipe or a set of genre quotas. Keep the existing foreground,
protected clips, lyrics, and intentional silences unless the brief explicitly asks for
a rewrite. The operations below are derived from production sources and adapted to
Auris's note/clip/performance tools; the source's product-specific controls are not
assumed to exist in Auris.

## Role palettes to try

These are starting palettes, not templates. Pick one whose role boundaries match the
brief, then remove or swap parts when the foreground needs room.

| Context | Palette A: intimate | Palette B: rising | Palette C: contrast |
| --- | --- | --- | --- |
| piano + vocal | piano low root + mid-register third/fifth or broken chords; voice above | sparse verse piano, bass enters on chorus, high octave answering figure | held piano voicings under phrases, short fills only in vocal gaps |
| pop band | kick/bass foundation, guitar offbeats, one pad | drums restrained in verse, bass and wide guitars open in chorus | bass stays steady while guitar changes from mute to sustained chords |
| electronic | sub/bass owns root, one mid pulse, dry lead | filtered pulse in verse, full transient layer at arrival, pad sustains between phrases | lead leaves holes for a call sample; automation carries the transition |
| game/BGM cue | ostinato plus sparse harmony, foreground motif | add register or counterline at the scene turn, then reduce for loop return | texture changes while tempo and motif remain stable for gameplay focus |

Check a palette by muting each role and naming what musical information disappears.
If two parts answer the same question at the same time, keep the clearer one or make
their entrances complementary.

### Decision: the form feels flat or crowded

Operation: subtract, delay, shorten, or thin one supporting part before adding a new
one. Let contrast come from register, articulation, duration, rhythm, lead sound, or
silence; a fader increase is only one option. A fully populated sketch can be reduced
backwards as an exploratory method, but a duration-constrained Auris project should
retain its requested bar count. Ableton describes this subtractive workflow in
[Arranging as a Subtractive Process](https://makingmusic.ableton.com/arranging-as-a-subtractive-process).

Check: solo the foreground and then restore one role at a time. The refrain should gain
an audible, purposeful change, while a deliberately sparse cue should remain sparse.
Record a no-change decision with the role and section that justify it.

### Decision: a repeat needs identity without losing the hook

Operation: preserve the motif's recognizable contour or rhythm and mutate one meaningful
feature: its answer, register, accompaniment rhythm, articulation, lead instrument, or
last cadence. Use a pickup, held note, fill, reduction, or harmonic turn only where it
prepares the next event. Ableton's [mutation over generations](https://makingmusic.ableton.com/creating-variation-2-mutation-over-generations)
and [unique events](https://makingmusic.ableton.com/unique-events) are option sets, not
requirements to vary every bar.

Check: compare the first and later statement with the lead muted, then with the lead
restored. The listener should be able to identify the shared idea and the changed
function. In a loop, audition the end directly into the beginning; a one-off fill can
become an unwanted accent on every cycle.

### Decision: an interlude or bridge has no purpose

Operation: choose whether it is functional or decorative. Functional jobs include
breathing after a dense chorus, turning harmony, answering the vocal, resetting register,
or setting up the return. A decorative figure may simply colour a stable section; it
does not need to make the listener expect a new section. Reuse a small motif through a
different rhythm, contour, timbre, octave, or call-and-response role rather than pasting
the chorus unchanged. Remove the lead briefly only when a question-and-answer effect is
intended; a sustained pad or ostinato may deliberately continue.

Check: listen to both joins (chorus→interlude and interlude→return). For a functional
interlude, verify that the turn or breath is audible and the return is legible. For a
decorative interlude, verify that it supports the existing scene and does not compete
with the foreground. Muting it is evidence to inspect, not proof that it is bad.

## Roles, register, and voicing

Give every simultaneous part a role and a register plan. A foreground line needs
temporal and spectral room; a pad can sustain harmony, a bass can define roots and
direction, a pulse can supply movement, and an answer should occupy gaps instead of
doubling every attack. Do not automatically eliminate overlap: unison, octave, and
intentional blend can be the musical point.

For a vocal or lead instrument, first try a thinner backing voicing, shorter releases,
an octave change, or a gap before reaching for EQ. Keep low-register chord stacks
playable and avoid having kick, bass, and left-hand harmony all claim the same attack.
Choose a bass line that supports the chord roots while using stepwise or purposeful
passing motion; verify that its rhythm leaves the kick's transient readable. These are
arrangement choices, not fixed frequency cutoffs.

An original 4/4 example with one chord per bar is `C | Am | F | G` and bass
`C3 dotted-half, B2 quarter | A2 dotted-half, G2 quarter | F2 dotted-half,
E2 quarter | G2 whole`. C–B–A and A–G–F connect roots; B over C, G over Am and
E over F are exposed passing/extension colours, so their placement and the melody
must justify them. The last E–G moves into the dominant; this is not a tonic ending.
Above the bass, sustained chord pairs `E4/G4 | E4/A4 | F4/A4 | G4/B4` retain
common tones or move by small intervals. This is one voicing option, not an implied
new chord map. Test these voices against the lead and kick; shorten a connection
or move an inner voice if it competes with the foreground.

iZotope documents that adding more instruments increases masking risk and recommends
solving clashes in the arrangement before surgical processing in [Unmasking Your Mix
with Neutron](https://www.izotope.com/en/learn/unmasking-your-mix-with-neutron). It also
notes that overlap can be desirable, so a meter is evidence of competition, not proof
that a part must be cut.

Decision: the lead is obscured. Operation: mute the suspected support, then change one
of its notes, octave, gate, rhythm, pan, or level and render the passage. Use dynamic
ducking or EQ only after the musical change has been considered. Check the lead with
lyrics or articulation present, not with a preview sine or an empty clip.

Decision: the low end is unstable. Operation: coordinate kick and bass note starts and
lengths; choose which element owns the deepest sustained energy; shorten or move the
other part before compression. Check the same bass note against the kick at section
changes and on the rendered stem. Preserve deliberate rub or distortion when it serves
the style.

## Instruments and performance

Select an instrument for the part's job and playable behavior. A fast repeated figure
needs a sound with a useful attack and release; a held line needs a convincing sustain
and tail. A short MIDI gate on a sustained patch does not create a sampled staccato.
For orchestral textures, hand a returning theme between families or octaves instead of
doubling the full ensemble continuously. Spitfire's [virtual orchestration guide](https://www.spitfireaudio.com/en-us/blogs/editorial/how-to-create-orchestra-music-with-virtual-instruments-a-beginners-guide)
is a source for role/range thinking, not an Auris instrument specification.

Shape phrase emphasis with deliberate velocity, gate, timing, or supported automation.
Keep ensemble attacks coherent and use rests that match the actual instrument's decay.
Do not map a tutorial's CC1, CC11, keyswitch, or “expression” label onto Auris without
checking the instrument metadata and effective result. Spitfire explains why dynamics,
vibrato, and expression can be distinct controls in [this support note](https://support.spitfireaudio.com/en/articles/11816091-what-is-the-difference-between-dynamics-vibrato-and-expression);
the parameter names and semantics are library-specific.

Check: read back the effective performance settings, render the affected phrase, and
inspect that the note endings and arrival emphasis changed as intended. If the host
reports a bypass or unsupported control, preserve the score and state that limitation.

## Space, effects, and automation

Choose the processor by the change wanted. EQ changes the spectral balance; a
compressor changes the level envelope, with attack/release affecting how transients
and sustain relate; saturation changes colour and can add harmonics. These are
audition hypotheses, not mandatory insert chains. Compare at similar output level:
a louder compressed or saturated version is not automatically a clearer one. Keep
the effect when it serves the source in context, including intentionally distorted
or pumping music. The artist's decision takes precedence over a clean-mix template.

Treat effects as part of the arrangement's depth and motion, not a substitute for
missing parts or conflicting notes. A delay can preserve a lead's position while adding
space; dense reverb can blur diction and attacks. Sound On Sound discusses using delay
when a mix is crowded, filtering the return, and using pre-delay to keep a source close
in [Making Space In Your Mix](https://www.soundonsound.com/techniques/making-space-your-mix)
and [Creating a Sense of Depth](https://www.soundonsound.com/techniques/creating-sense-depth-your-mix).

Decision: the lead needs depth but must stay intelligible. Operation: try a send or
short/tempo-related delay, filter the return, and compare with a reverb send. Adjust
pre-delay until the direct consonant/transient remains distinct from its tail; do not
copy a millisecond value between tempos, voices, or room sizes. Check the dry lead and
effect return separately and recheck the final phrase for accumulated tail.

Decision: a section should recede without a conspicuous volume drop. Operation: automate
a low-pass or effect send, reduce density, or move a support part in register. Sound On
Sound describes low-pass automation as an alternative to dropping levels in [Creating a
Sense of Depth](https://www.soundonsound.com/techniques/creating-sense-depth-your-mix).
Check that the movement supports the section boundary and does not remove an essential
melody overtone or vocal consonant.

Try a shared room for cohesion or deliberately contrasting spaces for separation.
Filter reverb/delay returns when their high-frequency repetition distracts from the dry
source; special spring or extreme effects are accents, not default depth. Sound On
Sound recommends subtle, coherent depth and cautions that overly bright/loud delays
sound like an effect in [Five Biggest Mixing Mistakes](https://www.soundonsound.com/techniques/5-biggest-mixing-mistakes-and-how-avoid-them).

Check: bypass effects at matched level, then restore them and inspect the foreground,
section transitions, and loop join. Decide whether the effect is supporting the part
or is itself the rhythmic/timbral idea: an echo pattern or distorted texture can be
essential. Revise the notes only when their role fails the brief, not merely because
the dry signal sounds incomplete. Report mix measurements separately from a listening
judgment; LUFS or peak values cannot establish depth or clarity.

For Auris tool discovery, protected clips, revision safety, and delivery checks, use the
operational [arrangement workflow](arrangement.md). This reference supplies the musical
choices to make before invoking those operations; it does not replace their schemas or
verification rules.

These are adaptations of the linked sources, consulted on 2026-10-25. Contemporary
producer practice also warns against universal processing recipes: in Paul Tingen's
[September 2024 interview with Julian Bunetta](https://www.soundonsound.com/techniques/inside-track-julian-bunetta),
the producer of "Espresso" describes experimental processing alongside close
attention to melody and lyrics. The useful inference for Auris is to audition a
purposeful treatment and preserve the musical idea, not copy his plugin chain.
