# Grand piano across the keyboard

[日本語](grand-piano-register.ja.md)

The 2026-10-26 revision rebuilds Physical Piano's strings, hammer and soundboard and voices them
against Steinway notes from C1 to C8. The [previous calibration](grand-piano-calibration.md)
covered C3–G4 with the old structure. Measured across the whole keyboard, that structure had
four faults no choice of controls could remove:

* **One-stage decay.** A grand's note falls quickly and then rings on: at C4 mf the recorded
  fundamental drops about 40 dB/s for the first second and about 2 dB/s afterwards. The old model
  decayed at a nearly constant 9 dB/s from C3 upwards, so mid-register notes faded too slowly at
  first and far too fast later (-74 dB at 8 s, against -35 dB recorded at the noise floor).
* **No register in the losses.** Every fundamental had the same nominal T60, so bass notes died
  early and C8 rang for about six seconds.
* **Wrong spectral balance at the ends.** C1/C2 partials 3–7 sit 20–30 dB above their weakly
  radiated fundamentals; the old model had them below it. Treble second partials were 20–25 dB
  too strong, because the hammer contact was capped at a fraction of the string's period.
* **No knock.** A treble strike carries a short, broadband soundboard knock, as strong as the
  string itself at C8. The old model had none.

## The model

**Strings and bridge.** Each partial of each string of a unison is a damped rotating phasor. The
bridge absorbs a share of only the motion the strings share; motion orthogonal to the coupling
keeps the strings' own loss. Struck together, a unison drains into the soundboard quickly; the
motion left by slight mistuning and an uneven hammer rings on as the aftersound. This is
Weinreich's account of coupled piano strings. Single bass strings (below 65 Hz) carry a weakly
coupled second polarization, which gives them the same two stages. The convex mixing it replaces
damped the *differential* motion instead, which is the opposite of the physics. The projection
is passive for any absorbed share between 0 and 1, and a test checks energy never grows.

**Losses.** The aftersound decay of the fundamental follows a fitted curve from 1.3–1.8 dB/s in
the bass and middle to 14 dB/s at C7 and 43 dB/s at C8. Upper partials add internal friction in
proportion to frequency squared. Resonance Decay and Damping scale these curves, and at their
factory values reproduce them exactly. Unison mistuning is bounded in hertz, so the top octave no
longer beats at 4 Hz. Up to 96 partials run on single and paired strings, 64 on triple unisons.

**Hammer.** The felt stays on the strings for a contact time that falls from 4.3 ms at C1 to
1.9 ms at C4 and 0.4 ms at C8 at mf: in the treble, for longer than a period, which is what keeps
those notes nearly pure. The pulse is a warped power of a sine, tabulated per voicing. A short
gamma pulse from the felt's hard core grows with velocity and keeps the upper partials of a loud
strike. Velocity changes what the hammer excites but not, on its own, how loud the note is: the
strike is compared with the same hammer at mf, over eight bands weighted by the soundboard's
radiation, and its drive is scaled so loudness keeps the instrument's velocity law.

**Radiation.** The output is the force on the bridge, the slope of each mode there. A second-order
highpass at 197 Hz stands for the soundboard's poor radiation below its first modes, ahead of the
twelve-band fitted coloration. A register gain curve evens out the strings' level, as a
technician's voicing does.

**Knock.** Each strike also excites the soundboard directly: band-limited noise from a seed fixed
by the note, rising with the hammer and decaying at 100 dB/s, highpassed at 40 Hz and lowpassed at
1.3 kHz. It is mixed after the radiation filters, whose shape was fitted to the strings, and no
damper stops it. Rendering stays deterministic and independent of block size.

**Dampers and pedal.** On the piano, Release is the damper: the time it takes to remove 80 dB, as
the shared envelope does, multiplied by a curve that is longer in the bass. Keys above F6 have no
damper and ring after release. The sustain pedal lifts every damper, including those already
falling on a released note. The damper curve has no reference recording and is not fitted.

## References

The University of Iowa's [Steinway model B recordings](https://theremin.music.uiowa.edu/MISpiano.html),
performed by Evan Mazunik, supply mf notes on every C from C1 to C8, and pp/ff notes on C2, C4,
C6 and C7. pp C6 and pp C7 lie within a few dB of the recording's noise floor and are not used:
14 of the 16 locked captures enter fitting. URLs, sizes and hashes are in
[piano-register-captures.json](../tools/eval/references/piano-register-captures.json). The
grand-piano cohort's six training notes (mf/ff at C3, G3, E4) join the final mel stage. Nine of its
notes enter no fit at all: pp at C3, E3, G3, E4, G4 and mf/ff at E3 and G4. They are the held-out
check below.

## Measurements and fitting

Measurements run at 24 kHz on onset-aligned notes, every 25 ms, against each file's own noise
floor:

| Measure | What it compares |
| --- | --- |
| decay | Each of up to twelve partials' level relative to itself at 100 ms, with the recording's floor added to the render |
| envelope | Broadband level relative to 100 ms, through the same floor |
| spectrum | Partial levels over 50–400 ms relative to their total |
| level | 50–500 ms level, compared with the same-dynamic C4 through offsets frozen at the start |
| knock | Band power under 0.6 f0 at 20/60/120 ms, absolute through the same offsets, above the floor only |

A bounded coordinate search fits register anchors on the notes at each anchor only, and shared
coefficients on all, in stages: decay (decay and envelope), spectrum (spectrum and level), knock
(knock and level). Smoothness penalties keep register curves and soundboard gains from zigzagging.
A differential-evolution search over C4's three dynamics then found the pulse shape and hard-core
growth that a coordinate search had missed, and the stages were repeated from it. The level
gain curve was set from the recordings' one-second level profile and smoothed by hand; the knock
below C4 copies C4's, where partials mask it.

The project's temporal mel distance then exposed a fault the partial measures had let through:
the soundboard boosted 320–500 Hz, where C3–C4 need their fundamental to dominate their second
partial. A final stage refits hammer, strike, soundboard, highpass and Excitation Position on that
distance, over these 14 notes and the cohort's six training notes, with the level as a guard. It
moved the soundboard's peak down to 210–320 Hz and the radiation highpass from 93 to 197 Hz.
[piano-register-fit.json](../tools/eval/references/piano-register-fit.json) records the sequence and
the voicing.

## Results

Actual Rust PCM from frozen before/after workers, with factory controls.
[piano-register-evaluation.json](../tools/eval/references/piano-register-evaluation.json) holds
every note. Mean absolute distance over the 14 fitted notes:

| Measure | Before | After |
| --- | ---: | ---: |
| decay | 17.75 dB | 4.70 dB |
| envelope | 4.39 dB | 1.40 dB |
| spectrum | 8.86 dB | 4.30 dB |
| level | 7.11 dB | 1.76 dB |
| knock | 35.61 dB | 3.44 dB |

These notes are training data; the mel distance on the grand-piano cohort is the independent
check. At 48 kHz ([report](../tools/eval/references/piano-register-grand-validation.json)):

| Notes | 1 s | 3 s | 6 s |
| --- | ---: | ---: | ---: |
| Cohort training (6) | 0.1588 → 0.1533 | 0.2052 → 0.1947 | 0.2437 → 0.2260 |
| Cohort validation (12) | 0.1611 → 0.1520 | 0.2032 → 0.1855 | 0.2410 → 0.2174 |
| Held out of every fit (9) | 0.1742 → 0.1639 (-5.9%) | 0.2190 → 0.1995 (-8.9%) | 0.2595 → 0.2332 (-10.1%) |

Seven of the nine held-out notes improve in each window. pp improves most, by 5–30% at three
seconds; E3 mf/ff worsen by 17% and 12%, and among cohort training notes G3 mf worsens by 24%.
At 24 kHz the held-out reductions are 5.1, 7.8 and 8.8%.

Broadband level at mf, dB relative to the first 100 ms, recorded / before / after. Recorded
values at the noise floor are marked with a dagger:

| Note | 0.5 s | 2 s | 8 s |
| --- | ---: | ---: | ---: |
| C1 | -2.1 / -3.7 / -2.3 | -7.3 / -14.0 / -6.8 | -20.8† / -47.8 / -27.0 |
| C2 | -4.5 / -5.0 / -3.7 | -11.0 / -17.1 / -14.1 | -22.4 / -56.1 / -28.4 |
| C4 | -13.4 / -5.8 / -8.6 | -24.0 / -24.8 / -28.3 | -35.4† / -74.4 / -44.6 |
| C6 | -9.8 / -7.8 / -6.5 | -31.8 / -25.1 / -27.1 | -38.5† / -85.9 / -60.9 |

Level at mf, one-second RMS relative to C4:

| | C1 | C2 | C3 | C5 | C6 | C7 | C8 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Recorded | -1.6 | 4.7 | 5.1 | 3.3 | -5.7 | -13.4 | -15.7 |
| Before | -6.4 | -6.6 | -1.3 | -1.8 | 1.6 | 5.2 | 0.7 |
| After | 0.4 | 4.0 | 5.2 | 1.8 | -6.7 | -9.7 | -15.8 |

The output multiplier preserves the median three-second RMS of the C3–G4 notes at all three
dynamics: 0.1867 becomes 0.0776, Level stays at -12 dB. The factory Excitation Position becomes
0.2444, from the mel stage. Velocity keeps its law. Relative to 0.65,
velocities 0.35 and 0.95 give -7.0 and +4.5 dB before, at every pitch; after, -8.4/+6.1 at C2,
-7.2/+5.0 at C4 and -6.7/+5.1 at C7. The recording spans about 25 dB from pp to ff at C4; this
keeps existing songs' dynamics rather than adopting it.

With 24 voices held at 48 kHz in 256-frame blocks, the callback costs about what it did:
2.18 against 2.07 ms and 1.88 against 1.90 ms in two paired runs under background load, and
1.25 against 1.0 ms for an earlier build on an idle machine. Striking all 24 notes in one block
adds about 0.4 ms to that block. These are development-machine runs, not a latency guarantee.

## Listening and limits

`target/piano-register/audition/` holds the keyboard walk, a velocity ladder, a pedalled phrase
and staccato dampers, each old then new from identical event scores and RMS-matched, plus the
recorded mf notes alternating with the new piano. They are development output, not release assets.

One recorded piano, one microphone position and three dynamic labels constrain this voicing. There
is no reference for the dampers, the release or the pedal, and the knock's spectrum above 1.3 kHz
is unmeasured under the treble partials. Sympathetic resonance with the pedal down is not modelled.
The level match comes from a smoothed gain curve, not from the hammer or bridge. These measures
describe resemblance to recordings, not preference; listen.

## Reproduction

```powershell
cargo build --release -p auris-synth --example physical_fit_render --features piano-calibration
uv run tools/eval/piano_register.py target/piano-register/references prepare
uv run tools/eval/grand_piano.py target/piano-grand/references prepare
uv run tools/eval/piano_register.py target/piano-register/references fit `
  target/release/examples/physical_fit_render.exe start.json fit.json --stages decay,spectrum,knock
uv run tools/eval/piano_register.py target/piano-register/references fit `
  target/release/examples/physical_fit_render.exe fit-voicing.json mel.json --stages mel `
  --grand target/piano-grand/references
uv run tools/eval/piano_register.py target/piano-register/references evaluate `
  before-renderer.exe target/release/examples/physical_fit_render.exe evaluation.json
uv run tools/eval/grand_piano.py target/piano-grand/references evaluate `
  before-renderer.exe target/release/examples/physical_fit_render.exe validation.json
uv run --with pytest --with numpy --with scipy --with soundfile pytest tools/eval/test_piano_register.py
```

A start file is a `PianoVoicing` as JSON, the keys of `piano-register-fit.json`'s `voicing`; keys
starting with an underscore set instrument parameters. Keep the worker built before a change as
the baseline. Downloaded recordings stay under `target/`.
